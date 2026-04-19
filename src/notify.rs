use serde::Serialize;

use crate::config::FleetSecrets;

#[derive(Debug, Clone, Copy)]
pub enum EventLevel {
    Success,
    Failure,
    Info,
}

pub struct Event {
    pub level: EventLevel,
    pub title: String,
    pub description: String,
}

impl Event {
    pub fn deploy_started(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            title: format!("Deploying {app}"),
            description: format!("{app} on {server}"),
        }
    }

    pub fn deploy_completed(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Success,
            title: format!("Deploy complete: {app}"),
            description: format!("{app} deployed to {server}"),
        }
    }

    pub fn deploy_failed(app: &str, server: &str, error: &str) -> Self {
        Self {
            level: EventLevel::Failure,
            title: format!("Deploy failed: {app}"),
            description: format!("{app} on {server}: {error}"),
        }
    }

    pub fn app_stopped(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            title: format!("Stopped: {app}"),
            description: format!("{app} stopped on {server}"),
        }
    }

    pub fn app_restarted(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            title: format!("Restarted: {app}"),
            description: format!("{app} restarted on {server}"),
        }
    }

    pub fn app_removed(app: &str, servers: &[String]) -> Self {
        Self {
            level: EventLevel::Info,
            title: format!("Removed: {app}"),
            description: format!("{app} removed from {}", servers.join(", ")),
        }
    }

    pub fn check_issue(server: &str, issues: &[String]) -> Self {
        Self {
            level: EventLevel::Failure,
            title: format!("Issues on {server}"),
            description: issues.join("\n"),
        }
    }
}

pub struct Notifier {
    discord_webhook_url: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
}

impl Notifier {
    pub fn from_secrets(secrets: &FleetSecrets) -> Self {
        Self {
            discord_webhook_url: secrets
                .discord_webhook_url
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
            telegram_bot_token: secrets
                .telegram_bot_token
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
            telegram_chat_id: secrets
                .telegram_chat_id
                .as_ref()
                .filter(|s| !s.is_empty())
                .cloned(),
        }
    }

    pub fn send(&self, event: Event) {
        let has_discord = self.discord_webhook_url.is_some();
        let has_telegram = self.telegram_bot_token.is_some() && self.telegram_chat_id.is_some();

        if !has_discord && !has_telegram {
            return;
        }

        let discord_url = self.discord_webhook_url.clone();
        let tg_token = self.telegram_bot_token.clone();
        let tg_chat = self.telegram_chat_id.clone();

        tokio::spawn(async move {
            let client = reqwest::Client::new();

            if let Some(url) = discord_url {
                let _ = send_discord(&client, &url, &event).await;
            }

            if let (Some(token), Some(chat_id)) = (tg_token, tg_chat) {
                let _ = send_telegram(&client, &token, &chat_id, &event).await;
            }
        });
    }
}

#[derive(Serialize)]
pub struct DiscordEmbed {
    pub title: String,
    pub description: String,
    pub color: u32,
}

#[derive(Serialize)]
pub struct DiscordPayload {
    pub embeds: Vec<DiscordEmbed>,
}

#[derive(Serialize)]
pub struct TelegramPayload {
    pub chat_id: String,
    pub text: String,
    pub parse_mode: String,
}

pub fn embed_color(level: EventLevel) -> u32 {
    match level {
        EventLevel::Success => 0x002e_cc71,
        EventLevel::Failure => 0x00e7_4c3c,
        EventLevel::Info => 0x0034_98db,
    }
}

pub fn discord_payload(event: &Event) -> DiscordPayload {
    DiscordPayload {
        embeds: vec![DiscordEmbed {
            title: event.title.clone(),
            description: event.description.clone(),
            color: embed_color(event.level),
        }],
    }
}

pub fn telegram_text(event: &Event) -> String {
    let emoji = match event.level {
        EventLevel::Success => "\u{2705}",
        EventLevel::Failure => "\u{274c}",
        EventLevel::Info => "\u{2139}\u{fe0f}",
    };
    format!("{} <b>{}</b>\n{}", emoji, event.title, event.description)
}

pub fn telegram_payload(chat_id: &str, event: &Event) -> TelegramPayload {
    TelegramPayload {
        chat_id: chat_id.to_string(),
        text: telegram_text(event),
        parse_mode: "HTML".to_string(),
    }
}

async fn send_discord(
    client: &reqwest::Client,
    url: &str,
    event: &Event,
) -> Result<(), reqwest::Error> {
    client
        .post(url)
        .json(&discord_payload(event))
        .send()
        .await?;
    Ok(())
}

async fn send_telegram(
    client: &reqwest::Client,
    token: &str,
    chat_id: &str,
    event: &Event,
) -> Result<(), reqwest::Error> {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    client
        .post(&url)
        .json(&telegram_payload(chat_id, event))
        .send()
        .await?;
    Ok(())
}
