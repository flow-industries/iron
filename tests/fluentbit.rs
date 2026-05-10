#![allow(clippy::unwrap_used)]

use iron::fluentbit::{generate_compose, generate_config, parse_tail_url};

#[test]
fn parse_tail_url_https_default_port() {
    let (host, port, tls, uri) = parse_tail_url("https://tail.example.com").unwrap();
    assert_eq!(host, "tail.example.com");
    assert_eq!(port, 443);
    assert!(tls);
    assert_eq!(uri, "/api/default/app_logs/_json");
}

#[test]
fn parse_tail_url_http_default_port() {
    let (host, port, tls, _uri) = parse_tail_url("http://10.0.0.1").unwrap();
    assert_eq!(host, "10.0.0.1");
    assert_eq!(port, 80);
    assert!(!tls);
}

#[test]
fn parse_tail_url_explicit_port() {
    let (host, port, tls, _uri) = parse_tail_url("http://10.0.0.1:5080").unwrap();
    assert_eq!(host, "10.0.0.1");
    assert_eq!(port, 5080);
    assert!(!tls);
}

#[test]
fn parse_tail_url_strips_path() {
    let (host, port, tls, uri) = parse_tail_url("https://tail.example.com/some/path").unwrap();
    assert_eq!(host, "tail.example.com");
    assert_eq!(port, 443);
    assert!(tls);
    assert_eq!(uri, "/api/default/app_logs/_json");
}

#[test]
fn parse_tail_url_rejects_bad_scheme() {
    assert!(parse_tail_url("ftp://x.example.com").is_none());
}

#[test]
fn parse_tail_url_rejects_no_scheme() {
    assert!(parse_tail_url("tail.example.com").is_none());
}

#[test]
fn generate_compose_includes_essentials() {
    let yaml = generate_compose("fl-1", "test@example.com", "hunter2");
    assert!(yaml.contains("fluent/fluent-bit:"));
    assert!(yaml.contains("FLOW_SERVER: fl-1"));
    assert!(yaml.contains("TAIL_USER: test@example.com"));
    assert!(yaml.contains("TAIL_PASSWORD: hunter2"));
    assert!(yaml.contains("/var/lib/docker/containers"));
    assert!(yaml.contains("/var/run/docker.sock"));
    assert!(yaml.contains("flow.watch=false"));
}

#[test]
fn generate_config_includes_essentials() {
    let conf = generate_config("tail.example.com", 443, true, "/api/default/app_logs/_json");
    assert!(conf.contains("Name              tail"));
    assert!(conf.contains("Path              /var/lib/docker/containers/*/*-json.log"));
    assert!(conf.contains("Host              tail.example.com"));
    assert!(conf.contains("Port              443"));
    assert!(conf.contains("URI               /api/default/app_logs/_json"));
    assert!(conf.contains("tls               On"));
    assert!(conf.contains("Record            server ${FLOW_SERVER}"));
    assert!(conf.contains("Exclude           container_name"));
    assert!(conf.contains("http_user         ${TAIL_USER}"));
}

#[test]
fn generate_config_renders_tls_off_for_http() {
    let conf = generate_config("10.0.0.1", 5080, false, "/api/default/app_logs/_json");
    assert!(conf.contains("tls               Off"));
    assert!(conf.contains("Host              10.0.0.1"));
    assert!(conf.contains("Port              5080"));
}
