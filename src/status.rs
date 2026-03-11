use anyhow::Result;
use comfy_table::{
    Cell, CellAlignment, Color, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL_CONDENSED,
};
use console::style;
use std::collections::HashMap;
use std::fmt::Write;
use std::time::Duration;

use crate::config::Fleet;
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(fleet: &Fleet, server_filter: Option<&str>, follow: bool) -> Result<()> {
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

    if follow {
        loop {
            let buf = render_status(&pool, &filtered).await?;
            // Move cursor home, print buffer, clear any leftover lines below
            print!("\x1b[H{buf}\x1b[J");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    } else {
        print_status(&pool, &filtered).await?;
        pool.close().await?;
        Ok(())
    }
}

async fn render_status(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
) -> Result<String> {
    let mut buf = String::new();
    for name in servers.keys() {
        writeln!(
            buf,
            "\n{}",
            style(format!("Server: {name}")).bold().underlined()
        )?;

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
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec!["Container", "Status", "Image", "Ports", "Size"]);

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() == 5 {
                let status_color = if parts[1].starts_with("Up") {
                    Color::Green
                } else {
                    Color::Red
                };
                table.add_row(vec![
                    Cell::new(parts[0]),
                    Cell::new(parts[1]).fg(status_color),
                    Cell::new(parts[2]),
                    Cell::new(parts[3]),
                    Cell::new(parts[4]).set_alignment(CellAlignment::Right),
                ]);
            }
        }

        writeln!(buf, "{table}")?;
    }
    Ok(buf)
}

async fn print_status(
    pool: &SshPool,
    servers: &HashMap<String, crate::config::Server>,
) -> Result<()> {
    for name in servers.keys() {
        ui::header(&format!("Server: {name}"));

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
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec!["Container", "Status", "Image", "Ports", "Size"]);

        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.splitn(5, '\t').collect();
            if parts.len() == 5 {
                let status_color = if parts[1].starts_with("Up") {
                    Color::Green
                } else {
                    Color::Red
                };
                table.add_row(vec![
                    Cell::new(parts[0]),
                    Cell::new(parts[1]).fg(status_color),
                    Cell::new(parts[2]),
                    Cell::new(parts[3]),
                    Cell::new(parts[4]).set_alignment(CellAlignment::Right),
                ]);
            }
        }

        println!("{table}");
    }
    Ok(())
}
