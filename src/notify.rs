use serde::Serialize;

use crate::config::FleetSecrets;

#[derive(Debug, Clone, Copy)]
pub enum EventLevel {
    Success,
    Failure,
    Info,
}

impl EventLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
            Self::Info => "info",
        }
    }

    pub fn discord_color(self) -> u32 {
        match self {
            Self::Success => 0x002e_cc71,
            Self::Failure => 0x00e7_4c3c,
            Self::Info => 0x0034_98db,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    pub level: EventLevel,
    pub source: &'static str,
    pub app: String,
    pub action: String,
    pub server: String,
    pub msg: String,
}

impl Event {
    pub fn deploy_started(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: app.into(),
            action: "Deploying".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn deploy_completed(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Success,
            source: "iron",
            app: app.into(),
            action: "Deploy complete".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn deploy_failed(app: &str, server: &str, error: &str) -> Self {
        Self {
            level: EventLevel::Failure,
            source: "iron",
            app: app.into(),
            action: "Deploy failed".into(),
            server: server.into(),
            msg: error.into(),
        }
    }

    pub fn app_stopped(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: app.into(),
            action: "Stopped".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn app_restarted(app: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: app.into(),
            action: "Restarted".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn app_removed(app: &str, servers: &[String]) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: app.into(),
            action: "Removed".into(),
            server: servers.join(", "),
            msg: String::new(),
        }
    }

    pub fn check_issue(server: &str, issues: &[String]) -> Self {
        Self {
            level: EventLevel::Failure,
            source: "iron",
            app: "infra".into(),
            action: "Issues detected".into(),
            server: server.into(),
            msg: issues.join("\n"),
        }
    }

    pub fn runner_deploy_started(name: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: format!("runner-{name}"),
            action: "Deploying".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn runner_deploy_completed(name: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Success,
            source: "iron",
            app: format!("runner-{name}"),
            action: "Deploy complete".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn runner_deploy_failed(name: &str, server: &str, error: &str) -> Self {
        Self {
            level: EventLevel::Failure,
            source: "iron",
            app: format!("runner-{name}"),
            action: "Deploy failed".into(),
            server: server.into(),
            msg: error.into(),
        }
    }

    pub fn runner_removed(name: &str, server: &str) -> Self {
        Self {
            level: EventLevel::Info,
            source: "iron",
            app: format!("runner-{name}"),
            action: "Removed".into(),
            server: server.into(),
            msg: String::new(),
        }
    }

    pub fn upgrade_completed(server: &str, mode: &str, packages_changed: usize) -> Self {
        Self {
            level: EventLevel::Success,
            source: "iron",
            app: "infra".into(),
            action: "Packages upgraded".into(),
            server: server.into(),
            msg: format!("{mode}: {packages_changed} package(s) changed"),
        }
    }

    pub fn upgrade_failed(server: &str, mode: &str, error: &str) -> Self {
        Self {
            level: EventLevel::Failure,
            source: "iron",
            app: "infra".into(),
            action: "Upgrade failed".into(),
            server: server.into(),
            msg: format!("{mode}: {error}"),
        }
    }
}

pub struct Notifier {
    url: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

impl Notifier {
    pub fn from_secrets(secrets: &FleetSecrets) -> Self {
        let strip_empty = |s: &Option<String>| s.as_ref().filter(|s| !s.is_empty()).cloned();
        Self {
            url: strip_empty(&secrets.tail_url),
            user: strip_empty(&secrets.tail_user),
            password: strip_empty(&secrets.tail_password),
        }
    }

    pub async fn send(&self, event: Event) {
        let Some(url) = self.url.as_deref() else {
            return;
        };
        let _ = post_event(url, self.user.as_deref(), self.password.as_deref(), &event).await;
    }
}

#[derive(Serialize)]
pub struct WirePayload<'a> {
    pub level: &'static str,
    pub source: &'static str,
    pub app: &'a str,
    pub action: &'a str,
    pub server: &'a str,
    pub msg: &'a str,
    pub color: u32,
}

pub fn wire_payload(event: &Event) -> WirePayload<'_> {
    WirePayload {
        level: event.level.as_str(),
        source: event.source,
        app: &event.app,
        action: &event.action,
        server: &event.server,
        msg: &event.msg,
        color: event.level.discord_color(),
    }
}

pub fn ingest_endpoint(base_url: &str) -> String {
    format!(
        "{}/api/default/flow_events/_json",
        base_url.trim_end_matches('/')
    )
}

async fn post_event(
    url: &str,
    user: Option<&str>,
    password: Option<&str>,
    event: &Event,
) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::new();
    let body = vec![wire_payload(event)];
    let mut req = client.post(ingest_endpoint(url)).json(&body);
    if let (Some(u), Some(p)) = (user, password) {
        req = req.basic_auth(u, Some(p));
    }
    req.send().await?;
    Ok(())
}
