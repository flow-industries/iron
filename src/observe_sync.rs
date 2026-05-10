use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::config::Fleet;
use crate::login;
use crate::observability::{SyncInput, sync};
use crate::ui;

pub async fn run(config_path: &str, fleet: &Fleet) -> Result<()> {
    let app = fleet
        .apps
        .get("observe")
        .context("No app named 'observe' in fleet.toml")?;

    let domain = app
        .routing
        .as_ref()
        .and_then(|r| r.domains.first())
        .context("[apps.observe.routing.domains] is empty; cannot derive hub URL")?;

    let user = app
        .env
        .get("ZO_ROOT_USER_EMAIL")
        .context("ZO_ROOT_USER_EMAIL not set in [apps.observe] env")?;
    let password = app
        .env
        .get("ZO_ROOT_USER_PASSWORD")
        .context("ZO_ROOT_USER_PASSWORD not set in [apps.observe] env")?;

    let hub_url = format!("https://{domain}");
    ui::header(&format!("Reconciling observability against {hub_url}"));

    let sp = ui::spinner("Syncing...");
    let out = sync(&SyncInput {
        hub_url: &hub_url,
        user,
        password,
        discord_webhook_url: fleet.secrets.discord_webhook_url.as_deref(),
        telegram_bot_token: fleet.secrets.telegram_bot_token.as_deref(),
        telegram_chat_id: fleet.secrets.telegram_chat_id.as_deref(),
    })
    .await
    .inspect_err(|_| {
        sp.finish_and_clear();
    })?;
    sp.finish_and_clear();

    let mut configured = Vec::new();
    if fleet
        .secrets
        .discord_webhook_url
        .as_deref()
        .is_some_and(|s| !s.is_empty())
    {
        configured.push("discord");
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
        configured.push("telegram");
    }
    if configured.is_empty() {
        ui::error(
            "No destinations configured. Set discord_webhook_url and/or telegram_bot_token + telegram_chat_id in [fleet] of fleet.env.toml.",
        );
    } else {
        ui::success(&format!(
            "Templates, destinations, alerts in sync ({})",
            configured.join(", ")
        ));
    }

    if let Some(rum_token) = out.rum_token {
        let prev = fleet.secrets.oo_rum_token.as_deref().unwrap_or("");
        if prev == rum_token {
            ui::success(&format!("RUM token already current: {rum_token}"));
        } else {
            let env_path = Path::new(config_path).with_file_name("fleet.env.toml");
            login::save_fleet_secret(&env_path, "oo_rum_token", &rum_token)?;
            ui::success(&format!("RUM token saved to fleet.env.toml: {rum_token}"));
        }
    } else {
        bail!("Failed to fetch RUM token from hub");
    }

    Ok(())
}
