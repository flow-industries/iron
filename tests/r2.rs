#![allow(clippy::unwrap_used)]

use iron::deploy::save_app_env_secrets;
use iron::r2::{custom_domain_cname_target, s3_endpoint};

#[test]
fn s3_endpoint_uses_account_subdomain() {
    assert_eq!(
        s3_endpoint("abc123"),
        "https://abc123.r2.cloudflarestorage.com"
    );
}

#[test]
fn custom_domain_cname_target_format() {
    assert_eq!(
        custom_domain_cname_target("flow-media", "abc123"),
        "flow-media.abc123.r2.cloudflarestorage.com"
    );
}

#[test]
fn save_app_env_secrets_creates_env_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("fleet.toml");
    std::fs::write(&config_path, "").unwrap();
    let env_path = dir.path().join("fleet.env.toml");

    save_app_env_secrets(
        config_path.to_str().unwrap(),
        "auth",
        &[
            ("R2_ACCESS_KEY_ID", "AKIA-test"),
            ("R2_SECRET_ACCESS_KEY", "secret-test"),
        ],
    )
    .unwrap();

    let written = std::fs::read_to_string(&env_path).unwrap();
    assert!(written.contains("[apps.auth]"));
    assert!(written.contains(r#"R2_ACCESS_KEY_ID = "AKIA-test""#));
    assert!(written.contains(r#"R2_SECRET_ACCESS_KEY = "secret-test""#));
}

#[test]
fn save_app_env_secrets_preserves_existing_keys_and_apps() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("fleet.toml");
    std::fs::write(&config_path, "").unwrap();
    let env_path = dir.path().join("fleet.env.toml");
    std::fs::write(
        &env_path,
        r#"cloudflare_api_token = "existing-token"

[apps.other]
DB_PASSWORD = "keep-me"

[apps.auth]
EXISTING = "preserved"
"#,
    )
    .unwrap();

    save_app_env_secrets(
        config_path.to_str().unwrap(),
        "auth",
        &[("R2_ACCESS_KEY_ID", "AKIA-new")],
    )
    .unwrap();

    let written = std::fs::read_to_string(&env_path).unwrap();
    assert!(written.contains(r#"cloudflare_api_token = "existing-token""#));
    assert!(written.contains(r#"DB_PASSWORD = "keep-me""#));
    assert!(written.contains(r#"EXISTING = "preserved""#));
    assert!(written.contains(r#"R2_ACCESS_KEY_ID = "AKIA-new""#));
}

#[test]
fn save_app_env_secrets_overwrites_existing_value() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("fleet.toml");
    std::fs::write(&config_path, "").unwrap();
    let env_path = dir.path().join("fleet.env.toml");
    std::fs::write(
        &env_path,
        r#"[apps.auth]
R2_ACCESS_KEY_ID = "old"
"#,
    )
    .unwrap();

    save_app_env_secrets(
        config_path.to_str().unwrap(),
        "auth",
        &[("R2_ACCESS_KEY_ID", "rotated")],
    )
    .unwrap();

    let written = std::fs::read_to_string(&env_path).unwrap();
    assert!(written.contains(r#"R2_ACCESS_KEY_ID = "rotated""#));
    assert!(!written.contains(r#""old""#));
}
