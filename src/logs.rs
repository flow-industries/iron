use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::config::Fleet;
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(fleet: &Fleet, app_name: &str, follow: bool, server: Option<&str>) -> Result<()> {
    let app = fleet
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown app: {app_name}"))?;

    if app.servers.is_empty() {
        bail!("App '{app_name}' has no servers assigned");
    }

    let server_name = if let Some(s) = server {
        if !app.servers.contains(&s.to_string()) {
            bail!("App '{app_name}' is not deployed to server '{s}'");
        }
        s.to_string()
    } else {
        if app.servers.len() > 1 {
            eprintln!(
                "Note: {} is on {} servers, showing logs from {} (use --server to pick)",
                app_name,
                app.servers.len(),
                app.servers[0]
            );
        }
        app.servers[0].clone()
    };
    let server_name = &server_name;
    let server = fleet
        .servers
        .get(server_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown server: {server_name}"))?;

    let sp = ui::spinner(&format!("Connecting to {server_name}..."));
    let pool = SshPool::connect_one(server_name, server).await?;
    sp.finish_and_clear();

    let follow_flag = if follow { " -f" } else { "" };
    let cmd = format!("cd /opt/flow/{app_name} && docker compose logs{follow_flag} --tail 100");

    if follow {
        let mut child = pool.exec_streaming(server_name, &cmd).await?;
        let stdout = child.stdout().take();
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Some(line) = lines.next_line().await? {
                println!("{line}");
            }
        }
        child.wait().await?;
    } else {
        let output = pool.exec(server_name, &cmd).await?;
        print!("{output}");
    }

    pool.close().await?;
    Ok(())
}
