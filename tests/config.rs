#![allow(clippy::unwrap_used)]

use flow::config::*;

#[test]
fn parse_minimal_config() {
    let toml_str = r#"
[servers.test-1]
host = "test-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["test-1"]
port = 80
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.domain, None);
    assert_eq!(config.servers.len(), 1);
    assert_eq!(config.servers["test-1"].host, "test-1.example.com");
    assert_eq!(config.servers["test-1"].ip, None);
    assert_eq!(config.servers["test-1"].user, "deploy");
    assert_eq!(config.apps.len(), 1);
    assert_eq!(config.apps["web"].image, "nginx:latest");
    assert_eq!(config.apps["web"].port, Some(80));
}

#[test]
fn parse_domain_field() {
    let toml_str = r#"
domain = "flow.industries"

[servers.fl-1]
host = "fl-1.flow.industries"
ip = "10.0.0.1"
user = "deploy"

[apps.web]
image = "nginx:latest"
servers = ["fl-1"]
port = 80
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.domain, Some("flow.industries".to_string()));
    assert_eq!(config.servers["fl-1"].ip, Some("10.0.0.1".to_string()));
}

#[test]
fn parse_full_config() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.flow.industries"
user = "deploy"

[apps.site]
image = "ghcr.io/flow-industries/site:latest"
servers = ["flow-1"]
port = 3000

[apps.site.routing]
routes = ["flow.industries"]
health_path = "/health"
health_interval = "5s"

[apps.game-server]
image = "ghcr.io/flow-industries/game-server:latest"
servers = ["flow-1"]
deploy_strategy = "recreate"

[[apps.game-server.ports]]
internal = 9999
external = 9999
protocol = "tcp"
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.apps["site"].routing.as_ref().unwrap().routes,
        vec!["flow.industries"]
    );
    assert_eq!(
        config.apps["game-server"].deploy_strategy,
        DeployStrategy::Recreate
    );
    assert_eq!(config.apps["game-server"].ports[0].external, 9999);
}

#[test]
fn parse_app_with_services() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.flow.industries"

[apps.auth]
image = "ghcr.io/flow-industries/auth:latest"
servers = ["flow-1"]
port = 3000

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"
volumes = ["pgdata:/var/lib/postgresql/data"]
healthcheck = "pg_isready -U flow -d flow_auth"

[[apps.auth.services]]
name = "backup"
image = "prodrigestivill/postgres-backup-local"
depends_on = "postgres"
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    let auth = &config.apps["auth"];
    assert_eq!(auth.services.len(), 2);
    assert_eq!(auth.services[0].name, "postgres");
    assert_eq!(
        auth.services[0].healthcheck,
        Some("pg_isready -U flow -d flow_auth".to_string())
    );
    assert_eq!(auth.services[1].depends_on, Some("postgres".to_string()));
}

#[test]
fn invalid_server_reference() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["nonexistent"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("nonexistent"));
}

#[test]
fn deny_unknown_fields_catches_typos() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
healt_path = "/health"
"#;
    let result: Result<FleetConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err());
}

#[test]
fn validate_app_no_servers() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = []
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no servers"));
}

#[test]
fn validate_routing_without_port() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]

[apps.web.routing]
routes = ["example.com"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("routing but no port")
    );
}

#[test]
fn validate_routing_and_ports_mutually_exclusive() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["example.com"]

[[apps.web.ports]]
internal = 9999
external = 9999
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("mutually exclusive")
    );
}

#[test]
fn validate_duplicate_routes() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["example.com"]

[apps.api]
image = "nginx:latest"
servers = ["flow-1"]
port = 3001

[apps.api.routing]
routes = ["example.com"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Duplicate route"));
}

#[test]
fn validate_sidecar_depends_on_nonexistent() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "ghcr.io/flow/auth:latest"
servers = ["flow-1"]
port = 3000

[[apps.auth.services]]
name = "backup"
image = "backup:latest"
depends_on = "nonexistent"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("nonexistent"));
}

#[test]
fn validate_empty_image() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = ""
servers = ["flow-1"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty image"));
}

