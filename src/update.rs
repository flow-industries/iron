use anyhow::{Context, Result, bail};

use crate::ui;

pub async fn run() -> Result<()> {
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

    println!("Updating flow CLI...\n");
    let status = tokio::process::Command::new(&cargo)
        .args([
            "install",
            "--git",
            "ssh://git@github.com/flow-industries/iron",
        ])
        .status()
        .await
        .context("Failed to run cargo install")?;

    if !status.success() {
        bail!("cargo install flow-iron failed");
    }

    ui::success("flow CLI updated");
    Ok(())
}
