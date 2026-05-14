use anyhow::{Context, Result, bail};

use crate::config::Fleet;
use crate::notify::{Event, Notifier};
use crate::ssh::{connect_root, stream_command};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpgradeMode {
    SecurityOnly,
    Standard,
    Full,
}

impl UpgradeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SecurityOnly => "security-only",
            Self::Standard => "standard",
            Self::Full => "full",
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
pub struct UpgradeOpts {
    pub server: Option<String>,
    pub mode: UpgradeMode,
    pub reboot_if_required: bool,
    pub dry_run: bool,
    pub autoremove: bool,
    pub yes: bool,
}

const DPKG_KEEP_CONFIGS: &str =
    r#"-o Dpkg::Options::="--force-confold" -o Dpkg::Options::="--force-confdef""#;

pub fn build_upgrade_cmd(mode: UpgradeMode, dry_run: bool) -> String {
    let env = "DEBIAN_FRONTEND=noninteractive";
    match mode {
        UpgradeMode::SecurityOnly => {
            let suffix = if dry_run { " --dry-run" } else { "" };
            format!("{env} unattended-upgrade -d{suffix}")
        }
        UpgradeMode::Standard => {
            let dry = if dry_run { " -s" } else { "" };
            format!("{env} apt-get{dry} upgrade -y {DPKG_KEEP_CONFIGS}")
        }
        UpgradeMode::Full => {
            let dry = if dry_run { " -s" } else { "" };
            format!("{env} apt-get{dry} dist-upgrade -y {DPKG_KEEP_CONFIGS}")
        }
    }
}

pub fn build_autoremove_cmd(dry_run: bool) -> String {
    let dry = if dry_run { " -s" } else { "" };
    format!("DEBIAN_FRONTEND=noninteractive apt-get{dry} autoremove -y")
}

const DOCKER_DAEMON_PACKAGES: &[&str] = &["docker-ce", "containerd.io"];

