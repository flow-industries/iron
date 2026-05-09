use std::collections::HashSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::{Fleet, RunnerScope};
use crate::ui;

pub const IMAGE: &str = "python:3.13-alpine";

pub fn generate_compose(
    network: &str,
    secret: &str,
    server_name: &str,
    tail_url: Option<&str>,
    tail_user: Option<&str>,
    tail_password: Option<&str>,
) -> String {
    let mut extra_env = String::new();
    if let Some(url) = tail_url {
        extra_env.push_str(&format!("      TAIL_URL: {url}\n"));
    }
    if let Some(user) = tail_user {
        extra_env.push_str(&format!("      TAIL_USER: {user}\n"));
    }
    if let Some(password) = tail_password {
        extra_env.push_str(&format!("      TAIL_PASSWORD: {password}\n"));
    }

    format!(
        r#"services:
  webhook:
    image: {IMAGE}
    entrypoint: python
    command: /webhook.py
    environment:
      GITHUB_WEBHOOK_SECRET: {secret}
      BIND: 0.0.0.0:8080
      IRON_SERVER: {server_name}
{extra_env}    volumes:
      - ./webhook.py:/webhook.py:ro
    networks:
      - {network}
    restart: always
    healthcheck:
      test: ["CMD-SHELL", "python -c 'import urllib.request; urllib.request.urlopen(\"http://localhost:8080/health\").read()' >/dev/null 2>&1 || exit 1"]
      interval: 30s
      timeout: 5s
      retries: 3

networks:
  {network}:
    external: true
"#
    )
}

pub fn generate_script() -> &'static str {
    include_str!("../stacks/webhook/webhook.py")
}

pub fn generate_caddy_fragment(domain: &str) -> String {
    format!("{domain} {{\n    reverse_proxy webhook:8080\n}}\n")
}

#[derive(Serialize)]
struct HookConfig {
    url: String,
    content_type: &'static str,
    secret: String,
    insecure_ssl: &'static str,
}

#[derive(Serialize)]
struct CreateHookBody {
    name: &'static str,
    events: Vec<&'static str>,
    active: bool,
    config: HookConfig,
}

#[derive(Serialize)]
struct UpdateHookBody {
    events: Vec<&'static str>,
    active: bool,
    config: HookConfig,
}

#[derive(Deserialize)]
struct ExistingHook {
    id: u64,
    config: ExistingHookConfig,
}

#[derive(Deserialize)]
struct ExistingHookConfig {
    url: Option<String>,
}

pub fn hooks_base_url(scope: &RunnerScope, target: &str) -> String {
    match scope {
        RunnerScope::Org => format!("https://api.github.com/orgs/{target}/hooks"),
        RunnerScope::Repo => format!("https://api.github.com/repos/{target}/hooks"),
    }
}

fn label(scope: &RunnerScope, target: &str) -> String {
    match scope {
        RunnerScope::Org => format!("org/{target}"),
        RunnerScope::Repo => format!("repo/{target}"),
    }
}

pub async fn ensure_github_webhooks(
    client: &reqwest::Client,
    token: &str,
    fleet: &Fleet,
    secret: &str,
) -> Vec<String> {
    let Some(cfg) = fleet.webhook.as_ref() else {
        return Vec::new();
    };
    let hook_url = format!("https://{}/github", cfg.domain);

    let targets: HashSet<(RunnerScope, String)> = fleet
        .runners
        .values()
        .map(|r| (r.scope.clone(), r.target.clone()))
        .collect();

    if targets.is_empty() {
        return Vec::new();
    }

    println!();
    let mut issues = Vec::new();
    for (scope, target) in &targets {
        match ensure_one(client, token, scope, target, &hook_url, secret).await {
            Ok(action) => {
                ui::success(&format!("webhook {} → {action}", label(scope, target)));
            }
            Err(e) => {
                let msg = format!("webhook {} registration failed: {e}", label(scope, target));
                ui::error(&msg);
                issues.push(msg);
            }
        }
    }
    issues
}

async fn ensure_one(
    client: &reqwest::Client,
    token: &str,
    scope: &RunnerScope,
    target: &str,
    hook_url: &str,
    secret: &str,
) -> Result<&'static str> {
    let base = hooks_base_url(scope, target);

    let resp = client
        .get(&base)
        .query(&[("per_page", "100")])
        .bearer_auth(token)
        .header("User-Agent", "flow-iron")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !resp.status().is_success() {
        bail!("GitHub list hooks: {}", resp.status());
    }
    let existing: Vec<ExistingHook> = resp.json().await?;

    let config = HookConfig {
        url: hook_url.to_string(),
        content_type: "json",
        secret: secret.to_string(),
        insecure_ssl: "0",
    };

    if let Some(found) = existing
        .iter()
        .find(|h| h.config.url.as_deref() == Some(hook_url))
    {
        let body = UpdateHookBody {
            events: vec!["workflow_job"],
            active: true,
            config,
        };
        let resp = client
            .patch(format!("{base}/{}", found.id))
            .bearer_auth(token)
            .header("User-Agent", "flow-iron")
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("GitHub PATCH hook {}: {}", found.id, resp.status());
        }
        Ok("updated")
    } else {
        let body = CreateHookBody {
            name: "web",
            events: vec!["workflow_job"],
            active: true,
            config,
        };
        let resp = client
            .post(&base)
            .bearer_auth(token)
            .header("User-Agent", "flow-iron")
            .header("Accept", "application/vnd.github+json")
            .json(&body)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("GitHub POST hook: {}", resp.status());
        }
        Ok("created")
    }
}
