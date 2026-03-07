use std::fs;

use tempfile::TempDir;

#[test]
fn save_token_creates_new_file() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join("fleet.env.toml");

    iron::login::save_cloudflare_token(&env_path, "test-token-123").unwrap();

    let content = fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("[fleet]"));
    assert!(content.contains("cloudflare_api_token = \"test-token-123\""));
}

#[test]
fn save_token_preserves_existing_keys() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join("fleet.env.toml");
    fs::write(
        &env_path,
        "[fleet]\nghcr_token = \"ghcr-abc\"\n\n[apps.site]\nAPI_KEY = \"secret\"\n",
    )
    .unwrap();

    iron::login::save_cloudflare_token(&env_path, "cf-token-456").unwrap();

    let content = fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("ghcr_token = \"ghcr-abc\""));
    assert!(content.contains("cloudflare_api_token = \"cf-token-456\""));
    assert!(content.contains("API_KEY = \"secret\""));
}

#[test]
fn save_token_overwrites_existing_token() {
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join("fleet.env.toml");
    fs::write(&env_path, "[fleet]\ncloudflare_api_token = \"old-token\"\n").unwrap();

    iron::login::save_cloudflare_token(&env_path, "new-token").unwrap();

    let content = fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("cloudflare_api_token = \"new-token\""));
    assert!(!content.contains("old-token"));
}
