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
            .ok_or_else(|| anyhow::anyhow!("Unknown app: {name}"))?;
        vec![app]
    } else {
        fleet.apps.values().collect()
    };

    let needed_servers: std::collections::HashSet<_> =
        apps.iter().flat_map(|a| a.servers.iter()).collect();

    let servers_to_connect: std::collections::HashMap<_, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| needed_servers.contains(name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let sp = ui::spinner("Connecting to servers...");
    let pool = SshPool::connect(&servers_to_connect).await?;
    sp.finish_and_clear();

    let sp = ui::spinner("Ensuring Docker network...");
    for server_name in &needed_servers {
        pool.exec(
            server_name,
            &format!(
                "docker network create {} 2>/dev/null || true",
                fleet.network
            ),
        )
        .await?;
    }
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

    let compose_yaml = compose::generate(app, &fleet.network);
    let env_content = compose::generate_env(app);
    let caddy_fragment = caddy::generate(app);

    for server_name in &app.servers {
        let sp = ui::spinner(&format!("  {server_name} → uploading files..."));

        let app_dir = format!("/opt/flow/{}", app.name);

        pool.exec(server_name, &format!("sudo mkdir -p {app_dir}"))
            .await?;

        let compose_path = format!("{app_dir}/docker-compose.yml");
        pool.upload_file(server_name, &compose_path, &compose_yaml)
            .await?;

        if !env_content.trim().is_empty() {
            let env_path = format!("{app_dir}/.env");
            pool.upload_file(server_name, &env_path, &env_content)
                .await?;
            pool.exec(server_name, &format!("chmod 600 {env_path}"))
                .await?;
        }

        sp.finish_and_clear();

        let sp = ui::spinner(&format!("  {server_name} → pulling images..."));
        pool.exec(server_name, &format!("cd {app_dir} && docker compose pull"))
            .await?;
        sp.finish_and_clear();

        let sp = ui::spinner(&format!("  {server_name} → deploying..."));
        match app.deploy_strategy {
            DeployStrategy::Rolling => {
                pool.exec(
                    server_name,
                    &format!(
                        "docker rollout {} -f {}/docker-compose.yml",
                        app.name, app_dir
                    ),
                )
                .await?;
            }
            DeployStrategy::Recreate => {
                pool.exec(
                    server_name,
                    &format!("cd {app_dir} && docker compose up -d"),
                )
                .await?;
            }
        }
        sp.finish_and_clear();

        if let Some(ref fragment) = caddy_fragment {
            let sp = ui::spinner(&format!("  {server_name} → updating Caddy..."));
            let caddy_sites_dir = "/opt/flow/caddy/sites";
            pool.exec(server_name, &format!("sudo mkdir -p {caddy_sites_dir}"))
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
                    let server = &fleet.servers[server_name];
                    let server_ip = match &server.ip {
                        Some(ip) => ip.clone(),
                        None => pool
                            .exec(server_name, "hostname -I | awk '{print $1}'")
                            .await?
                            .trim()
                            .to_string(),
                    };

                    for route in &routing.routes {
                        cloudflare::ensure_dns_record(cf_token, route, &server_ip).await?;
                    }
                }
                sp.finish_and_clear();
                ui::success("  DNS records ensured");
            }
        }
    }

    Ok(())
}
