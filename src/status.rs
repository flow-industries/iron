use anyhow::Result;
use comfy_table::{Table, presets::UTF8_FULL_CONDENSED, modifiers::UTF8_ROUND_CORNERS};
use console::style;
use std::collections::HashMap;

use crate::config::Fleet;
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(fleet: &Fleet, server_filter: Option<&str>) -> Result<()> {
    let filtered: HashMap<String, _> = fleet
        .servers
        .iter()
        .filter(|(name, _)| server_filter.is_none() || server_filter == Some(name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    if filtered.is_empty() {
        anyhow::bail!("No matching server found");
    }

    let sp = ui::spinner("Connecting...");
    let pool = SshPool::connect(&filtered).await?;
    sp.finish_and_clear();

    for (name, _server) in &filtered {
        ui::header(&format!("Server: {}", name));

        let output = pool
            .exec(
                name,
                "docker ps -a --format '{{.Names}}\t{{.Status}}\t{{.Image}}\t{{.Ports}}\t{{.Size}}' 2>/dev/null || echo ''",
            )
            .await?;

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL_CONDENSED)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                style("Container").bold().to_string(),
                style("Status").bold().to_string(),
                style("Image").bold().to_string(),
                style("Ports").bold().to_string(),
                style("Size").bold().to_string(),
            ]);

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() == 5 {
                let status_colored = if parts[1].starts_with("Up") {
                    style(parts[1]).green().to_string()
                } else {
                    style(parts[1]).red().to_string()
                };
                table.add_row(vec![
                    parts[0].to_string(),
                    status_colored,
                    parts[2].to_string(),
                    parts[3].to_string(),
                    parts[4].to_string(),
                ]);
            }
        }

        println!("{table}");
    }

    pool.close().await?;
    Ok(())
}
