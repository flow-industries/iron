use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::ServerCommand;
use crate::config::{EnvConfig, FleetConfig, Server};
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(config_path: &str, command: ServerCommand) -> Result<()> {
    match command {
        ServerCommand::Add {
            name,
            ip,
            host,
            user,
            ssh_user,
        } => add(config_path, &name, &ip, host.as_deref(), &user, &ssh_user).await,
        ServerCommand::Remove { name } => remove(config_path, &name),
        ServerCommand::Check { name } => check(config_path, name.as_deref()).await,
    }
}

async fn add(
    config_path: &str,
    name: &str,
    ip: &str,
    host_override: Option<&str>,
    user: &str,
    ssh_user: &str,
) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if config.servers.contains_key(name) {
        bail!("Server '{name}' already exists");
    }

    let hostname = if let Some(h) = host_override {
        h.to_string()
    } else {
        let domain = config.domain.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot derive hostname: no 'domain' in fleet.toml (use --host to specify)"
            )
        })?;
        format!("{name}.{domain}")
    };

    let env_path = config_path.with_file_name("fleet.env.toml");
    let (ghcr_token, cf_token) = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        let env_config: EnvConfig = toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?;
        (
            env_config.fleet.ghcr_token.filter(|t| !t.is_empty()),
            env_config
                .fleet
                .cloudflare_api_token
                .filter(|t| !t.is_empty()),
        )
    } else {
        (None, None)
    };

    let cf_token = cf_token.ok_or_else(|| {
        anyhow::anyhow!("Cannot create DNS record: cloudflare_api_token not set in fleet.env.toml")
    })?;

    let sp = ui::spinner(&format!("Creating DNS record {hostname} → {ip}..."));
    crate::cloudflare::ensure_dns_record(&cf_token, &hostname, ip).await?;
    sp.finish_and_clear();
    ui::success(&format!("{hostname} → {ip}"));

    let ansible_dir = config_path
        .parent()
        .unwrap_or(Path::new("."))
        .join("ansible");
    if !ansible_dir.join("setup.yml").exists() {
        bail!(
            "Ansible playbook not found at {}",
            ansible_dir.join("setup.yml").display()
        );
    }

    let sp = ui::spinner("Running Ansible setup...");
    let mut cmd = tokio::process::Command::new("ansible-playbook");
    cmd.arg("ansible/setup.yml")
        .arg("-i")
        .arg(format!("{ip},"))
        .arg("-u")
        .arg(ssh_user)
        .current_dir(config_path.parent().unwrap_or(Path::new(".")));

    if let Some(ref token) = ghcr_token {
        cmd.arg("-e").arg(format!("ghcr_token={token}"));
    }

    let status = cmd
        .status()
        .await
        .context("Failed to run ansible-playbook")?;
    sp.finish_and_clear();

    if !status.success() {
        bail!("Ansible setup failed (exit code: {status})");
    }

    write_server_to_config(config_path, name, &hostname, ip, user)?;
    ui::success(&format!("Server '{name}' added and bootstrapped"));
    Ok(())
}

fn remove(config_path: &str, name: &str) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if !config.servers.contains_key(name) {
        bail!("Server '{name}' does not exist");
    }

    let referencing_apps: Vec<&String> = config
        .apps
        .iter()
        .filter(|(_, app)| app.servers.contains(&name.to_string()))
        .map(|(app_name, _)| app_name)
        .collect();

    if !referencing_apps.is_empty() {
        let app_list: Vec<&str> = referencing_apps.iter().map(|s| s.as_str()).collect();
        bail!(
            "Cannot remove server '{}': referenced by apps: {}",
            name,
            app_list.join(", ")
        );
    }

    remove_server_from_config(config_path, name)?;
    ui::success(&format!("Server '{name}' removed"));
    Ok(())
}

async fn check(config_path: &str, name: Option<&str>) -> Result<()> {
    let fleet = crate::config::load(config_path)?;

    let servers_to_check: Vec<(String, Server)> = if let Some(name) = name {
        let server = fleet
            .servers
            .get(name)
            .with_context(|| format!("Server '{name}' not found"))?;
        vec![(name.to_string(), server.clone())]
    } else {
        fleet
            .servers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    for (name, server) in &servers_to_check {
        let display = match &server.ip {
            Some(ip) => format!("Server: {} ({} / {})", name, server.host, ip),
            None => format!("Server: {} ({})", name, server.host),
        };
        ui::header(&display);

        let pool = match SshPool::connect_one(name, server).await {
            Ok(pool) => {
                ui::success("SSH connection");
                pool
            }
            Err(e) => {
                ui::error(&format!("SSH connection: {e}"));
                continue;
            }
        };

        run_check(
            &pool,
            name,
            "Docker running",
            "docker info --format '{{.ServerVersion}}'",
        )
        .await;
        run_check(
            &pool,
            name,
            "docker-rollout",
            "test -x /usr/libexec/docker/cli-plugins/docker-rollout",
        )
        .await;
        run_check(&pool, name, "Deploy directory", "test -d /opt/flow").await;
        run_check(
            &pool,
            name,
            "Flow network",
            "docker network inspect flow --format '{{.Name}}'",
        )
        .await;

        let _ = pool.close().await;
    }

    Ok(())
}

async fn run_check(pool: &SshPool, server: &str, label: &str, cmd: &str) {
    match pool.exec(server, cmd).await {
        Ok(_) => ui::success(label),
        Err(_) => ui::error(label),
    }
}

pub fn write_server_to_config(
    config_path: &Path,
    name: &str,
    host: &str,
    ip: &str,
    user: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let servers = doc
        .entry("servers")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("'servers' is not a table")?;

    let mut server_table = toml_edit::Table::new();
    server_table.insert("host", toml_edit::value(host));
    server_table.insert("ip", toml_edit::value(ip));
    server_table.insert("user", toml_edit::value(user));
    servers.insert(name, toml_edit::Item::Table(server_table));

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn remove_server_from_config(config_path: &Path, name: &str) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let servers = doc
        .get_mut("servers")
        .and_then(|s| s.as_table_mut())
        .context("'servers' table not found")?;

    servers.remove(name);

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}