pub async fn run(fleet: &Fleet, opts: UpgradeOpts, notifier: &Notifier) -> Result<()> {
    let servers: Vec<(String, crate::config::Server)> = if let Some(ref name) = opts.server {
        let server = fleet
            .servers
            .get(name)
            .with_context(|| format!("Server '{name}' not found in fleet.toml"))?;
        vec![(name.clone(), server.clone())]
    } else {
        fleet
            .servers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    if servers.is_empty() {
        bail!("No servers configured in fleet.toml");
    }

    let mode_label = opts.mode.as_str();
    let dry_label = if opts.dry_run { " (dry-run)" } else { "" };
    let mut failures: Vec<String> = Vec::new();

    for (name, server) in &servers {
        let ip = server
            .ip
            .as_deref()
            .with_context(|| format!("Server '{name}' has no IP address"))?;

        ui::header(&format!(
            "Upgrade: {name} ({} / {ip}) — {mode_label}{dry_label}",
            server.host
        ));

        if opts.mode == UpgradeMode::Full && !opts.yes && !opts.dry_run {
            let prompt = format!(
                "This will dist-upgrade {name} (kernel, dependencies, removals). Continue? (y/N)"
            );
            if !ui::confirm(&prompt) {
                ui::error(&format!("{name}: skipped"));
                continue;
            }
        }

        match upgrade_one(name, server, &opts, notifier).await {
            Ok(()) => {}
            Err(e) => {
                let err_str = format!("{e:#}");
                ui::error(&format!("{name}: {err_str}"));
                notifier
                    .send(Event::upgrade_failed(name, mode_label, &err_str))
                    .await;
                failures.push(name.clone());
            }
        }
    }

    if !failures.is_empty() {
        bail!("Upgrade failed on: {}", failures.join(", "));
    }

    Ok(())
}

async fn upgrade_one(
    name: &str,
    server: &crate::config::Server,
    opts: &UpgradeOpts,
    notifier: &Notifier,
) -> Result<()> {
    let session = connect_root(name, server).await?;

    let sp = ui::spinner(&format!("{name} → apt-get update"));
    let code = stream_command(&session, "apt-get update -qq").await?;
    sp.finish_and_clear();
    if code != 0 {
        bail!("apt-get update exited with status {code}");
    }

    let pre_count = count_upgradable(&session).await.unwrap_or(0);
    let pre_docker_versions = read_package_versions(&session, DOCKER_DAEMON_PACKAGES).await;

    let code = stream_command(&session, &build_upgrade_cmd(opts.mode, opts.dry_run)).await?;
    if code != 0 {
        bail!("upgrade command exited with status {code}");
    }

    let post_count = count_upgradable(&session).await.unwrap_or(pre_count);
    let packages_changed = if opts.dry_run {
        0
    } else {
        pre_count.saturating_sub(post_count)
    };

    if opts.autoremove && !opts.dry_run {
        let sp = ui::spinner(&format!("{name} → apt-get autoremove"));
        let code = stream_command(&session, &build_autoremove_cmd(false)).await?;
        sp.finish_and_clear();
        if code != 0 {
            bail!("apt-get autoremove exited with status {code}");
        }
    }

    if !opts.dry_run {
        let post_docker_versions = read_package_versions(&session, DOCKER_DAEMON_PACKAGES).await;
        let changed = docker_packages_changed(&pre_docker_versions, &post_docker_versions);
        if !changed.is_empty() {
            ui::header(&format!(
                "{name}: {} upgraded — restarting docker to re-assert iptables",
                changed.join(", ")
            ));
            let sp = ui::spinner(&format!("{name} → systemctl restart docker"));
            let code = stream_command(&session, "systemctl restart docker").await?;
            sp.finish_and_clear();
            if code != 0 {
                bail!("systemctl restart docker exited with status {code}");
            }
            ui::success(&format!("{name} → docker restarted"));
        }
    }

    let reboot_needed = check_reboot_required(&session).await?;

    if opts.dry_run {
        ui::success(&format!(
            "{name} → dry-run complete (would change {pre_count} package(s))"
        ));
    } else {
        ui::success(&format!(
            "{name} → {packages_changed} package(s) upgraded, {post_count} still pending"
        ));
    }
    notifier
        .send(Event::upgrade_completed(
            name,
            opts.mode.as_str(),
            packages_changed,
        ))
        .await;

    if reboot_needed {
        if opts.reboot_if_required && !opts.dry_run {
            let sp = ui::spinner(&format!("{name} → rebooting"));
            let _ = stream_command(&session, "systemctl reboot").await;
            sp.finish_and_clear();
            ui::success(&format!("{name} → reboot triggered"));
        } else {
            ui::error(&format!(
                "{name}: reboot required (next 04:00 UTC unattended-upgrade cycle, or pass --reboot-if-required)"
            ));
        }
    }

    let _ = session.close().await;
    Ok(())
}

async fn count_upgradable(session: &openssh::Session) -> Result<usize> {
    let output = session
        .command("sh")
        .arg("-c")
        .arg("apt list --upgradable 2>/dev/null | tail -n +2 | wc -l")
        .output()
        .await
        .context("failed to count upgradable packages")?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim()
        .parse()
        .context("failed to parse upgradable count")
}

async fn read_package_versions(
    session: &openssh::Session,
    packages: &[&str],
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pkg in packages {
        let cmd = format!("dpkg-query -W -f='${{Version}}' {pkg} 2>/dev/null || true");
        let Ok(output) = session.command("sh").arg("-c").arg(&cmd).output().await else {
            continue;
        };
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !version.is_empty() {
            out.insert((*pkg).to_string(), version);
        }
    }
    out
}

pub fn docker_packages_changed<S: std::hash::BuildHasher>(
    before: &std::collections::HashMap<String, String, S>,
    after: &std::collections::HashMap<String, String, S>,
) -> Vec<String> {
    let mut changed: Vec<String> = Vec::new();
    for (pkg, after_ver) in after {
        match before.get(pkg) {
            Some(before_ver) if before_ver == after_ver => {}
            _ => changed.push(pkg.clone()),
        }
    }
    changed.sort();
    changed
}

async fn check_reboot_required(session: &openssh::Session) -> Result<bool> {
    let output = session
        .command("sh")
        .arg("-c")
        .arg("[ -f /var/run/reboot-required ] && echo yes || echo no")
        .output()
        .await
        .context("failed to check reboot-required marker")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim() == "yes")
}
