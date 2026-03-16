use std::path::Path;

use anyhow::{Context, Result};
use toml_edit::DocumentMut;

use crate::cli::LoginCommand;
use crate::ui;

const CF_TOKEN_URL: &str =
    "https://dash.cloudflare.com/profile/api-tokens → Create Token → Edit zone DNS";
const GH_TOKEN_URL: &str =
    "https://github.com/settings/tokens/new?scopes=read:packages&description=flow-iron";

pub async fn run(config_path: &str, command: Option<&LoginCommand>) -> Result<()> {
    let env_path = Path::new(config_path).with_file_name("fleet.env.toml");

    match command {
        Some(LoginCommand::Cf) => cloudflare_login(&env_path).await,
        Some(LoginCommand::Gh) => github_login(&env_path).await,
        None => {
            cloudflare_login(&env_path).await?;
            println!();
            github_login(&env_path).await
        }
    }
}

async fn cloudflare_login(env_path: &Path) -> Result<()> {
    ui::header("Cloudflare");
    println!("  Create a token at:");
    println!("  {CF_TOKEN_URL}");
    println!();

    let token = ui::prompt_secret("Cloudflare API token:")
        .ok_or_else(|| anyhow::anyhow!("No token provided"))?;

    let sp = ui::spinner("Validating token...");
    crate::cloudflare::verify_token(&token).await?;
    sp.finish_and_clear();
    ui::success("Token is valid");

    save_fleet_secret(env_path, "cloudflare_api_token", &token)?;
    ui::success("Saved to fleet.env.toml");
    Ok(())
}

async fn github_login(env_path: &Path) -> Result<()> {
    ui::header("GitHub Container Registry");
    println!("  Create a Personal Access Token (classic) with read:packages scope:");
    println!("  {GH_TOKEN_URL}");
    println!();

    let token =
        ui::prompt_secret("GitHub token:").ok_or_else(|| anyhow::anyhow!("No token provided"))?;

    let sp = ui::spinner("Validating token...");
    verify_github_token(&token).await?;
    sp.finish_and_clear();
    ui::success("Token is valid");

    save_fleet_secret(env_path, "gh_token", &token)?;
    ui::success("Saved to fleet.env.toml");
    Ok(())
}

async fn verify_github_token(token: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.github.com/user")
        .header("User-Agent", "flow-iron")
        .bearer_auth(token)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Invalid GitHub token");
    }
    Ok(())
}

pub fn save_fleet_secret(env_path: &Path, key: &str, value: &str) -> Result<()> {
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

    fleet.insert(key, toml_edit::value(value));

    std::fs::write(env_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", env_path.display()))?;
    Ok(())
}
