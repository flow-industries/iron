use anyhow::{Result, bail};

use crate::config::Fleet;
use crate::notify::{Event, Notifier};
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(
    fleet: &Fleet,
    app_name: &str,
    server_filter: Option<&str>,
    notifier: &Notifier,
) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;

    let target_servers: Vec<&str> = if let Some(server) = server_filter {
        if !app.servers.contains(&server.to_string()) {
            bail!("App '{app_name}' is not assigned to server '{server}'");
        }
        vec![server]
    } else {
        app.servers.iter().map(String::as_str).collect()
    };

    let servers_to_connect: std::collections::HashMap<_, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| target_servers.contains(&name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let sp = ui::spinner("Connecting to servers...");
    let pool = SshPool::connect(&servers_to_connect).await?;
    sp.finish_and_clear();

    let app_dir = format!("/opt/flow/{}", app.name);

    for server_name in &target_servers {
        let sp = ui::spinner(&format!("{server_name} → restarting {app_name}..."));
        pool.exec(
            server_name,
            &format!("cd {app_dir} && docker compose restart"),
        )
        .await?;
        sp.finish_and_clear();
        ui::success(&format!("{server_name} → {app_name} restarted"));
        notifier.send(Event::app_restarted(app_name, server_name));
    }

    pool.close().await?;
    Ok(())
}
