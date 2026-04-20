use std::path::Path;

use anyhow::Result;

use crate::config::{EnvConfig, FleetSecrets};
use crate::ghcr;
use crate::server::WATCHER_IMAGE;

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn run(config_path: &str) -> Result<()> {
    println!("flow {CRATE_VERSION}");
    println!("watcher image: {WATCHER_IMAGE}");

    let Some((owner, package)) = ghcr::parse_ghcr_image(WATCHER_IMAGE) else {
        println!("latest GHCR tag: — (watcher image is not on GHCR)");
        return Ok(());
    };

    let token = load_gh_token(config_path);
    if let Some(release) = ghcr::fetch_latest_release(token.as_deref(), owner, package).await {
        println!("latest GHCR tag: {} ({})", release.tag, release.published);
    } else {
        let hint = if token.is_none() {
            " (set gh_token in fleet.env.toml or run `flow login gh`)"
        } else {
            ""
        };
        println!("latest GHCR tag: — (unavailable{hint})");
    }

    Ok(())
}

fn load_gh_token(config_path: &str) -> Option<String> {
    let env_path = Path::new(config_path).with_file_name("fleet.env.toml");
    let content = std::fs::read_to_string(&env_path).ok()?;
    let env_config: EnvConfig = toml::from_str(&content).ok()?;
    let FleetSecrets { gh_token, .. } = env_config.fleet;
    gh_token.filter(|t| !t.is_empty())
}
