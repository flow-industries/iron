use std::fs;

use tempfile::TempDir;

#[test]
fn creates_fleet_toml_when_missing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");

    flow::init::run(path.to_str().unwrap()).unwrap();

    assert!(path.exists());
    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("[servers.my-server]"));
    assert!(content.contains("[apps.my-app]"));
}

#[test]
fn does_nothing_when_fleet_toml_exists() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(&path, "existing content").unwrap();

    flow::init::run(path.to_str().unwrap()).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "existing content");
}
