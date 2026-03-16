use crate::cli::DbCommand;
use crate::config::{Fleet, ResolvedApp, ResolvedSidecar, Server};
use crate::ssh::SshPool;
use crate::ui;
use anyhow::{Result, bail};

pub fn find_postgres(app: &ResolvedApp) -> Result<&ResolvedSidecar> {
    app.services
        .iter()
        .find(|svc| svc.image.starts_with("postgres:"))
        .ok_or_else(|| anyhow::anyhow!("App '{}' has no postgres service", app.name))
}

fn resolve_app_name(fleet: &Fleet, app: Option<&str>) -> Result<String> {
    if let Some(name) = app {
        return Ok(name.to_string());
    }
    for (name, app) in &fleet.apps {
        if app
            .services
            .iter()
            .any(|s| s.image.starts_with("postgres:"))
        {
            return Ok(name.clone());
        }
    }
    bail!("No app with a postgres service found in fleet.toml");
}

fn pg_user(svc: &ResolvedSidecar) -> &str {
    svc.env
        .get("POSTGRES_USER")
        .map_or("postgres", String::as_str)
}

fn pg_db(svc: &ResolvedSidecar) -> &str {
    svc.env
        .get("POSTGRES_DB")
        .map_or("postgres", String::as_str)
}

fn resolve_server<'a>(
    fleet: &'a Fleet,
    app: &ResolvedApp,
    server_filter: Option<&str>,
) -> Result<(String, &'a Server)> {
    if app.servers.is_empty() {
        bail!("App '{}' has no servers assigned", app.name);
    }

    let server_name = if let Some(s) = server_filter {
        if !app.servers.contains(&s.to_string()) {
            bail!("App '{}' is not deployed to server '{s}'", app.name);
        }
        s.to_string()
    } else {
        if app.servers.len() > 1 {
            eprintln!(
                "Note: {} is on {} servers, using {} (use --server to pick)",
                app.name,
                app.servers.len(),
                app.servers[0]
            );
        }
        app.servers[0].clone()
    };

    let server = fleet
        .servers
        .get(&server_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown server: {server_name}"))?;

    Ok((server_name, server))
}

pub async fn run(fleet: &Fleet, command: DbCommand) -> Result<()> {
    match command {
        DbCommand::Shell { app, server } => {
            let name = resolve_app_name(fleet, app.as_deref())?;
            shell(fleet, &name, server.as_deref())
        }
        DbCommand::Dump {
            app,
            output,
            server,
        } => {
            let name = resolve_app_name(fleet, app.as_deref())?;
            dump(fleet, &name, output.as_deref(), server.as_deref()).await
        }
        DbCommand::Restore {
            app,
            file,
            yes,
            server,
        } => {
            let name = resolve_app_name(fleet, app.as_deref())?;
            restore(fleet, &name, &file, yes, server.as_deref()).await
        }
        DbCommand::List { app, server } => {
            let name = resolve_app_name(fleet, app.as_deref())?;
            list(fleet, &name, server.as_deref()).await
        }
    }
}

fn shell(fleet: &Fleet, app_name: &str, server_filter: Option<&str>) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;
    let svc = find_postgres(app)?;
    let (_, server) = resolve_server(fleet, app, server_filter)?;

    let ssh_target = server.ip.as_deref().unwrap_or(&server.host);
    let docker_cmd = format!(
        "cd /opt/flow/{} && docker compose exec {} psql -U {} -d {}",
        app.name,
        svc.name,
        pg_user(svc),
        pg_db(svc)
    );

    let status = std::process::Command::new("ssh")
        .args([
            "-t",
            &format!("{}@{}", server.user, ssh_target),
            &docker_cmd,
        ])
        .status()?;

    if !status.success() {
        bail!("psql exited with {status}");
    }
    Ok(())
}

async fn dump(
    fleet: &Fleet,
    app_name: &str,
    output: Option<&str>,
    server_filter: Option<&str>,
) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;
    let svc = find_postgres(app)?;
    let (server_name, server) = resolve_server(fleet, app, server_filter)?;

    let default_name = format!("{app_name}.sql.gz");
    let filename = output.unwrap_or(&default_name);

    let sp = ui::spinner(&format!("Connecting to {server_name}..."));
    let pool = SshPool::connect_one(&server_name, server).await?;
    sp.finish_and_clear();

    let cmd = format!(
        "cd /opt/flow/{} && docker compose exec -T {} pg_dump -U {} {} | gzip",
        app.name,
        svc.name,
        pg_user(svc),
        pg_db(svc)
    );

    let sp = ui::spinner(&format!("Dumping {} database...", app.name));
    let mut child = pool.exec_streaming(&server_name, &cmd).await?;
    let stdout = child
        .stdout()
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture stdout"))?;

    let mut reader = tokio::io::BufReader::new(stdout);
    let mut file = tokio::fs::File::create(&filename).await?;
    tokio::io::copy(&mut reader, &mut file).await?;
    child.wait().await?;
    sp.finish_and_clear();

    let metadata = std::fs::metadata(filename)?;
    let size_kb = metadata.len() / 1024;
    ui::success(&format!("Saved to {filename} ({size_kb} KB)"));

    pool.close().await?;
    Ok(())
}

async fn restore(
    fleet: &Fleet,
    app_name: &str,
    file_path: &str,
    yes: bool,
    server_filter: Option<&str>,
) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;
    let svc = find_postgres(app)?;
    let (_, server) = resolve_server(fleet, app, server_filter)?;

    if !std::path::Path::new(file_path).exists() {
        bail!("File not found: {file_path}");
    }

    if !yes
        && !ui::confirm(&format!(
            "Restore {file_path} to {app_name}? This will overwrite the database. (y/N)"
        ))
    {
        return Ok(());
    }

    let is_gzipped = std::path::Path::new(file_path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    let ssh_target = server.ip.as_deref().unwrap_or(&server.host);

    let docker_cmd = if is_gzipped {
        format!(
            "cd /opt/flow/{} && gunzip | docker compose exec -T {} psql -U {} -d {}",
            app.name,
            svc.name,
            pg_user(svc),
            pg_db(svc)
        )
    } else {
        format!(
            "cd /opt/flow/{} && docker compose exec -T {} psql -U {} -d {}",
            app.name,
            svc.name,
            pg_user(svc),
            pg_db(svc)
        )
    };

    let sp = ui::spinner(&format!("Restoring to {app_name}..."));

    let file_data = std::fs::read(file_path)?;
    let mut child = tokio::process::Command::new("ssh")
        .args([&format!("{}@{}", server.user, ssh_target), &docker_cmd])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(&file_data).await?;
        drop(stdin);
    }

    let output = child.wait_with_output().await?;
    sp.finish_and_clear();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Restore failed: {stderr}");
    }

    ui::success(&format!("Restored {file_path} to {app_name}"));
    Ok(())
}

async fn list(fleet: &Fleet, app_name: &str, server_filter: Option<&str>) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;
    find_postgres(app)?;
    let (server_name, server) = resolve_server(fleet, app, server_filter)?;

    let sp = ui::spinner(&format!("Connecting to {server_name}..."));
    let pool = SshPool::connect_one(&server_name, server).await?;
    sp.finish_and_clear();

    let cmd = format!(
        "find /opt/flow/{}/backups -name '*.sql.gz' -exec ls -lh {{}} \\; 2>/dev/null | sort -k6,7r",
        app.name
    );

    let output = pool.exec(&server_name, &cmd).await?;

    if output.trim().is_empty() {
        ui::error("No backups found");
    } else {
        println!("{output}");
    }

    pool.close().await?;
    Ok(())
}
