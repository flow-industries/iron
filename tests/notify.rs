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
fn notifier_enabled_with_discord_only() {
    let secrets = FleetSecrets {
        discord_webhook_url: Some("https://discord.com/api/webhooks/123/abc".to_string()),
        ..Default::default()
    };
    let _notifier = Notifier::from_secrets(&secrets);
}

#[test]
fn notifier_enabled_with_telegram_only() {
    let secrets = FleetSecrets {
        telegram_bot_token: Some("123:abc".to_string()),
        telegram_chat_id: Some("-100123".to_string()),
        ..Default::default()
    };
    let _notifier = Notifier::from_secrets(&secrets);
}

#[test]
fn notifier_ignores_empty_strings() {
    let secrets = FleetSecrets {
        discord_webhook_url: Some(String::new()),
        telegram_bot_token: Some(String::new()),
        telegram_chat_id: Some(String::new()),
        ..Default::default()
    };
    let notifier = Notifier::from_secrets(&secrets);
    notifier.send(Event::deploy_started("web", "fl-1"));
}

#[test]
fn discord_payload_success_event() {
    let event = Event::deploy_completed("site", "fl-1");
    let payload = discord_payload(&event);

    assert_eq!(payload.embeds.len(), 1);
    assert_eq!(payload.embeds[0].color, embed_color(EventLevel::Success));
    assert!(payload.embeds[0].title.contains("site"));
    assert!(payload.embeds[0].description.contains("fl-1"));
}

#[test]
fn discord_payload_failure_event() {
    let event = Event::deploy_failed("site", "fl-1", "timeout");
    let payload = discord_payload(&event);

    assert_eq!(payload.embeds[0].color, embed_color(EventLevel::Failure));
    assert!(payload.embeds[0].description.contains("timeout"));
}

#[test]
fn discord_payload_info_event() {
    let event = Event::deploy_started("site", "fl-1");
    let payload = discord_payload(&event);

    assert_eq!(payload.embeds[0].color, embed_color(EventLevel::Info));
}

#[test]
fn telegram_text_contains_emoji_and_html() {
    let event = Event::deploy_completed("site", "fl-1");
    let text = telegram_text(&event);

    assert!(text.contains("\u{2705}"));
    assert!(text.contains("<b>"));
    assert!(text.contains("site"));
    assert!(text.contains("fl-1"));
}

#[test]
fn telegram_text_failure_emoji() {
    let event = Event::deploy_failed("api", "fl-2", "connection refused");
    let text = telegram_text(&event);

    assert!(text.contains("\u{274c}"));
    assert!(text.contains("connection refused"));
}

#[test]
fn telegram_payload_structure() {
    let event = Event::app_stopped("web", "fl-1");
    let payload = telegram_payload("-100123", &event);

    assert_eq!(payload.chat_id, "-100123");
    assert_eq!(payload.parse_mode, "HTML");
    assert!(payload.text.contains("web"));
}

#[test]
fn embed_colors_are_distinct() {
    let success = embed_color(EventLevel::Success);
    let failure = embed_color(EventLevel::Failure);
    let info = embed_color(EventLevel::Info);

    assert_ne!(success, failure);
    assert_ne!(success, info);
    assert_ne!(failure, info);
}

#[test]
fn event_constructors() {
    let e = Event::deploy_started("web", "fl-1");
    assert!(matches!(e.level, EventLevel::Info));

    let e = Event::deploy_completed("web", "fl-1");
    assert!(matches!(e.level, EventLevel::Success));

    let e = Event::deploy_failed("web", "fl-1", "err");
    assert!(matches!(e.level, EventLevel::Failure));

    let e = Event::app_stopped("web", "fl-1");
    assert!(matches!(e.level, EventLevel::Info));

    let e = Event::app_restarted("web", "fl-1");
    assert!(matches!(e.level, EventLevel::Info));

    let e = Event::app_removed("web", &["fl-1".to_string(), "fl-2".to_string()]);
    assert!(matches!(e.level, EventLevel::Info));
    assert!(e.description.contains("fl-1"));
    assert!(e.description.contains("fl-2"));

    let e = Event::check_issue("fl-1", &["container missing".to_string()]);
    assert!(matches!(e.level, EventLevel::Failure));
    assert!(e.description.contains("container missing"));
}

#[test]
fn discord_payload_serializes_to_json() {
    let event = Event::deploy_completed("site", "fl-1");
    let payload = discord_payload(&event);
    let json = serde_json::to_value(&payload).unwrap();

    assert!(json["embeds"].is_array());
    assert_eq!(json["embeds"][0]["color"], embed_color(EventLevel::Success));
    assert!(
        json["embeds"][0]["title"]
            .as_str()
            .unwrap()
            .contains("site")
    );
}

#[test]
fn telegram_payload_serializes_to_json() {
    let event = Event::app_restarted("api", "fl-1");
    let payload = telegram_payload("-999", &event);
    let json = serde_json::to_value(&payload).unwrap();

    assert_eq!(json["chat_id"], "-999");
    assert_eq!(json["parse_mode"], "HTML");
    assert!(json["text"].as_str().unwrap().contains("api"));
}
