#![allow(clippy::unwrap_used)]

use iron::compose::{generate, generate_env};
use iron::config::*;

fn simple_app() -> ResolvedApp {
    ResolvedApp {
        name: "site".to_string(),
        image: "ghcr.io/flow-industries/site:latest".to_string(),
        servers: vec!["flow-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["flow.industries".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: Some("5s".to_string()),
        }),
        env: std::collections::HashMap::new(),
        services: vec![],
        ports: vec![],
        r2_buckets: vec![],
        volumes: vec![],
    }
}

#[test]
fn generate_simple_compose() {
    let app = simple_app();
    let output = generate(&app, "flow");
    assert!(output.contains("image: ghcr.io/flow-industries/site:latest"));
    assert!(output.contains("flow.strategy=rolling"));
    assert!(output.contains("networks:"));
    assert!(output.contains("flow:"));
    assert!(output.contains("wget"));
    assert!(output.contains("/health"));
}

#[test]
fn generate_recreate_strategy() {
    let mut app = simple_app();
    app.deploy_strategy = DeployStrategy::Recreate;
    let output = generate(&app, "flow");
    assert!(output.contains("flow.strategy=recreate"));
}

#[test]
fn generate_with_ports() {
    let app = ResolvedApp {
        name: "game-server".to_string(),
        image: "ghcr.io/flow-industries/game-server:latest".to_string(),
        servers: vec!["game-1".to_string()],
        port: None,
        deploy_strategy: DeployStrategy::Recreate,
        routing: None,
        env: [("REGION".into(), "eu".into())].into(),
        services: vec![],
        ports: vec![PortMapping {
            internal: 9999,
            external: 9999,
            protocol: "tcp".to_string(),
        }],
        r2_buckets: vec![],
        volumes: vec![],
    };
    let output = generate(&app, "flow");
    assert!(output.contains("\"9999:9999\""));
    assert!(!output.contains("networks:"));
}

#[test]
fn generate_with_sidecars() {
    let app = ResolvedApp {
        name: "auth".to_string(),
        image: "ghcr.io/flow-industries/auth:latest".to_string(),
        servers: vec!["flow-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["id.flow.industries".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: None,
        }),
        env: [("NODE_ENV".into(), "production".into())].into(),
        services: vec![
            ResolvedSidecar {
                name: "postgres".to_string(),
                image: "postgres:17".to_string(),
                volumes: vec!["pgdata:/var/lib/postgresql/data".to_string()],
                env: [("POSTGRES_USER".into(), "flow".into())].into(),
                healthcheck: Some("pg_isready -U flow -d flow_auth".to_string()),
                depends_on: None,
            },
            ResolvedSidecar {
                name: "backup".to_string(),
                image: "prodrigestivill/postgres-backup-local".to_string(),
                volumes: vec!["./backups:/backups".to_string()],
                env: [("POSTGRES_HOST".into(), "postgres".into())].into(),
                healthcheck: None,
                depends_on: Some("postgres".to_string()),
            },
        ],
        ports: vec![],
        r2_buckets: vec![],
        volumes: vec![],
    };
    let output = generate(&app, "flow");
    assert!(output.contains("postgres:"));
    assert!(output.contains("pg_isready"));
    assert!(output.contains("flow.watch=false"));
    assert!(output.contains("pgdata:"));
    assert!(output.contains("depends_on:"));
}

#[test]
fn generate_custom_network_name() {
    let app = simple_app();
    let output = generate(&app, "mynet");
    assert!(output.contains("- mynet"));
    assert!(output.contains("mynet:"));
    assert!(output.contains("external: true"));
}

#[test]
fn generate_env_file() {
    let app = ResolvedApp {
        name: "auth".to_string(),
        image: "test".to_string(),
        servers: vec![],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: None,
        env: [
            ("DB_PASSWORD".into(), "secret123".into()),
            ("NODE_ENV".into(), "production".into()),
        ]
        .into(),
        services: vec![ResolvedSidecar {
            name: "postgres".to_string(),
            image: "postgres:17".to_string(),
            volumes: vec![],
            env: [("POSTGRES_USER".into(), "flow".into())].into(),
            healthcheck: None,
            depends_on: None,
        }],
        ports: vec![],
        r2_buckets: vec![],
        volumes: vec![],
    };
    let env = generate_env(&app);
    assert!(env.contains("DB_PASSWORD=secret123"));
    assert!(env.contains("NODE_ENV=production"));
    assert!(env.contains("POSTGRES_USER=flow"));
}

// Regression: env vars must be loaded via env_file so that shell variables
// in the calling process (notably HOSTNAME inside the watcher container) cannot
// shadow values from .env via ${VAR} interpolation.
#[test]
fn compose_uses_env_file_not_shell_interpolation() {
    let app = ResolvedApp {
        name: "paper".to_string(),
        image: "ghcr.io/flow-industries/paper:latest".to_string(),
        servers: vec!["fl-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["paper.flow.industries".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: None,
        }),
        env: [
            ("HOSTNAME".into(), "0.0.0.0".into()),
            (
                "NEXT_PUBLIC_SITE_URL".into(),
                "https://paper.flow.industries".into(),
            ),
        ]
        .into(),
        services: vec![ResolvedSidecar {
            name: "postgres".to_string(),
            image: "postgres:17".to_string(),
            volumes: vec![],
            env: [("POSTGRES_USER".into(), "flow".into())].into(),
            healthcheck: None,
            depends_on: None,
        }],
        ports: vec![],
        r2_buckets: vec![],
        volumes: vec![],
    };
    let output = generate(&app, "flow");
    assert!(output.contains("env_file:\n      - .env"));
    assert!(!output.contains("${HOSTNAME}"));
    assert!(!output.contains("${NEXT_PUBLIC_SITE_URL}"));
    assert!(!output.contains("${POSTGRES_USER}"));
    assert!(!output.contains("environment:"));
}

#[test]
fn generate_env_hides_admin_only_keys() {
    let mut app = simple_app();
    app.env = [
        ("R2_ACCESS_KEY_ID".into(), "AKIA-public".into()),
        ("R2_SECRET_ACCESS_KEY".into(), "secret-public".into()),
        ("R2_ACCESS_KEY_TOKEN_ID".into(), "internal-token-id".into()),
        ("PORT".into(), "3000".into()),
    ]
    .into();

    let env = generate_env(&app);

    assert!(env.contains("R2_ACCESS_KEY_ID=AKIA-public"));
    assert!(env.contains("R2_SECRET_ACCESS_KEY=secret-public"));
    assert!(env.contains("PORT=3000"));
    assert!(!env.contains("R2_ACCESS_KEY_TOKEN_ID"));
    assert!(!env.contains("internal-token-id"));
}

#[test]
fn generate_with_app_volumes() {
    let mut app = simple_app();
    app.volumes = vec![
        "openobserve-data:/data".to_string(),
        "/host/path:/in/container:ro".to_string(),
    ];
    let output = generate(&app, "flow");
    assert!(output.contains("    volumes:\n"));
    assert!(output.contains("      - openobserve-data:/data\n"));
    assert!(output.contains("      - /host/path:/in/container:ro\n"));
    assert!(output.contains("\nvolumes:\n  openobserve-data:\n"));
    assert!(!output.contains("\nvolumes:\n  /host/path:\n"));
}