#[test]
fn validate_empty_route() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = [""]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("empty route"));
}

#[test]
fn env_loaded_from_env_config() {
    let dir = tempfile::tempdir().unwrap();
    let fleet_path = dir.path().join("fleet.toml");
    let env_path = dir.path().join("fleet.env.toml");

    std::fs::write(
        &fleet_path,
        r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#,
    )
    .unwrap();

    std::fs::write(
        &env_path,
        r#"
[apps.web]
NODE_ENV = "production"
SECRET_KEY = "abc123"
"#,
    )
    .unwrap();

    let fleet = load(fleet_path.to_str().unwrap()).unwrap();
    let web = &fleet.apps["web"];
    assert_eq!(web.env.get("NODE_ENV").unwrap(), "production");
    assert_eq!(web.env.get("SECRET_KEY").unwrap(), "abc123");
}

#[test]
fn validate_server_invalid_ip() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"
ip = "256.1.1.1"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("invalid IP"));
}

#[test]
fn validate_server_ip_not_a_number() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"
ip = "not-an-ip"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("invalid IP"));
}

#[test]
fn validate_server_ip_none_is_valid() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_ok());
}

#[test]
fn validate_port_protocol_invalid() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.game]
image = "game:latest"
servers = ["flow-1"]

[[apps.game.ports]]
internal = 9999
external = 9999
protocol = "invalid"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("invalid port protocol")
    );
}

#[test]
fn validate_port_protocol_udp() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.game]
image = "game:latest"
servers = ["flow-1"]

[[apps.game.ports]]
internal = 9999
external = 9999
protocol = "udp"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_ok());
}

#[test]
fn validate_health_path_no_leading_slash() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["example.com"]
health_path = "health"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("health_path"));
    assert!(err.contains("must start with /"));
}

#[test]
fn validate_health_interval_invalid() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["example.com"]
health_interval = "five-seconds"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("health_interval"));
}

#[test]
fn validate_health_interval_valid_formats() {
    for interval in &["5s", "500ms", "1m", "1.5s", "2h", "1d"] {
        let toml_str = format!(
            r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["example.com"]
health_interval = "{interval}"
"#
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fleet.toml");
        std::fs::write(&path, &toml_str).unwrap();
        let result = load(path.to_str().unwrap());
        assert!(result.is_ok(), "Expected '{interval}' to be valid");
    }
}

#[test]
fn validate_route_with_whitespace() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["flow .industries"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("whitespace"));
}

#[test]
fn validate_route_with_protocol() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["https://flow.industries"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("protocol prefix"));
}

#[test]
fn validate_route_no_dot() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 3000

[apps.web.routing]
routes = ["localhost"]
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no domain"));
}

#[test]
fn parse_server_ssh_key() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"
ssh_key = "~/.ssh/custom_key.pub"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 80
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(
        config.servers["flow-1"].ssh_key,
        Some("~/.ssh/custom_key.pub".to_string())
    );
}

#[test]
fn parse_fleet_ssh_key() {
    let toml_str = r#"
ssh_key = "~/.ssh/fleet_key.pub"

[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 80
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.ssh_key, Some("~/.ssh/fleet_key.pub".to_string()));
    assert_eq!(config.servers["flow-1"].ssh_key, None);
}

#[test]
fn parse_ssh_key_defaults_to_none() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.web]
image = "nginx:latest"
servers = ["flow-1"]
port = 80
"#;
    let config: FleetConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.ssh_key, None);
    assert_eq!(config.servers["flow-1"].ssh_key, None);
}

#[test]
fn validate_duplicate_sidecar_names() {
    let toml_str = r#"
[servers.flow-1]
host = "flow-1.example.com"

[apps.auth]
image = "auth:latest"
servers = ["flow-1"]
port = 3000

[[apps.auth.services]]
name = "postgres"
image = "postgres:17"

[[apps.auth.services]]
name = "postgres"
image = "postgres:16"
"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(&path, toml_str).unwrap();
    let result = load(path.to_str().unwrap());
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("duplicate service name")
    );
}
