use anyhow::{Result, bail};

use crate::caddy;
use crate::cloudflare;
use crate::compose;
use crate::config::{DeployStrategy, Fleet, ResolvedApp};
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(fleet: &Fleet, app_filter: Option<&str>) -> Result<()> {
    let apps: Vec<&ResolvedApp> = if let Some(name) = app_filter {
        let app = fleet
            .apps
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown app: {}", name))?;
        vec![app]
    } else {
        fleet.apps.values().collect()
    };

    let needed_servers: std::collections::HashSet<_> = apps
        .iter()
        .flat_map(|a| a.servers.iter())
        .collect();

    let servers_to_connect: std::collections::HashMap<_, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| needed_servers.contains(name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let sp = ui::spinner("Connecting to servers...");
    let pool = SshPool::connect(&servers_to_connect).await?;
    sp.finish_and_clear();

    for app in &apps {
        deploy_app(fleet, app, &pool).await?;
    }

    pool.close().await?;
    ui::success("Deploy complete");
    Ok(())
}

async fn deploy_app(fleet: &Fleet, app: &ResolvedApp, pool: &SshPool) -> Result<()> {
    if app.servers.is_empty() {
        bail!("App '{}' has no servers assigned", app.name);
    }

    println!();
    ui::header(&format!("Deploying {}", app.name));

    let compose_yaml = compose::generate(app);
    let env_content = compose::generate_env(app);
    let caddy_fragment = caddy::generate(app);

    for server_name in &app.servers {
        let sp = ui::spinner(&format!("  {} → uploading files...", server_name));

        let app_dir = format!("/opt/flow/{}", app.name);

        pool.exec(server_name, &format!("sudo mkdir -p {}", app_dir))
            .await?;

        let compose_path = format!("{}/docker-compose.yml", app_dir);
        pool.upload_file(server_name, &compose_path, &compose_yaml)
            .await?;

        if !env_content.trim().is_empty() {
            let env_path = format!("{}/.env", app_dir);
            pool.upload_file(server_name, &env_path, &env_content)
                .await?;
            pool.exec(server_name, &format!("chmod 600 {}", env_path))
                .await?;
        }

        sp.finish_and_clear();

        let sp = ui::spinner(&format!("  {} → pulling images...", server_name));
        pool.exec(
            server_name,
            &format!("cd {} && docker compose pull", app_dir),
        )
        .await?;
        sp.finish_and_clear();

        let sp = ui::spinner(&format!("  {} → deploying...", server_name));
        match app.deploy_strategy {
            DeployStrategy::Rolling => {
                pool.exec(
                    server_name,
                    &format!("docker rollout {} -f {}/docker-compose.yml", app.name, app_dir),
                )
                .await?;
            }
            DeployStrategy::Recreate => {
                pool.exec(
                    server_name,
                    &format!("cd {} && docker compose up -d", app_dir),
                )
                .await?;
            }
        }
        sp.finish_and_clear();

        if let Some(ref fragment) = caddy_fragment {
            let sp = ui::spinner(&format!("  {} → updating Caddy...", server_name));
            let caddy_sites_dir = "/opt/flow/caddy/sites";
            pool.exec(server_name, &format!("sudo mkdir -p {}", caddy_sites_dir))
                .await?;
            let caddy_path = format!("{}/{}", caddy_sites_dir, app.name);
            pool.upload_file(server_name, &caddy_path, fragment).await?;
            pool.exec(
                server_name,
                "cd /opt/flow/caddy && docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile",
            )
            .await?;
            sp.finish_and_clear();
        }

        ui::success(&format!("  {} → {}", server_name, app.name));
    }

    if let Some(ref routing) = app.routing {
        if !routing.routes.is_empty() {
            if let Some(ref cf_token) = fleet.secrets.cloudflare_api_token {
                let sp = ui::spinner("  Ensuring DNS records...");
                for server_name in &app.servers {
                    let server_ip = pool
                        .exec(server_name, "hostname -I | awk '{print $1}'")
                        .await?;
                    let server_ip = server_ip.trim();

                    for route in &routing.routes {
                        cloudflare::ensure_dns_record(cf_token, route, server_ip).await?;
                    }
                }
                sp.finish_and_clear();
                ui::success("  DNS records ensured");
            }
        }
    }

    Ok(())
}
