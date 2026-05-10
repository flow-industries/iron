use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::caddy;
use crate::cloudflare;
use crate::compose;
use crate::config::{DeployStrategy, Fleet, ResolvedApp, Runner};
use crate::notify::{Event, Notifier};
use crate::r2;
use crate::runner;
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(
    config_path: &str,
    fleet: &Fleet,
    app_filter: Option<&str>,
    force: bool,
    notifier: &Notifier,
) -> Result<()> {
    let (apps, runners): (Vec<&ResolvedApp>, Vec<(&str, &Runner)>) = if let Some(name) = app_filter
    {
        if let Some(app) = fleet.apps.get(name) {
            (vec![app], vec![])
        } else if let Some(runner) = fleet.runners.get(name) {
            (vec![], vec![(name, runner)])
        } else {
            bail!("Unknown app or runner: {name}");
        }
    } else {
        let apps = fleet.apps.values().collect();
        let runners = fleet.runners.iter().map(|(k, v)| (k.as_str(), v)).collect();
        (apps, runners)
    };

    let mut needed_servers: std::collections::HashSet<_> =
        apps.iter().flat_map(|a| a.servers.iter()).collect();
    for (_, r) in &runners {
        needed_servers.insert(&r.server);
    }

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

    if let (Some(username), Some(token)) = (&fleet.secrets.gh_username, &fleet.secrets.gh_token) {
        let sp = ui::spinner("Logging in to GHCR...");
        for server_name in &needed_servers {
            pool.exec(
                server_name,
                &format!("echo '{token}' | docker login ghcr.io -u {username} --password-stdin"),
            )
            .await?;
        }
        sp.finish_and_clear();
    }

    for app in &apps {
        deploy_app(config_path, fleet, app, &pool, force, notifier).await?;
    }

    for (name, r) in &runners {
        deploy_runner(fleet, name, r, &pool, notifier).await?;
    }

    pool.close().await?;
    ui::success("Deploy complete");
    Ok(())
}

async fn deploy_app(
    config_path: &str,
    fleet: &Fleet,
    app: &ResolvedApp,
    pool: &SshPool,
    force: bool,
    notifier: &Notifier,
) -> Result<()> {
    if app.servers.is_empty() {
        bail!("App '{}' has no servers assigned", app.name);
    }

    println!();
    ui::header(&format!("Deploying {}", app.name));

    let mut app = app.clone();
    ensure_r2(config_path, fleet, &mut app).await?;
    let app = &app;

    let compose_yaml = compose::generate(app, &fleet.network);
    let env_content = compose::generate_env(app);
    let caddy_fragment = caddy::generate(app);

    for server_name in &app.servers {
        notifier
            .send(Event::deploy_started(&app.name, server_name))
            .await;

        let result = deploy_app_to_server(
            app,
            pool,
            server_name,
            &compose_yaml,
            &env_content,
            caddy_fragment.as_deref(),
            force,
        )
        .await;

        if let Err(ref e) = result {
            notifier
                .send(Event::deploy_failed(&app.name, server_name, &e.to_string()))
                .await;
            result?;
        }

        notifier
            .send(Event::deploy_completed(&app.name, server_name))
            .await;
    }

    if let Some(ref routing) = app.routing {
        if !routing.domains.is_empty() {
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

                    for domain in &routing.domains {
                        cloudflare::ensure_dns_record(cf_token, domain, &server_ip, true).await?;
                    }
                }
                sp.finish_and_clear();
                ui::success("  DNS records ensured");
            }
        }
    }

    if app.name == "observe" {
        sync_observability(config_path, fleet, app).await?;
    }

    Ok(())
}

