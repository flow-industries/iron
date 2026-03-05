use std::path::Path;

use anyhow::{Context, Result};

use crate::cloudflare;
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(config_path: &str, app_name: &str, skip_confirm: bool) -> Result<()> {
    let fleet = crate::config::load(config_path)?;

    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;

    if !skip_confirm {
        println!("This will remove '{app_name}':");
        println!("  Servers: {}", app.servers.join(", "));
        if let Some(ref routing) = app.routing {
            if !routing.routes.is_empty() {
                println!("  Routes:  {}", routing.routes.join(", "));
            }
        }
        println!();
        if !ui::confirm("Are you sure? (y/N)") {
            println!("Aborted.");
            return Ok(());
        }
    }

    let servers_to_connect: std::collections::HashMap<_, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| app.servers.contains(name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let sp = ui::spinner("Connecting to servers...");
    let pool = SshPool::connect(&servers_to_connect).await?;
    sp.finish_and_clear();

    let app_dir = format!("/opt/flow/{}", app.name);
    let has_routing = app.routing.as_ref().is_some_and(|r| !r.routes.is_empty());

    for server_name in &app.servers {
        let sp = ui::spinner(&format!("{server_name} → stopping containers..."));
        pool.exec(
            server_name,
            &format!("cd {app_dir} && docker compose down 2>/dev/null || true"),
        )
        .await?;
        sp.finish_and_clear();

        if has_routing {
            let sp = ui::spinner(&format!("{server_name} → removing Caddy config..."));
            pool.exec(
                server_name,
                &format!("sudo rm -f /opt/flow/caddy/sites/{}", app.name),
            )
            .await?;
            pool.exec(
                server_name,
                "cd /opt/flow/caddy && docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile",
            )
            .await?;
            sp.finish_and_clear();
        }

        let sp = ui::spinner(&format!("{server_name} → removing app files..."));
        pool.exec(server_name, &format!("sudo rm -rf {app_dir}"))
            .await?;
        sp.finish_and_clear();

        ui::success(&format!("{server_name} → {app_name} removed"));
    }

    if let Some(ref routing) = app.routing {
        if !routing.routes.is_empty() {
            if let Some(ref cf_token) = fleet.secrets.cloudflare_api_token {
                let sp = ui::spinner("Deleting DNS records...");
                for route in &routing.routes {
                    cloudflare::delete_dns_record(cf_token, route).await?;
                }
                sp.finish_and_clear();
                ui::success("DNS records deleted");
            }
        }
    }

    pool.close().await?;

    let config = Path::new(config_path);
    remove_app_from_config(config, app_name)?;
    remove_app_from_env_config(config, app_name)?;

    ui::success(&format!("{app_name} fully removed"));
    Ok(())
}

pub fn remove_app_from_config(config_path: &Path, name: &str) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let apps = doc
        .get_mut("apps")
        .and_then(|a| a.as_table_mut())
        .context("'apps' table not found")?;

    apps.remove(name);

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

fn remove_app_from_env_config(config_path: &Path, name: &str) -> Result<()> {
    let env_path = config_path.with_file_name("fleet.env.toml");
    if !env_path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&env_path)
        .with_context(|| format!("Failed to read {}", env_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", env_path.display()))?;

    if let Some(apps) = doc.get_mut("apps").and_then(|a| a.as_table_mut()) {
        apps.remove(name);
    }

    std::fs::write(&env_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", env_path.display()))?;
    Ok(())
}
