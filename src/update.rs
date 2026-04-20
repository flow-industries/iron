use anyhow::{Context, Result, bail};

use crate::ui;

const DEFAULT_GIT_URL: &str = "https://github.com/flow-industries/iron";
const CRATE_NAME: &str = "flow-iron";

pub async fn run(git: bool, git_url: Option<&str>) -> Result<()> {
    let cargo = tokio::process::Command::new("which")
        .arg("cargo")
        .output()
        .await
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout).ok()
            } else {
                None
            }
        })
        .map(|s| s.trim().to_string())
        .context("cargo not found — install Rust via https://rustup.rs")?;

    let use_git = git || git_url.is_some();

    let mut command = tokio::process::Command::new(&cargo);
    command.env("CARGO_NET_GIT_FETCH_WITH_CLI", "true");

    if use_git {
        let url = git_url.unwrap_or(DEFAULT_GIT_URL);
        println!("Updating flow CLI from {url}...\n");
        command.args(["install", "--git", url]);
    } else {
        println!("Updating flow CLI from crates.io...\n");
        command.args(["install", CRATE_NAME]);
    }

    let status = command
        .status()
        .await
        .context("Failed to run cargo install")?;

    if !status.success() {
        bail!("cargo install {CRATE_NAME} failed");
    }

    ui::success("flow CLI updated");
    Ok(())
}