async fn sync_observability(config_path: &str, fleet: &Fleet, app: &ResolvedApp) -> Result<()> {
    let Some(routing) = app.routing.as_ref() else {
        return Ok(());
    };
    let Some(domain) = routing.domains.first() else {
        return Ok(());
    };
    let Some(user) = app.env.get("ZO_ROOT_USER_EMAIL") else {
        return Ok(());
    };
    let Some(password) = app.env.get("ZO_ROOT_USER_PASSWORD") else {
        return Ok(());
    };

    let hub_url = format!("https://{domain}");
    let sp = ui::spinner("  Syncing observability config...");
    let out = crate::observability::sync(&crate::observability::SyncInput {
        hub_url: &hub_url,
        user,
        password,
        discord_webhook_url: fleet.secrets.discord_webhook_url.as_deref(),
        telegram_bot_token: fleet.secrets.telegram_bot_token.as_deref(),
        telegram_chat_id: fleet.secrets.telegram_chat_id.as_deref(),
    })
    .await?;
    sp.finish_and_clear();

    let mut details = Vec::new();
    if fleet
        .secrets
        .discord_webhook_url
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        details.push("discord");
    }
    if fleet
        .secrets
        .telegram_bot_token
        .as_deref()
        .is_some_and(|s| !s.is_empty())
        && fleet
            .secrets
            .telegram_chat_id
            .as_deref()
            .is_some_and(|s| !s.is_empty())
    {
        details.push("telegram");
    }
    let summary = if details.is_empty() {
        "no destinations configured".to_string()
    } else {
        format!("destinations: {}", details.join(", "))
    };
    ui::success(&format!("  Observability synced ({summary})"));

    if let Some(rum_token) = out.rum_token {
        let prev = fleet.secrets.oo_rum_token.as_deref().unwrap_or("");
        if prev != rum_token {
            crate::login::save_fleet_secret(
                std::path::Path::new(config_path)
                    .with_file_name("fleet.env.toml")
                    .as_path(),
                "oo_rum_token",
                &rum_token,
            )?;
            ui::success(&format!("  RUM token saved to fleet.env.toml: {rum_token}"));
        }
    }

    Ok(())
}

