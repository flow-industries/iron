use std::fs;

use tempfile::TempDir;

#[test]
fn does_nothing_when_fleet_toml_exists() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(&path, "existing content").unwrap();

    iron::init::run(path.to_str().unwrap()).unwrap();

    let content = fs::read_to_string(&path).unwrap();
    assert_eq!(content, "existing content");
}
