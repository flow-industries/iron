use flow::config::FleetConfig;
use flow::server::{remove_server_from_config, write_server_to_config};

#[test]
fn add_writes_server_to_fleet_toml() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, "[servers.existing]\nhost = \"existing.example.com\"\nuser = \"deploy\"\n").unwrap();

    write_server_to_config(&path, "new-server", "new.example.com", "deploy").unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(config.servers.contains_key("new-server"));
    assert_eq!(config.servers["new-server"].host, "new.example.com");
    assert_eq!(config.servers["new-server"].user, "deploy");
}

#[test]
fn add_preserves_existing_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, r#"
[servers.flow-1]
host = "flow-1.example.com"
user = "deploy"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#).unwrap();

    write_server_to_config(&path, "flow-2", "flow-2.example.com", "deploy").unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(config.servers.contains_key("flow-1"));
    assert!(config.servers.contains_key("flow-2"));
    assert_eq!(config.apps["web"].image, "nginx:latest");
}

#[test]
fn add_rejects_duplicate_name() {
    let dir = tempfile::tempdir().unwrap();
    let fleet_path = dir.path().join("fleet.toml");
    std::fs::write(&fleet_path, r#"
[servers.flow-1]
host = "flow-1.example.com"
user = "deploy"
"#).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(flow::server::run(
        fleet_path.to_str().unwrap(),
        flow::cli::ServerCommand::Add {
            name: "flow-1".to_string(),
            host: "flow-1.example.com".to_string(),
            user: "deploy".to_string(),
            ssh_user: "root".to_string(),
        },
    ));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn remove_deletes_server() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, r#"
[servers.flow-1]
host = "flow-1.example.com"
user = "deploy"

[servers.flow-2]
host = "flow-2.example.com"
user = "deploy"
"#).unwrap();

    remove_server_from_config(&path, "flow-1").unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(!config.servers.contains_key("flow-1"));
    assert!(config.servers.contains_key("flow-2"));
}

#[test]
fn remove_rejects_referenced_server() {
    let dir = tempfile::tempdir().unwrap();
    let fleet_path = dir.path().join("fleet.toml");
    std::fs::write(&fleet_path, r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(flow::server::run(
        fleet_path.to_str().unwrap(),
        flow::cli::ServerCommand::Remove {
            name: "flow-1".to_string(),
        },
    ));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("Cannot remove"));
    assert!(err.contains("web"));
}

#[test]
fn remove_rejects_nonexistent_server() {
    let dir = tempfile::tempdir().unwrap();
    let fleet_path = dir.path().join("fleet.toml");
    std::fs::write(&fleet_path, r#"
[servers.flow-1]
host = "flow-1.example.com"
"#).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(flow::server::run(
        fleet_path.to_str().unwrap(),
        flow::cli::ServerCommand::Remove {
            name: "nonexistent".to_string(),
        },
    ));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}