async fn deploy_app_to_server(
    app: &ResolvedApp,
    pool: &SshPool,
    server_name: &str,
    compose_yaml: &str,
    env_content: &str,
    caddy_fragment: Option<&str>,
    force: bool,
) -> Result<()> {
    let sp = ui::spinner(&format!("  {server_name} → uploading files..."));

    let app_dir = format!("/opt/flow/{}", app.name);

    pool.exec(server_name, &format!("mkdir -p {app_dir}"))
        .await?;

    let compose_path = format!("{app_dir}/docker-compose.yml");
    pool.upload_file(server_name, &compose_path, compose_yaml)
        .await?;

    if !env_content.trim().is_empty() {
        let env_path = format!("{app_dir}/.env");
        pool.upload_file(server_name, &env_path, env_content)
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
    if force {
        pool.exec(
            server_name,
            &format!("cd {app_dir} && docker compose up -d --force-recreate"),
        )
        .await?;
    } else {
        match app.deploy_strategy {
            DeployStrategy::Rolling => {
                pool.exec(
                    server_name,
                    &format!("cd {app_dir} && docker compose up -d"),
                )
                .await?;
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
    }
    sp.finish_and_clear();

    if let Some(fragment) = caddy_fragment {
        let sp = ui::spinner(&format!("  {server_name} → updating Caddy..."));
        let caddy_sites_dir = "/opt/flow/caddy/sites";
        pool.exec(server_name, &format!("mkdir -p {caddy_sites_dir}"))
            .await?;
        let caddy_path = format!("{caddy_sites_dir}/{}", app.name);
        pool.upload_file(server_name, &caddy_path, fragment).await?;
        pool.exec(
            server_name,
            "cd /opt/flow/caddy && docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile",
        )
        .await?;
        sp.finish_and_clear();
    }

    ui::success(&format!("  {server_name} → {}", app.name));
    Ok(())
}

async fn deploy_runner(
    fleet: &Fleet,
    name: &str,
    r: &Runner,
    pool: &SshPool,
    notifier: &Notifier,
) -> Result<()> {
    let gh_token = fleet
        .secrets
        .gh_token
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("gh_token not set — run `flow login gh`"))?;

    println!();
    ui::header(&format!("Deploying runner-{name}"));

    notifier
        .send(Event::runner_deploy_started(name, &r.server))
        .await;

    let result = deploy_runner_inner(name, r, pool, gh_token).await;

    if let Err(ref e) = result {
        notifier
            .send(Event::runner_deploy_failed(name, &r.server, &e.to_string()))
            .await;
        result?;
    }

    notifier
        .send(Event::runner_deploy_completed(name, &r.server))
        .await;
    ui::success(&format!("  {} → runner-{}", r.server, name));
    Ok(())
}

async fn deploy_runner_inner(name: &str, r: &Runner, pool: &SshPool, gh_token: &str) -> Result<()> {
    let compose_yaml = runner::generate_compose(name, r);
    let env_content = runner::generate_env(gh_token);
    let runner_dir = format!("/opt/flow/runner-{name}");

    let sp = ui::spinner(&format!("  {} → uploading files...", r.server));
    pool.exec(&r.server, &format!("mkdir -p {runner_dir}"))
        .await?;
    pool.upload_file(
        &r.server,
        &format!("{runner_dir}/docker-compose.yml"),
        &compose_yaml,
    )
    .await?;
    pool.upload_file(&r.server, &format!("{runner_dir}/.env"), &env_content)
        .await?;
    pool.exec(&r.server, &format!("chmod 600 {runner_dir}/.env"))
        .await?;
    sp.finish_and_clear();

    let sp = ui::spinner(&format!("  {} → pulling images...", r.server));
    pool.exec(
        &r.server,
        &format!("cd {runner_dir} && docker compose pull"),
    )
    .await?;
    sp.finish_and_clear();

    let sp = ui::spinner(&format!("  {} → deploying...", r.server));
    pool.exec(
        &r.server,
        &format!("cd {runner_dir} && docker compose up -d"),
    )
    .await?;
    sp.finish_and_clear();

    Ok(())
}

async fn ensure_r2(config_path: &str, fleet: &Fleet, app: &mut ResolvedApp) -> Result<()> {
    if app.r2_buckets.is_empty() {
        return Ok(());
    }

    let token =
        fleet.secrets.cloudflare_api_token.as_deref().context(
            "r2_buckets configured but cloudflare_api_token missing — run `iron login cf`",
        )?;
    let account_id =
        fleet.secrets.cloudflare_account_id.as_deref().context(
            "r2_buckets configured but cloudflare_account_id missing — run `iron login cf`",
        )?;

    let sp = ui::spinner("  Ensuring R2 buckets...");
    for bucket in app.r2_buckets.clone() {
        r2::ensure_bucket(token, account_id, &bucket.name).await?;
        let public_url = if let Some(ref domain) = bucket.public_domain {
            r2::attach_custom_domain(token, account_id, &bucket.name, domain).await?;
            let target = r2::custom_domain_cname_target(&bucket.name, account_id);
            cloudflare::ensure_cname_record(token, domain, &target, true).await?;
            format!("https://{domain}")
        } else {
            format!("{}/{}", r2::s3_endpoint(account_id), bucket.name)
        };
        let key = bucket.name.to_uppercase().replace('-', "_");
        app.env
            .insert(format!("R2_BUCKET_{key}"), bucket.name.clone());
        app.env.insert(format!("R2_PUBLIC_URL_{key}"), public_url);
    }
    sp.finish_and_clear();
    ui::success("  R2 buckets ensured");

    if !app.env.contains_key("R2_ACCESS_KEY_ID") {
        let sp = ui::spinner("  Minting R2 API token...");
        let bucket_names: Vec<&str> = app.r2_buckets.iter().map(|b| b.name.as_str()).collect();
        let creds = r2::mint_app_token(token, account_id, &app.name, &bucket_names).await?;
        save_app_env_secrets(
            config_path,
            &app.name,
            &[
                ("R2_ACCESS_KEY_TOKEN_ID", &creds.token_id),
                ("R2_ACCESS_KEY_ID", &creds.access_key_id),
                ("R2_SECRET_ACCESS_KEY", &creds.secret_access_key),
            ],
        )?;
        app.env
            .insert("R2_ACCESS_KEY_TOKEN_ID".to_string(), creds.token_id);
        app.env
            .insert("R2_ACCESS_KEY_ID".to_string(), creds.access_key_id);
        app.env
            .insert("R2_SECRET_ACCESS_KEY".to_string(), creds.secret_access_key);
        sp.finish_and_clear();
        ui::success("  R2 API token minted and saved to fleet.env.toml");
    }

    app.env
        .insert("R2_ENDPOINT".to_string(), r2::s3_endpoint(account_id));
    app.env
        .insert("R2_ACCOUNT_ID".to_string(), account_id.to_string());

    Ok(())
}

pub fn save_app_env_secrets(
    config_path: &str,
    app_name: &str,
    vars: &[(&str, &str)],
) -> Result<()> {
    let env_path = Path::new(config_path).with_file_name("fleet.env.toml");

    let mut doc = if env_path.exists() {
        let content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        content
            .parse::<toml_edit::DocumentMut>()
            .with_context(|| format!("Failed to parse {}", env_path.display()))?
    } else {
        toml_edit::DocumentMut::new()
    };

    let apps = doc
        .entry("apps")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("[apps] is not a table in fleet.env.toml")?;

    let app_table = apps
        .entry(app_name)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .with_context(|| format!("[apps.{app_name}] is not a table in fleet.env.toml"))?;

    for (key, value) in vars {
        app_table.insert(key, toml_edit::value(*value));
    }

    std::fs::write(&env_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", env_path.display()))?;
    Ok(())
}
