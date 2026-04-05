# Notification System Design

## Overview

Notifications are triggered by three event types:

| Event | Source | Current behavior |
|---|---|---|
| **Deploy** | `flow deploy` CLI | Prints to terminal only |
| **Image update** | WUD detects new digest on GHCR | `rollout.sh` pulls + rolls out silently |
| **Health degradation** | `flow check` CLI | Prints to terminal only |

Goal: send structured messages to Telegram and/or Discord when any of these events occur.

---

## Configuration

### fleet.toml

```toml
[notifications]
events = ["deploy", "image_update", "health"]  # which events to send

[[notifications.channels]]
type = "telegram"

[[notifications.channels]]
type = "discord"
```

No secrets in `fleet.toml`. Channel types simply declare intent — tokens live in `fleet.env.toml`.

### fleet.env.toml

```toml
[notifications.telegram]
bot_token = "123456:ABC-DEF..."
chat_id = "-1001234567890"

[notifications.discord]
webhook_url = "https://discord.com/api/webhooks/12345/abcdef..."
```

Discord uses a webhook URL (no bot needed, simplest setup). Telegram uses a bot token + chat ID.

---

## Architecture

There are two independent notification paths because WUD and the CLI run in different contexts:

```
┌─────────────────────────────────────┐
│         flow CLI (Rust)             │
│                                     │
│  deploy.rs ──┐                      │
│  check.rs  ──┼──▶ notify::send()   │──▶ Telegram / Discord APIs
│              │                      │
└──────────────┼──────────────────────┘
               │
┌──────────────┼──────────────────────┐
│         WUD (on server)             │
│                                     │
│  image detected ──▶ rollout.sh     │
│                    ├─ docker pull    │
│                    ├─ docker rollout │
│                    └─ curl notify   │──▶ Telegram / Discord APIs
└─────────────────────────────────────┘
```

### Path 1: CLI notifications (deploy + health)

A new `src/notify.rs` module with a simple trait:

```rust
pub struct Event {
    pub kind: EventKind,
    pub app: String,
    pub server: String,
    pub detail: String,       // e.g. image tag, health status
    pub timestamp: DateTime<Utc>,
}

pub enum EventKind {
    Deploy,
    ImageUpdate,
    Health,
}

pub trait Channel {
    async fn send(&self, event: &Event) -> Result<()>;
}
```

Two implementations: `TelegramChannel` and `DiscordChannel`. Both use `reqwest` to POST a formatted message.

**Integration points in existing code:**

- `deploy.rs:171` — after `ui::success` per server, fire `EventKind::Deploy`
- `deploy.rs:78` — summary event after full deploy completes
- `check.rs` — fire `EventKind::Health` for containers that are unhealthy/missing

The notify call is fire-and-forget with a timeout (5s). A notification failure never blocks a deploy.

### Path 2: WUD notifications (image updates)

WUD has **built-in** Telegram and Discord trigger support. We add env vars to the WUD compose generation in `server.rs:227`:

```yaml
# Telegram (if configured)
WUD_TRIGGER_TELEGRAM_NOTIFY_BOTTOKEN: ${telegram_bot_token}
WUD_TRIGGER_TELEGRAM_NOTIFY_CHATID: ${telegram_chat_id}

# Discord (if configured)
WUD_TRIGGER_DISCORD_NOTIFY_URL: ${discord_webhook_url}
```

WUD will automatically notify on both channels when it detects and rolls out a new image. This covers image_update events without any custom code on the server side.

**Alternative: rollout.sh curl** — If WUD's built-in messages aren't rich enough, we enhance `rollout.sh` to `curl` the APIs directly after a successful rollout. This gives us full control over message formatting but requires the tokens to be available on the server (mounted as env vars into the WUD container).

Recommendation: start with WUD's built-in triggers. Switch to rollout.sh curl only if message formatting is insufficient.

---

## Message Formats

### Telegram (Markdown)

```
✅ *Deploy* — site
Server: flow-1
Image: ghcr.io/flow-industries/site:latest
```

```
🔄 *Image Update* — site
Server: flow-1
Old: sha256:abc123...
New: sha256:def456...
```

```
🔴 *Unhealthy* — site
Server: flow-1
Status: container missing
```

Telegram API: `POST https://api.telegram.org/bot{token}/sendMessage` with `parse_mode=Markdown`.

### Discord (Embed)

```json
{
  "embeds": [{
    "title": "Deploy — site",
    "color": 3066993,
    "fields": [
      { "name": "Server", "value": "flow-1", "inline": true },
      { "name": "Image", "value": "ghcr.io/...:latest", "inline": true }
    ],
    "timestamp": "2026-04-05T12:00:00Z"
  }]
}
```

Discord API: `POST {webhook_url}` with JSON body. Color codes: green (0x2ECC71) for deploy, blue (0x3498DB) for image update, red (0xE74C3C) for health.

---

## Implementation Plan

### Phase 1: Config + notify module

1. Add `Notifications` struct to `config.rs` — parse `[notifications]` from fleet.toml
2. Add notification secrets to `EnvConfig` — parse from fleet.env.toml
3. Create `src/notify.rs` — `Event`, `Channel` trait, `TelegramChannel`, `DiscordChannel`
4. Wire into `deploy.rs` — send after successful/failed deploy per server

### Phase 2: WUD integration

5. Update `generate_wud_compose()` in `server.rs` — inject WUD Telegram/Discord env vars when configured
6. This handles image_update events with zero custom server-side code

### Phase 3: Health notifications

7. Add `--notify` flag to `flow check` — sends notifications for unhealthy containers
8. Optionally: `flow watch` command that runs `check` on an interval and notifies on state changes (healthy → unhealthy transitions only, to avoid spam)

### Phase 4: Optional enhancements

- **Deduplication**: track last-notified state to avoid repeated health alerts
- **Mute/quiet hours**: `[notifications] quiet_hours = "02:00-08:00"`
- **Per-app overrides**: `[apps.site.notifications] events = ["deploy"]` to opt specific apps in/out
- **Slack connector**: same webhook pattern as Discord, easy to add later

---

## Connector Comparison

| | Telegram | Discord |
|---|---|---|
| **Setup** | Create bot via @BotFather, get chat ID | Create webhook in channel settings |
| **Auth** | Bot token + chat ID | Single webhook URL |
| **Rate limits** | 30 msg/sec to same chat | 30 req/60s per webhook |
| **Message format** | Markdown or HTML | Rich embeds with colors, fields |
| **Complexity** | Low — one HTTP POST | Low — one HTTP POST |
| **Best for** | Personal/small team alerts | Team channels with rich formatting |

Both are trivial to implement. Discord embeds look nicer out of the box. Telegram is simpler to set up (no server/channel needed, just a bot + chat).

---

## Dependencies

- `reqwest` — already in Cargo.toml for Cloudflare API calls
- `chrono` — for timestamps (already used or add as lightweight dep)
- No new external services or infrastructure needed
