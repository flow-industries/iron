#![allow(clippy::unwrap_used)]

use clap::Parser;
use iron::cli::{Cli, Command};
use iron::upgrade::{
    UpgradeMode, build_autoremove_cmd, build_upgrade_cmd, docker_packages_changed,
};
use std::collections::HashMap;

#[test]
fn standard_upgrade_uses_apt_get_upgrade() {
    let cmd = build_upgrade_cmd(UpgradeMode::Standard, false);
    assert!(cmd.contains("apt-get upgrade -y"));
    assert!(cmd.contains("DEBIAN_FRONTEND=noninteractive"));
    assert!(cmd.contains("--force-confold"));
    assert!(cmd.contains("--force-confdef"));
    assert!(!cmd.contains(" -s "));
}

#[test]
fn full_upgrade_uses_dist_upgrade() {
    let cmd = build_upgrade_cmd(UpgradeMode::Full, false);
    assert!(cmd.contains("apt-get dist-upgrade -y"));
    assert!(cmd.contains("--force-confold"));
}

#[test]
fn security_only_uses_unattended_upgrade() {
    let cmd = build_upgrade_cmd(UpgradeMode::SecurityOnly, false);
    assert!(cmd.contains("unattended-upgrade -d"));
    assert!(!cmd.contains("--dry-run"));
}

#[test]
fn dry_run_standard_passes_s_flag() {
    let cmd = build_upgrade_cmd(UpgradeMode::Standard, true);
    assert!(cmd.contains("apt-get -s upgrade -y"));
}

#[test]
fn dry_run_full_passes_s_flag() {
    let cmd = build_upgrade_cmd(UpgradeMode::Full, true);
    assert!(cmd.contains("apt-get -s dist-upgrade -y"));
}

#[test]
fn dry_run_security_only_uses_unattended_dry_run() {
    let cmd = build_upgrade_cmd(UpgradeMode::SecurityOnly, true);
    assert!(cmd.contains("unattended-upgrade -d --dry-run"));
}

#[test]
fn autoremove_command_includes_y_flag() {
    let cmd = build_autoremove_cmd(false);
    assert!(cmd.contains("apt-get autoremove -y"));
    assert!(!cmd.contains(" -s "));
}

#[test]
fn autoremove_dry_run_includes_s_flag() {
    let cmd = build_autoremove_cmd(true);
    assert!(cmd.contains("apt-get -s autoremove -y"));
}

#[test]
fn cli_parses_upgrade_with_no_flags() {
    let cli = Cli::try_parse_from(["iron", "upgrade"]).unwrap();
    match cli.command {
        Some(Command::Upgrade {
            server,
            security_only,
            full,
            reboot_if_required,
            dry_run,
            autoremove,
            yes,
        }) => {
            assert_eq!(server, None);
            assert!(!security_only);
            assert!(!full);
            assert!(!reboot_if_required);
            assert!(!dry_run);
            assert!(!autoremove);
            assert!(!yes);
        }
        _ => panic!("expected Upgrade variant"),
    }
}

#[test]
fn cli_parses_upgrade_with_all_flags() {
    let cli = Cli::try_parse_from([
        "iron",
        "upgrade",
        "--server",
        "fl-1",
        "--reboot-if-required",
        "--dry-run",
        "--autoremove",
        "--yes",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Upgrade {
            server,
            reboot_if_required,
            dry_run,
            autoremove,
            yes,
            ..
        }) => {
            assert_eq!(server.as_deref(), Some("fl-1"));
            assert!(reboot_if_required);
            assert!(dry_run);
            assert!(autoremove);
            assert!(yes);
        }
        _ => panic!("expected Upgrade variant"),
    }
}

#[test]
fn cli_rejects_security_only_with_full() {
    let result = Cli::try_parse_from(["iron", "upgrade", "--security-only", "--full"]);
    assert!(result.is_err(), "expected clap to reject conflicting flags");
}

#[test]
fn cli_parses_security_only() {
    let cli = Cli::try_parse_from(["iron", "upgrade", "--security-only"]).unwrap();
    match cli.command {
        Some(Command::Upgrade {
            security_only,
            full,
            ..
        }) => {
            assert!(security_only);
            assert!(!full);
        }
        _ => panic!("expected Upgrade variant"),
    }
}

#[test]
fn cli_parses_full() {
    let cli = Cli::try_parse_from(["iron", "upgrade", "--full"]).unwrap();
    match cli.command {
        Some(Command::Upgrade {
            security_only,
            full,
            ..
        }) => {
            assert!(!security_only);
            assert!(full);
        }
        _ => panic!("expected Upgrade variant"),
    }
}

#[test]
fn upgrade_mode_labels() {
    assert_eq!(UpgradeMode::SecurityOnly.as_str(), "security-only");
    assert_eq!(UpgradeMode::Standard.as_str(), "standard");
    assert_eq!(UpgradeMode::Full.as_str(), "full");
}

fn versions(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect()
}

#[test]
fn docker_changed_returns_empty_when_versions_match() {
    let before = versions(&[("docker-ce", "29.3.0-1"), ("containerd.io", "2.2.1-1")]);
    let after = versions(&[("docker-ce", "29.3.0-1"), ("containerd.io", "2.2.1-1")]);
    assert!(docker_packages_changed(&before, &after).is_empty());
}

#[test]
fn docker_changed_detects_version_bump() {
    let before = versions(&[("docker-ce", "29.3.0-1"), ("containerd.io", "2.2.1-1")]);
    let after = versions(&[("docker-ce", "29.4.3-1"), ("containerd.io", "2.2.1-1")]);
    assert_eq!(
        docker_packages_changed(&before, &after),
        vec!["docker-ce".to_string()]
    );
}

#[test]
fn docker_changed_detects_both_packages() {
    let before = versions(&[("docker-ce", "29.3.0-1"), ("containerd.io", "2.2.1-1")]);
    let after = versions(&[("docker-ce", "29.4.3-1"), ("containerd.io", "2.2.3-1")]);
    let mut result = docker_packages_changed(&before, &after);
    result.sort();
    assert_eq!(
        result,
        vec!["containerd.io".to_string(), "docker-ce".to_string()]
    );
}

#[test]
fn docker_changed_detects_newly_installed_package() {
    let before = versions(&[]);
    let after = versions(&[("docker-ce", "29.4.3-1")]);
    assert_eq!(
        docker_packages_changed(&before, &after),
        vec!["docker-ce".to_string()]
    );
}

#[test]
fn docker_changed_ignores_removed_package() {
    let before = versions(&[("docker-ce", "29.3.0-1")]);
    let after = versions(&[]);
    assert!(docker_packages_changed(&before, &after).is_empty());
}
