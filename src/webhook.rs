pub const IMAGE: &str = "python:3.13-alpine";

pub fn generate_compose(
    network: &str,
    secret: &str,
    discord_webhook_url: Option<&str>,
    telegram_bot_token: Option<&str>,
    telegram_chat_id: Option<&str>,
) -> String {
    let mut extra_env = String::new();
    if let Some(url) = discord_webhook_url {
        extra_env.push_str(&format!("      DISCORD_WEBHOOK_URL: {url}\n"));
    }
    if let Some(token) = telegram_bot_token {
        extra_env.push_str(&format!("      TELEGRAM_BOT_TOKEN: {token}\n"));
    }
    if let Some(chat_id) = telegram_chat_id {
        extra_env.push_str(&format!("      TELEGRAM_CHAT_ID: {chat_id}\n"));
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
{extra_env}    volumes:
      - ./webhook.py:/webhook.py:ro
    networks:
      - {network}
    restart: always
    healthcheck:
      test: ["CMD-SHELL", "wget -qO- http://localhost:8080/health >/dev/null 2>&1 || exit 1"]
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
