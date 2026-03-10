use std::path::Path;

use anyhow::{Context, Result};
use comfy_table::{
    Cell, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED,
};
use toml_edit::DocumentMut;

use crate::ui;

pub fn run(config_path: &str, args: &[String]) -> Result<()> {
    let env_path = Path::new(config_path).with_file_name("fleet.env.toml");

    if args.is_empty() {
        list_fleet(&env_path)?;
        interactive_fleet(&env_path)?;
        return Ok(());
    }

    if args[0].contains('=') {
        for arg in args {
            let (key, value) = parse_assignment(arg)?;
            set_fleet_var(&env_path, key, value)?;
            ui::success(&format!("Set fleet var {key}"));
        }
        return Ok(());
    }

    let app_name = &args[0];
    let assignments = &args[1..];

    if assignments.is_empty() {
        list_app(&env_path, app_name)?;
        interactive_app(&env_path, app_name)?;
        return Ok(());
    }

    for arg in assignments {
        let (key, value) = parse_assignment(arg)?;
        set_app_var(&env_path, app_name, key, value)?;
        ui::success(&format!("Set {app_name} var {key}"));
    }

    Ok(())
}

fn load_doc(env_path: &Path) -> Result<DocumentMut> {
    if env_path.exists() {
        let content = std::fs::read_to_string(env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        content
            .parse::<DocumentMut>()
            .with_context(|| format!("Failed to parse {}", env_path.display()))
    } else {
        Ok(DocumentMut::new())
    }
}

fn save_doc(env_path: &Path, doc: &DocumentMut) -> Result<()> {
    std::fs::write(env_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", env_path.display()))
}

fn list_fleet(env_path: &Path) -> Result<()> {
    let doc = load_doc(env_path)?;

    ui::header("Fleet secrets");

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Key", "Value"]);

    if let Some(fleet) = doc.get("fleet").and_then(|f| f.as_table()) {
        for (key, item) in fleet {
            if let Some(value) = item.as_str() {
                table.add_row(vec![Cell::new(key), Cell::new(censor(value))]);
            }
        }
    }

    println!("{table}");
    Ok(())
}

fn list_app(env_path: &Path, app_name: &str) -> Result<()> {
    let doc = load_doc(env_path)?;

    ui::header(&format!("Environment: {app_name}"));

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["Key", "Value"]);

    if let Some(app_table) = doc
        .get("apps")
        .and_then(|a| a.as_table())
        .and_then(|a| a.get(app_name))
        .and_then(|a| a.as_table())
    {
        for (key, item) in app_table {
            if key == "services" {
                continue;
            }
            if let Some(value) = item.as_str() {
                table.add_row(vec![Cell::new(key), Cell::new(censor(value))]);
            }
        }

        if let Some(services) = app_table.get("services").and_then(|s| s.as_table()) {
            for (svc_name, svc_table) in services {
                if let Some(svc) = svc_table.as_table() {
                    for (key, item) in svc {
                        if let Some(value) = item.as_str() {
                            table.add_row(vec![
                                Cell::new(format!("{svc_name}.{key}")),
                                Cell::new(censor(value)),
                            ]);
                        }
                    }
                }
            }
        }
    }

    println!("{table}");
    Ok(())
}

fn interactive_fleet(env_path: &Path) -> Result<()> {
    let mut count = 0;
    while let Some(input) = ui::prompt("key=value (empty to finish):") {
        let (key, value) = parse_assignment(&input)?;
        set_fleet_var(env_path, key, value)?;
        count += 1;
    }
    if count > 0 {
        ui::success(&format!("Saved {count} variable(s)"));
    }
    Ok(())
}

fn interactive_app(env_path: &Path, app_name: &str) -> Result<()> {
    let mut count = 0;
    while let Some(input) = ui::prompt("key=value (empty to finish):") {
        let (key, value) = parse_assignment(&input)?;
        set_app_var(env_path, app_name, key, value)?;
        count += 1;
    }
    if count > 0 {
        ui::success(&format!("Saved {count} variable(s)"));
    }
    Ok(())
}

fn set_fleet_var(env_path: &Path, key: &str, value: &str) -> Result<()> {
    let mut doc = load_doc(env_path)?;

    let fleet = doc
        .entry("fleet")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("[fleet] is not a table in fleet.env.toml")?;

    fleet.insert(key, toml_edit::value(value));
    save_doc(env_path, &doc)
}

fn set_app_var(env_path: &Path, app_name: &str, key: &str, value: &str) -> Result<()> {
    let mut doc = load_doc(env_path)?;

    let apps = doc
        .entry("apps")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("[apps] is not a table in fleet.env.toml")?;

    let app = apps
        .entry(app_name)
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .with_context(|| format!("[apps.{app_name}] is not a table in fleet.env.toml"))?;

    app.insert(key, toml_edit::value(value));
    save_doc(env_path, &doc)
}

fn censor(value: &str) -> String {
    if value.len() >= 4 {
        format!("{}***", &value[..3])
    } else {
        "***".to_string()
    }
}

fn parse_assignment(s: &str) -> Result<(&str, &str)> {
    s.split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Invalid assignment: {s} (expected KEY=VALUE)"))
}
