#![allow(clippy::unwrap_used)]

use iron::app::{
    ParsedPortMap, remove_service_from_config, write_app_to_config, write_service_to_config,
};
use iron::config::FleetConfig;

fn fleet_with_server() -> &'static str {
    r#"
[servers.flow-1]
host = "flow-1.example.com"
user = "deploy"
"#
}

#[test]
fn add_writes_basic_app() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    write_app_to_config(
        &path,
        "worker",
        "ghcr.io/org/worker:latest",
        &["flow-1".to_string()],
        None,
        &[],
        None,
        None,
        &[],
        "rolling",
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(config.apps.contains_key("worker"));
    assert_eq!(config.apps["worker"].image, "ghcr.io/org/worker:latest");
    assert_eq!(config.apps["worker"].servers, vec!["flow-1"]);
    assert_eq!(config.apps["worker"].port, None);
}

#[test]
fn add_writes_app_with_routing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    write_app_to_config(
        &path,
        "site",
        "ghcr.io/org/site:latest",
        &["flow-1".to_string()],
        Some(3000),
        &["example.com".to_string(), "www.example.com".to_string()],
        Some("/health"),
        Some("5s"),
        &[],
        "rolling",
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    let app = &config.apps["site"];
    assert_eq!(app.port, Some(3000));
    let routing = app.routing.as_ref().unwrap();
    assert_eq!(routing.domains, vec!["example.com", "www.example.com"]);
    assert_eq!(routing.health_path, Some("/health".to_string()));
    assert_eq!(routing.health_interval, Some("5s".to_string()));
}

#[test]
fn add_writes_app_with_port_maps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    write_app_to_config(
        &path,
        "game",
        "ghcr.io/org/game:latest",
        &["flow-1".to_string()],
        None,
        &[],
        None,
        None,
        &[
            ParsedPortMap {
                internal: 9999,
                external: 9999,
                protocol: "tcp".to_string(),
            },
            ParsedPortMap {
                internal: 8888,
                external: 8888,
                protocol: "udp".to_string(),
            },
        ],
        "recreate",
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    let app = &config.apps["game"];
    assert_eq!(app.ports.len(), 2);
    assert_eq!(app.ports[0].internal, 9999);
    assert_eq!(app.ports[0].external, 9999);
    assert_eq!(app.ports[0].protocol, "tcp");
    assert_eq!(app.ports[1].internal, 8888);
    assert_eq!(app.ports[1].protocol, "udp");
    assert_eq!(app.deploy_strategy, iron::config::DeployStrategy::Recreate);
}

#[test]
fn add_preserves_existing_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"
user = "deploy"

[apps.existing]
image = "nginx:latest"
servers = ["flow-1"]
port = 80
"#,
    )
    .unwrap();

    write_app_to_config(
        &path,
        "new-app",
        "redis:7",
        &["flow-1".to_string()],
        None,
        &[],
        None,
        None,
        &[],
        "rolling",
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(config.apps.contains_key("existing"));
    assert!(config.apps.contains_key("new-app"));
    assert!(config.servers.contains_key("flow-1"));
}

#[test]
fn add_rejects_duplicate_app_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.site]
image = "nginx:latest"
servers = ["flow-1"]
"#,
    )
    .unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::Add {
            name: Some("site".to_string()),
            image: Some("nginx:latest".to_string()),
            server: vec!["flow-1".to_string()],
            port: None,
            domain: vec![],
            health_path: None,
            health_interval: None,
            port_map: vec![],
            deploy_strategy: Some("rolling".to_string()),
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn add_rejects_unknown_server() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::Add {
            name: Some("site".to_string()),
            image: Some("nginx:latest".to_string()),
            server: vec!["nonexistent".to_string()],
            port: None,
            domain: vec![],
            health_path: None,
            health_interval: None,
            port_map: vec![],
            deploy_strategy: Some("rolling".to_string()),
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn add_rejects_routing_without_port() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::Add {
            name: Some("site".to_string()),
            image: Some("nginx:latest".to_string()),
            server: vec!["flow-1".to_string()],
            port: None,
            domain: vec!["example.com".to_string()],
            health_path: None,
            health_interval: None,
            port_map: vec![],
            deploy_strategy: Some("rolling".to_string()),
        },
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("--port is required when using --domain")
    );
}

