use iron::config::{DeployStrategy, ResolvedApp, ResolvedSidecar};
use iron::db::find_postgres;
use std::collections::HashMap;

fn make_app(services: Vec<ResolvedSidecar>) -> ResolvedApp {
    ResolvedApp {
        name: "test".to_string(),
        image: "ghcr.io/org/test:latest".to_string(),
        servers: vec!["srv-1".to_string()],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: None,
        env: HashMap::new(),
        services,
        ports: vec![],
    }
}

fn make_postgres(name: &str, image: &str) -> ResolvedSidecar {
    ResolvedSidecar {
        name: name.to_string(),
        image: image.to_string(),
        volumes: vec![],
        env: HashMap::new(),
        healthcheck: None,
        depends_on: None,
    }
}

#[test]
fn find_postgres_with_standard_sidecar() {
    let app = make_app(vec![make_postgres("postgres", "postgres:17")]);
    let svc = find_postgres(&app).unwrap();
    assert_eq!(svc.name, "postgres");
}

#[test]
fn find_postgres_with_custom_name() {
    let app = make_app(vec![make_postgres("db", "postgres:16-alpine")]);
    let svc = find_postgres(&app).unwrap();
    assert_eq!(svc.name, "db");
}

#[test]
fn find_postgres_errors_when_none() {
    let app = make_app(vec![make_postgres("redis", "redis:7")]);
    assert!(find_postgres(&app).is_err());
}

#[test]
fn find_postgres_skips_non_postgres() {
    let app = make_app(vec![
        make_postgres("backup", "prodrigestivill/postgres-backup-local"),
        make_postgres("postgres", "postgres:17"),
    ]);
    let svc = find_postgres(&app).unwrap();
    assert_eq!(svc.name, "postgres");
}

#[test]
fn find_postgres_no_services() {
    let app = make_app(vec![]);
    assert!(find_postgres(&app).is_err());
}
