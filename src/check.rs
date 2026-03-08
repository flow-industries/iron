use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};

use crate::config::{Fleet, ResolvedApp, Server};
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(fleet: &Fleet, server_filter: Option<&str>) -> Result<()> {
    let filtered: HashMap<String, Server> = fleet
        .servers
        .iter()
        .filter(|(name, _)| server_filter.is_none() || server_filter == Some(name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if filtered.is_empty() {
        anyhow::bail!("No matching server found");
    }

    let apps_by_server = build_server_app_map(fleet, &filtered);

    let sp = ui::spinner("Connecting...");
    let pool = SshPool::connect(&filtered).await?;
    sp.finish_and_clear();

    for (server_name, server) in &filtered {
        let display = match &server.ip {
            Some(ip) => format!("{server_name} ({} / {ip})", server.host),
            None => format!("{server_name} ({})", server.host),
        };
        ui::header(&display);

        if let Err(e) = crate::server::deploy_infra(
            &pool,
            server_name,
            &fleet.network,
            fleet.secrets.ghcr_username.as_deref(),
            fleet.secrets.ghcr_token.as_deref(),
        )
        .await
        {
            ui::error(&format!("Infra error: {e}"));
        }

        let apps = apps_by_server
            .get(server_name.as_str())
            .cloned()
            .unwrap_or_default();

        if let Err(e) = check_server(&pool, server_name, &apps, fleet).await {
            ui::error(&format!("SSH error: {e}"));
        }
    }

    check_dns(fleet, &filtered).await;

    let _ = pool.close().await;
    Ok(())
}

fn build_server_app_map<'a>(
    fleet: &'a Fleet,
    filtered: &HashMap<String, Server>,
) -> HashMap<&'a str, Vec<&'a ResolvedApp>> {
    let mut map: HashMap<&str, Vec<&ResolvedApp>> = HashMap::new();
    for app in fleet.apps.values() {
        for server_name in &app.servers {
            if filtered.contains_key(server_name.as_str()) {
                map.entry(server_name.as_str()).or_default().push(app);
            }
        }
    }
    map
}

fn expected_containers(app: &ResolvedApp) -> Vec<(String, &str)> {
    let mut names = vec![(format!("{}-{}-1", app.name, app.name), app.name.as_str())];
    for svc in &app.services {
        names.push((format!("{}-{}-1", app.name, svc.name), svc.name.as_str()));
    }
    names
}

async fn check_server(
    pool: &SshPool,
    server: &str,
    apps: &[&ResolvedApp],
    fleet: &Fleet,
) -> Result<()> {
    check_containers(pool, server, apps).await?;
    check_caddy(pool, server, apps).await?;
    check_stale(pool, server, apps, fleet).await?;
    Ok(())
}

async fn check_containers(pool: &SshPool, server: &str, apps: &[&ResolvedApp]) -> Result<()> {
    let output = pool
        .exec(server, "docker ps --format '{{.Names}}\t{{.Status}}'")
        .await
        .context("Failed to list containers")?;

    let running: HashMap<&str, &str> = output
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();

    println!();
    for app in apps {
        for (container, label) in expected_containers(app) {
            match running.get(container.as_str()) {
                Some(status) if status.starts_with("Up") => {
                    ui::success(&format!("{label} running"));
                }
                Some(status) => {
                    ui::error(&format!("{label} not running ({status})"));
                }
                None => {
                    ui::error(&format!("{label} missing"));
                }
            }
        }
    }

    Ok(())
}

async fn check_caddy(pool: &SshPool, server: &str, apps: &[&ResolvedApp]) -> Result<()> {
    let expected: HashSet<&str> = apps
        .iter()
        .filter(|a| a.routing.is_some())
        .map(|a| a.name.as_str())
        .collect();

    let output = pool
        .exec(server, "ls -1 /opt/flow/caddy/sites/ 2>/dev/null")
        .await
        .unwrap_or_default();

    let on_disk: HashSet<&str> = output.lines().filter(|l| !l.is_empty()).collect();

    if expected.is_empty() && on_disk.is_empty() {
        return Ok(());
    }

    println!();
    for name in &expected {
        if on_disk.contains(name) {
            ui::success(&format!("caddy: {name}"));
        } else {
            ui::error(&format!("caddy: {name} missing"));
        }
    }

    for name in &on_disk {
        if !expected.contains(name) {
            ui::error(&format!("caddy: {name} stale"));
        }
    }

    Ok(())
}

async fn check_stale(
    pool: &SshPool,
    server: &str,
    apps: &[&ResolvedApp],
    fleet: &Fleet,
) -> Result<()> {
    let expected: HashSet<&str> = apps.iter().map(|a| a.name.as_str()).collect();

    let output = pool
        .exec(server, "ls -1 /opt/flow/ 2>/dev/null")
        .await
        .unwrap_or_default();

    let on_disk: Vec<&str> = output
        .lines()
        .filter(|l| !l.is_empty() && *l != "caddy" && *l != "wud")
        .collect();

    let mut found_stale = false;
    for dir in &on_disk {
        if expected.contains(dir) {
            continue;
        }
        found_stale = true;
        if fleet.apps.contains_key(*dir) {
            ui::error(&format!(
                "stale: /opt/flow/{dir} (assigned to different server)"
            ));
        } else {
            ui::error(&format!("stale: /opt/flow/{dir} (not in fleet.toml)"));
        }
    }

    if !found_stale && !on_disk.is_empty() {
        println!();
        ui::success("no stale apps");
    }

    Ok(())
}

async fn check_dns(fleet: &Fleet, filtered: &HashMap<String, Server>) {
    let cf_token = match fleet.secrets.cloudflare_api_token {
        Some(ref t) if !t.is_empty() => t,
        _ => return,
    };

    let mut routes: HashMap<&str, HashSet<&str>> = HashMap::new();
    for app in fleet.apps.values() {
        let Some(ref routing) = app.routing else {
            continue;
        };
        for server_name in &app.servers {
            if !filtered.contains_key(server_name.as_str()) {
                continue;
            }
            if let Some(server) = fleet.servers.get(server_name.as_str()) {
                if let Some(ref ip) = server.ip {
                    for route in &routing.routes {
                        routes
                            .entry(route.as_str())
                            .or_default()
                            .insert(ip.as_str());
                    }
                }
            }
        }
    }

    if routes.is_empty() {
        return;
    }

    ui::header("DNS");

    let client = reqwest::Client::new();
    let mut zone_cache: HashMap<String, Option<String>> = HashMap::new();

    for (hostname, valid_ips) in &routes {
        let zone_name = crate::cloudflare::extract_zone(hostname);

        let zone_id = if let Some(cached) = zone_cache.get(&zone_name) {
            cached.clone()
        } else {
            let id = crate::cloudflare::get_zone_id(&client, cf_token, &zone_name)
                .await
                .ok();
            zone_cache.insert(zone_name.clone(), id.clone());
            id
        };

        let Some(zone_id) = zone_id else {
            ui::error(&format!("{hostname} (zone {zone_name} not found)"));
            continue;
        };

        match crate::cloudflare::get_record(&client, cf_token, &zone_id, hostname).await {
            Ok(Some(record)) if valid_ips.contains(record.content.as_str()) => {
                ui::success(&format!("{hostname} → {}", record.content));
            }
            Ok(Some(record)) => {
                let expected: Vec<&str> = valid_ips.iter().copied().collect();
                ui::error(&format!(
                    "{hostname} → {} (expected {})",
                    record.content,
                    expected.join(" or ")
                ));
            }
            Ok(None) => {
                ui::error(&format!("{hostname} missing"));
            }
            Err(e) => {
                ui::error(&format!("{hostname} ({e})"));
            }
        }
    }
}
