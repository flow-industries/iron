#![allow(clippy::unwrap_used)]

use std::collections::HashMap;

use iron::caddy::generate;
use iron::config::*;

#[test]
fn generate_caddy_fragment() {
    let app = ResolvedApp {
        name: "site".to_string(),
        image: "test".to_string(),
        servers: vec![],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["flow.industries".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: Some("5s".to_string()),
        }),
        env: HashMap::default(),
        services: vec![],
        ports: vec![],
    };
    let fragment = generate(&app).unwrap();
    assert!(fragment.contains("flow.industries {"));
    assert!(fragment.contains("reverse_proxy site:3000"));
    assert!(fragment.contains("health_uri /health"));
    assert!(fragment.contains("health_interval 5s"));
    assert!(fragment.contains("lb_try_duration 10s"));
}

#[test]
fn no_fragment_without_routing() {
    let app = ResolvedApp {
        name: "game-server".to_string(),
        image: "test".to_string(),
        servers: vec![],
        port: None,
        deploy_strategy: DeployStrategy::Recreate,
        routing: None,
        env: HashMap::default(),
        services: vec![],
        ports: vec![],
    };
    assert!(generate(&app).is_none());
}

#[test]
fn multiple_domains() {
    let app = ResolvedApp {
        name: "talk".to_string(),
        image: "test".to_string(),
        servers: vec![],
        port: Some(3000),
        deploy_strategy: DeployStrategy::Rolling,
        routing: Some(Routing {
            domains: vec!["flow.talk".to_string(), "www.flow.talk".to_string()],
            health_path: Some("/health".to_string()),
            health_interval: None,
        }),
        env: HashMap::default(),
        services: vec![],
        ports: vec![],
    };
    let fragment = generate(&app).unwrap();
    assert!(fragment.contains("flow.talk {"));
    assert!(fragment.contains("www.flow.talk {"));
    assert!(fragment.contains("health_interval 5s"));
}