#[test]
fn add_rejects_routing_with_port_maps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::Add {
            name: Some("site".to_string()),
            image: Some("nginx:latest".to_string()),
            server: vec!["flow-1".to_string()],
            port: Some(3000),
            domain: vec!["example.com".to_string()],
            health_path: None,
            health_interval: None,
            port_map: vec!["9999:9999".to_string()],
            deploy_strategy: Some("rolling".to_string()),
        },
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("mutually exclusive")
    );
}

#[test]
fn add_rejects_health_without_route() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::Add {
            name: Some("site".to_string()),
            image: Some("nginx:latest".to_string()),
            server: vec!["flow-1".to_string()],
            port: Some(3000),
            domain: vec![],
            health_path: Some("/health".to_string()),
            health_interval: None,
            port_map: vec![],
            deploy_strategy: Some("rolling".to_string()),
        },
    );
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("--health-path and --health-interval require --domain")
    );
}

#[test]
fn add_roundtrips_through_config_parse() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    write_app_to_config(
        &path,
        "site",
        "ghcr.io/org/site:latest",
        &["flow-1".to_string()],
        Some(3000),
        &["example.com".to_string()],
        Some("/health"),
        Some("5s"),
        &[],
        "rolling",
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let _config: FleetConfig = toml::from_str(&content).unwrap();
}

#[test]
fn add_service_writes_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]
port = 3000
"#,
    )
    .unwrap();

    write_service_to_config(
        &path,
        "auth",
        "postgres",
        "postgres:17",
        &["pgdata:/var/lib/postgresql/data".to_string()],
        Some("pg_isready -U flow"),
        None,
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert_eq!(config.apps["auth"].services.len(), 1);
    assert_eq!(config.apps["auth"].services[0].name, "postgres");
    assert_eq!(config.apps["auth"].services[0].image, "postgres:17");
    assert_eq!(
        config.apps["auth"].services[0].volumes,
        vec!["pgdata:/var/lib/postgresql/data"]
    );
    assert_eq!(
        config.apps["auth"].services[0].healthcheck,
        Some("pg_isready -U flow".to_string())
    );
}

#[test]
fn add_service_appends_to_existing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]
port = 3000

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"
"#,
    )
    .unwrap();

    write_service_to_config(
        &path,
        "auth",
        "backup",
        "prodrigestivill/postgres-backup-local",
        &["./backups:/backups".to_string()],
        None,
        Some("postgres"),
    )
    .unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert_eq!(config.apps["auth"].services.len(), 2);
    assert_eq!(config.apps["auth"].services[1].name, "backup");
    assert_eq!(
        config.apps["auth"].services[1].depends_on,
        Some("postgres".to_string())
    );
}

#[test]
fn add_service_rejects_unknown_app() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, fleet_with_server()).unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::AddService {
            app: "nonexistent".to_string(),
            name: "postgres".to_string(),
            image: "postgres:17".to_string(),
            volume: vec![],
            healthcheck: None,
            depends_on: None,
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
fn add_service_rejects_duplicate_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"
"#,
    )
    .unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::AddService {
            app: "auth".to_string(),
            name: "postgres".to_string(),
            image: "postgres:17".to_string(),
            volume: vec![],
            healthcheck: None,
            depends_on: None,
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already exists"));
}

#[test]
fn remove_service_deletes_sidecar() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"

[[apps.auth.services]]
name = "backup"
image = "backup:latest"
"#,
    )
    .unwrap();

    remove_service_from_config(&path, "auth", "postgres").unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert_eq!(config.apps["auth"].services.len(), 1);
    assert_eq!(config.apps["auth"].services[0].name, "backup");
}

#[test]
fn remove_service_cleans_up_empty_array() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"
"#,
    )
    .unwrap();

    remove_service_from_config(&path, "auth", "postgres").unwrap();

    let content = std::fs::read_to_string(&path).unwrap();
    let config: FleetConfig = toml::from_str(&content).unwrap();
    assert!(config.apps["auth"].services.is_empty());
    assert!(!content.contains("services"));
}

#[test]
fn remove_service_rejects_unknown_service() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/org/auth:latest"
servers = ["flow-1"]

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"
"#,
    )
    .unwrap();

    let result = iron::app::run(
        path.to_str().unwrap(),
        iron::cli::AppCommand::RemoveService {
            app: "auth".to_string(),
            name: "nonexistent".to_string(),
        },
    );
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}
