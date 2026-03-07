use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::cli::LoginCommand;
use crate::ui;

pub async fn run(config_path: &str, command: LoginCommand) -> Result<()> {
    match command {
        LoginCommand::Cf => cloudflare_login(config_path).await,
    }
}

async fn cloudflare_login(config_path: &str) -> Result<()> {
    let env_path = Path::new(config_path).with_file_name("fleet.env.toml");

    let token = ui::prompt_secret("Cloudflare API token:")
        .ok_or_else(|| anyhow::anyhow!("No token provided"))?;

    let sp = ui::spinner("Validating token...");
    crate::cloudflare::verify_token(&token).await?;
    sp.finish_and_clear();
    ui::success("Token is valid");

    save_cloudflare_token(&env_path, &token)?;
    ui::success("Saved to fleet.env.toml");
    Ok(())
}

pub fn save_cloudflare_token(env_path: &Path, token: &str) -> Result<()> {
    let mut doc = if env_path.exists() {
        let content = std::fs::read_to_string(env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        content
            .parse::<DocumentMut>()
            .with_context(|| format!("Failed to parse {}", env_path.display()))?
    } else {
        DocumentMut::new()
    };

    let fleet = doc
        .entry("fleet")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("[fleet] is not a table in fleet.env.toml")?;

    fleet.insert("cloudflare_api_token", toml_edit::value(token));

    std::fs::write(env_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", env_path.display()))?;
    Ok(())
}
