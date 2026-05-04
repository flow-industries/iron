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

#[test]
fn generate_with_primary_volumes() {
    let app = ResolvedApp {
        name: "pds".to_string(),
        image: "ghcr.io/bluesky-social/pds:latest".to_string(),
        servers: vec!["fl-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["pds.flow.talk".to_string()],
            health_path: Some("/xrpc/_health".to_string()),
            health_interval: None,
        }),
        env: std::collections::HashMap::new(),
        services: vec![ResolvedSidecar {
            name: "postgres".to_string(),
            image: "postgres:17".to_string(),
            volumes: vec!["pdspg:/var/lib/postgresql/data".to_string()],
            env: std::collections::HashMap::new(),
            healthcheck: Some("pg_isready".to_string()),
            depends_on: None,
        }],
        ports: vec![],
        r2_buckets: vec![],
        volumes: vec!["pdsdata:/pds".to_string()],
    };
    let output = generate(&app, "flow");
    let pds_section = output.split("\n  postgres:").next().unwrap();
    assert!(pds_section.contains("    volumes:"));
    assert!(pds_section.contains("- pdsdata:/pds"));
    assert!(output.contains("\nvolumes:\n"));
    assert!(output.contains("  pdsdata:"));
    assert!(output.contains("  pdspg:"));
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
