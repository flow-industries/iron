use flow::compose::{generate, generate_env};
use flow::config::*;

fn simple_app() -> ResolvedApp {
    ResolvedApp {
        name: "site".to_string(),
        image: "ghcr.io/flow-industries/site:latest".to_string(),
        servers: vec!["flow-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            routes: vec!["flow.industries".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: Some("5s".to_string()),
        }),
        env: std::collections::HashMap::new(),
        services: vec![],
        ports: vec![],
    }
}

#[test]
fn generate_simple_compose() {
    let app = simple_app();
    let output = generate(&app);
    assert!(output.contains("image: ghcr.io/flow-industries/site:latest"));
    assert!(output.contains("wud.trigger.include=rollout"));
    assert!(output.contains("networks:"));
    assert!(output.contains("flow:"));
    assert!(output.contains("wget"));
    assert!(output.contains("/health"));
}

#[test]
fn generate_recreate_strategy() {
    let mut app = simple_app();
    app.deploy_strategy = DeployStrategy::Recreate;
    let output = generate(&app);
    assert!(output.contains("wud.trigger.include=gameupdate"));
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
    };
    let output = generate(&app);
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
            routes: vec!["id.flow.industries".to_string()],
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
    };
    let output = generate(&app);
    assert!(output.contains("postgres:"));
    assert!(output.contains("pg_isready"));
    assert!(output.contains("wud.watch=false"));
    assert!(output.contains("pgdata:"));
    assert!(output.contains("depends_on:"));
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
    };
    let env = generate_env(&app);
    assert!(env.contains("DB_PASSWORD=secret123"));
    assert!(env.contains("NODE_ENV=production"));
    assert!(env.contains("POSTGRES_USER=flow"));
}
