use std::path::Path;

use anyhow::Result;
use toml_edit::{Array, DocumentMut, Item, Table};

use crate::ui;

pub fn run(config_path: &str) -> Result<()> {
    let path = Path::new(config_path);

    if path.exists() {
        ui::success(&format!("{config_path} already exists"));
        return Ok(());
    }

    let mut doc = DocumentMut::new();

    ui::header("Initialize fleet.toml");

    if let Some(domain) = ui::prompt("Fleet domain (e.g. example.com):") {
        doc["domain"] = toml_edit::value(domain);
    }

    if let Some(zone_id) = ui::prompt("Cloudflare zone ID (skip if not using DNS):") {
        doc["cloudflare_zone_id"] = toml_edit::value(zone_id);
    }

    let mut server_names: Vec<String> = Vec::new();

    while ui::confirm("Add a server? (y/N)") {
        let Some(name) = ui::prompt("  Server name:") else {
            continue;
        };
        let Some(ip) = ui::prompt("  Server IP:") else {
            continue;
        };

        let mut server = Table::new();
        server["host"] = toml_edit::value(format!(
            "{name}.{}",
            doc.get("domain")
                .and_then(Item::as_str)
                .unwrap_or("example.com")
        ));
        server["ip"] = toml_edit::value(&ip);

        if doc.get("servers").is_none() {
            doc["servers"] = Item::Table(Table::new());
        }
        doc["servers"][&name] = Item::Table(server);
        server_names.push(name);
        println!();
    }

    while ui::confirm("Add an app? (y/N)") {
        let Some(name) = ui::prompt("  App name:") else {
            continue;
        };
        let Some(image) = ui::prompt("  Docker image:") else {
            continue;
        };

        let server = if server_names.len() == 1 {
            server_names[0].clone()
        } else {
            let hint = if server_names.is_empty() {
                String::new()
            } else {
                format!(" ({})", server_names.join(", "))
            };
            let Some(s) = ui::prompt(&format!("  Server{hint}:")) else {
                continue;
            };
            s
        };

        let mut app = Table::new();
        app["image"] = toml_edit::value(&image);

        let mut servers = Array::new();
        servers.push(&server);
        app["servers"] = toml_edit::value(servers);

        let port = ui::prompt("  Container port (skip if no HTTP routing):");

        if let Some(port_str) = &port {
            if let Ok(p) = port_str.parse::<i64>() {
                app["port"] = toml_edit::value(p);
            }
        }

        if port.is_some() {
            if let Some(route) = ui::prompt("  Route / domain (e.g. example.com):") {
                let mut routing = Table::new();
                let mut routes = Array::new();
                routes.push(&route);
                routing["routes"] = toml_edit::value(routes);
                app["routing"] = Item::Table(routing);
            }
        }

        if doc.get("apps").is_none() {
            doc["apps"] = Item::Table(Table::new());
        }
        doc["apps"][&name] = Item::Table(app);
        println!();
    }

    std::fs::write(path, doc.to_string())?;
    ui::success(&format!("Created {config_path}"));
    Ok(())
}
