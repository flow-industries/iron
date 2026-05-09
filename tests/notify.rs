#![allow(clippy::unwrap_used)]

use iron::config::FleetSecrets;
use iron::notify::*;

#[test]
fn notifier_disabled_when_no_secrets() {
    let secrets = FleetSecrets::default();
    let notifier = Notifier::from_secrets(&secrets);
    notifier.send(Event::deploy_started("web", "fl-1"));
}

#[test]
fn notifier_ignores_empty_strings() {
    let secrets = FleetSecrets {
        tail_url: Some(String::new()),
        tail_user: Some(String::new()),
        tail_password: Some(String::new()),
        ..Default::default()
    };
    let notifier = Notifier::from_secrets(&secrets);
    notifier.send(Event::deploy_started("web", "fl-1"));
}

#[test]
fn notifier_constructed_with_all_fields() {
    let secrets = FleetSecrets {
        tail_url: Some("https://tail.example.com".into()),
        tail_user: Some("test@example.com".into()),
        tail_password: Some("p".into()),
        ..Default::default()
    };
    let _notifier = Notifier::from_secrets(&secrets);
}

#[test]
fn ingest_endpoint_strips_trailing_slash() {
    assert_eq!(
        ingest_endpoint("https://tail.example.com"),
        "https://tail.example.com/api/default/flow_events/_json"
    );
    assert_eq!(
        ingest_endpoint("https://tail.example.com/"),
        "https://tail.example.com/api/default/flow_events/_json"
    );
}

#[test]
fn event_constructors_set_structured_fields() {
    let e = Event::deploy_started("paper", "fl-1");
    assert!(matches!(e.level, EventLevel::Info));
    assert_eq!(e.source, "iron");
    assert_eq!(e.app, "paper");
    assert_eq!(e.action, "Deploying");
    assert_eq!(e.server, "fl-1");
    assert_eq!(e.msg, "");

    let e = Event::deploy_completed("paper", "fl-1");
    assert!(matches!(e.level, EventLevel::Success));
    assert_eq!(e.action, "Deploy complete");

    let e = Event::deploy_failed("paper", "fl-1", "connection refused");
    assert!(matches!(e.level, EventLevel::Failure));
    assert_eq!(e.action, "Deploy failed");
    assert_eq!(e.msg, "connection refused");

    let e = Event::app_stopped("paper", "fl-1");
    assert_eq!(e.action, "Stopped");

    let e = Event::app_restarted("paper", "fl-1");
    assert_eq!(e.action, "Restarted");

    let e = Event::app_removed("paper", &["fl-1".into(), "fl-2".into()]);
    assert_eq!(e.action, "Removed");
    assert_eq!(e.server, "fl-1, fl-2");

    let e = Event::check_issue("fl-1", &["container missing".into(), "caddy stale".into()]);
    assert!(matches!(e.level, EventLevel::Failure));
    assert_eq!(e.app, "infra");
    assert_eq!(e.action, "Issues detected");
    assert!(e.msg.contains("container missing"));
    assert!(e.msg.contains("caddy stale"));

    let e = Event::runner_deploy_started("ci", "fl-1");
    assert_eq!(e.app, "runner-ci");
    assert_eq!(e.action, "Deploying");

    let e = Event::runner_deploy_failed("ci", "fl-1", "ssh closed");
    assert!(matches!(e.level, EventLevel::Failure));
    assert_eq!(e.app, "runner-ci");
    assert_eq!(e.msg, "ssh closed");

    let e = Event::runner_removed("ci", "fl-1");
    assert_eq!(e.app, "runner-ci");
    assert_eq!(e.action, "Removed");
}

#[test]
fn event_levels_distinct_colors() {
    assert_ne!(
        EventLevel::Success.discord_color(),
        EventLevel::Failure.discord_color()
    );
    assert_ne!(
        EventLevel::Success.discord_color(),
        EventLevel::Info.discord_color()
    );
    assert_ne!(
        EventLevel::Failure.discord_color(),
        EventLevel::Info.discord_color()
    );
}

#[test]
fn event_levels_have_expected_string() {
    assert_eq!(EventLevel::Info.as_str(), "info");
    assert_eq!(EventLevel::Success.as_str(), "success");
    assert_eq!(EventLevel::Failure.as_str(), "failure");
}

#[test]
fn wire_payload_serializes_with_all_fields() {
    let event = Event::deploy_failed("paper", "fl-1", "connection refused");
    let payload = wire_payload(&event);
    let json = serde_json::to_value(&payload).unwrap();

    assert_eq!(json["level"], "failure");
    assert_eq!(json["source"], "iron");
    assert_eq!(json["app"], "paper");
    assert_eq!(json["action"], "Deploy failed");
    assert_eq!(json["server"], "fl-1");
    assert_eq!(json["msg"], "connection refused");
    assert_eq!(json["color"], EventLevel::Failure.discord_color());
}
