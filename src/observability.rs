use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const ORG: &str = "default";
const STREAM: &str = "flow_events";
const ALERT_DISCORD_NAME: &str = "flow-events-discord";
const ALERT_TELEGRAM_NAME: &str = "flow-events-telegram";

const DISCORD_TEMPLATE_BODY: &str = r#"{"embeds":"{rows}"}"#;
const DISCORD_ROW_TEMPLATE: &str = r#"{"author":{"name":"{app}"},"title":"{action}","description":"{msg}","color":{color},"footer":{"text":"{server}"}}"#;
const TELEGRAM_ROW_TEMPLATE: &str = "<b>{app}</b> · {action}\n<i>{server}</i>{msg}";

const GEOIP_FN_NAME: &str = "geoip_enrich";
const GEOIP_FN_BODY: &str = r#"record, _ = get_enrichment_table_record("maxmind_city", {"ip": to_string(.ip) ?? ""})
if is_object(record) {
  obj = object!(record)
  .country_code = obj.country_code
  .country_name = obj.country_name
}
."#;
const GEOIP_PIPELINE_NAME: &str = "rum_geoip";
const RUM_STREAM: &str = "_rumdata";

#[derive(Debug, Clone)]
pub struct SyncInput<'a> {
    pub hub_url: &'a str,
    pub user: &'a str,
    pub password: &'a str,
    pub discord_webhook_url: Option<&'a str>,
    pub telegram_bot_token: Option<&'a str>,
    pub telegram_chat_id: Option<&'a str>,
}

#[derive(Debug, Default)]
pub struct SyncOutput {
    pub rum_token: Option<String>,
}

pub async fn sync(input: &SyncInput<'_>) -> Result<SyncOutput> {
    let client = reqwest::Client::new();
    let base = input.hub_url.trim_end_matches('/');

    wait_until_ready(&client, base, input.user, input.password).await?;

    let discord_enabled = input
        .discord_webhook_url
        .filter(|s| !s.is_empty())
        .is_some();
    let telegram_enabled = input.telegram_bot_token.filter(|s| !s.is_empty()).is_some()
        && input.telegram_chat_id.filter(|s| !s.is_empty()).is_some();

    if discord_enabled {
        ensure_template(
            &client,
            base,
            input.user,
            input.password,
            "discord",
            DISCORD_TEMPLATE_BODY,
        )
        .await?;
        ensure_destination(
            &client,
            base,
            input.user,
            input.password,
            "discord",
            input.discord_webhook_url.unwrap_or_default(),
            "discord",
        )
        .await?;
        ensure_alert(
            &client,
            base,
            input.user,
            input.password,
            ALERT_DISCORD_NAME,
            "discord",
            DISCORD_ROW_TEMPLATE,
            "Json",
        )
        .await?;
    }

    if telegram_enabled {
        let chat_id = input.telegram_chat_id.unwrap_or_default();
        let token = input.telegram_bot_token.unwrap_or_default();
        let body = format!(r#"{{"chat_id":"{chat_id}","text":"{{rows}}","parse_mode":"HTML"}}"#);
        let url = format!("https://api.telegram.org/bot{token}/sendMessage");
        ensure_template(&client, base, input.user, input.password, "telegram", &body).await?;
        ensure_destination(
            &client,
            base,
            input.user,
            input.password,
            "telegram",
            &url,
            "telegram",
        )
        .await?;
        ensure_alert(
            &client,
            base,
            input.user,
            input.password,
            ALERT_TELEGRAM_NAME,
            "telegram",
            TELEGRAM_ROW_TEMPLATE,
            "String",
        )
        .await?;
    }

    ensure_function(
        &client,
        base,
        input.user,
        input.password,
        GEOIP_FN_NAME,
        GEOIP_FN_BODY,
    )
    .await?;
    ensure_geoip_pipeline(&client, base, input.user, input.password).await?;

    let rum_token = fetch_rum_token(&client, base, input.user, input.password).await?;

    Ok(SyncOutput { rum_token })
}

async fn ensure_function(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    name: &str,
    body: &str,
) -> Result<()> {
    let payload = json!({
        "name": name,
        "function": body,
        "params": "row",
        "transType": 0,
    });
    let exists = client
        .get(format!("{base}/api/{ORG}/functions/{name}"))
        .basic_auth(user, Some(password))
        .send()
        .await?
        .status()
        .is_success();

    let resp = if exists {
        client
            .put(format!("{base}/api/{ORG}/functions/{name}"))
            .basic_auth(user, Some(password))
            .json(&payload)
            .send()
            .await?
    } else {
        client
            .post(format!("{base}/api/{ORG}/functions"))
            .basic_auth(user, Some(password))
            .json(&payload)
            .send()
            .await?
    };
    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("function {name}: {s} {t}");
    }
    Ok(())
}

#[derive(Deserialize)]
struct PipelineSummary {
    #[serde(default, alias = "pipeline_id", alias = "id")]
    id: Option<String>,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct PipelineList {
    list: Vec<PipelineSummary>,
}

async fn ensure_geoip_pipeline(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
) -> Result<()> {
    let body = json!({
        "name": GEOIP_PIPELINE_NAME,
        "description": "Enrich _rumdata with country from ip",
        "source": {
            "source_type": "realtime",
            "stream_type": "logs",
            "stream_name": RUM_STREAM,
            "org_id": ORG,
        },
        "nodes": [
            {
                "id": "src",
                "type": "input",
                "data": {"node_type": "stream", "stream_type": "logs", "stream_name": RUM_STREAM, "org_id": ORG},
                "position": {"x": 0, "y": 0},
                "io_type": "input",
            },
            {
                "id": "fn1",
                "type": "default",
                "data": {"node_type": "function", "name": GEOIP_FN_NAME, "after_flatten": false, "num_args": 0},
                "position": {"x": 100, "y": 0},
                "io_type": "default",
            },
            {
                "id": "dst",
                "type": "output",
                "data": {"node_type": "stream", "stream_type": "logs", "stream_name": RUM_STREAM, "org_id": ORG},
                "position": {"x": 200, "y": 0},
                "io_type": "output",
            },
        ],
        "edges": [
            {"id": "e1", "source": "src", "target": "fn1"},
            {"id": "e2", "source": "fn1", "target": "dst"},
        ],
    });

    let list: PipelineList = client
        .get(format!("{base}/api/{ORG}/pipelines"))
        .basic_auth(user, Some(password))
        .send()
        .await?
        .json()
        .await
        .with_context(|| "list pipelines")?;

    let existing = list
        .list
        .into_iter()
        .find(|p| p.name == GEOIP_PIPELINE_NAME);

    let resp = if let Some(p) = existing {
        let mut updated = body.clone();
        if let Some(id) = p.id.as_deref() {
            updated["pipeline_id"] = json!(id);
        }
        client
            .put(format!("{base}/api/{ORG}/pipelines"))
            .basic_auth(user, Some(password))
            .json(&updated)
            .send()
            .await?
    } else {
        client
            .post(format!("{base}/api/{ORG}/pipelines"))
            .basic_auth(user, Some(password))
            .json(&body)
            .send()
            .await?
    };
    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("pipeline {GEOIP_PIPELINE_NAME}: {s} {t}");
    }
    Ok(())
}

async fn wait_until_ready(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
) -> Result<()> {
    for _ in 0..30 {
        let resp = client
            .get(format!("{base}/api/{ORG}/streams"))
            .basic_auth(user, Some(password))
            .send()
            .await;
        if let Ok(r) = resp {
            if r.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    bail!("OpenObserve hub at {base} not reachable after 60s");
}

async fn ensure_template(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    name: &str,
    body: &str,
) -> Result<()> {
    let payload = json!({"name": name, "type": "http", "title": "", "body": body});
    let exists = client
        .get(format!("{base}/api/{ORG}/alerts/templates/{name}"))
        .basic_auth(user, Some(password))
        .send()
        .await?
        .status()
        .is_success();

    let url = if exists {
        format!("{base}/api/{ORG}/alerts/templates/{name}")
    } else {
        format!("{base}/api/{ORG}/alerts/templates")
    };
    let req = if exists {
        client.put(url)
    } else {
        client.post(url)
    };
    let resp = req
        .basic_auth(user, Some(password))
        .json(&payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("template {name}: {s} {t}");
    }
    Ok(())
}

async fn ensure_destination(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    name: &str,
    url: &str,
    template: &str,
) -> Result<()> {
    let payload = json!({
        "name": name,
        "url": url,
        "method": "post",
        "type": "http",
        "template": template,
    });
    let exists = client
        .get(format!("{base}/api/{ORG}/alerts/destinations/{name}"))
        .basic_auth(user, Some(password))
        .send()
        .await?
        .status()
        .is_success();

    let endpoint = if exists {
        format!("{base}/api/{ORG}/alerts/destinations/{name}")
    } else {
        format!("{base}/api/{ORG}/alerts/destinations?module=alert")
    };
    let req = if exists {
        client.put(endpoint)
    } else {
        client.post(endpoint)
    };
    let resp = req
        .basic_auth(user, Some(password))
        .json(&payload)
        .send()
        .await?;
    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("destination {name}: {s} {t}");
    }
    Ok(())
}

#[derive(Deserialize)]
struct AlertSummary {
    name: String,
    #[serde(default, alias = "alert_id")]
    id: Option<String>,
}

#[derive(Deserialize)]
struct AlertList {
    list: Vec<AlertSummary>,
}

#[allow(clippy::too_many_arguments)]
async fn ensure_alert(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    name: &str,
    destination: &str,
    row_template: &str,
    row_template_type: &str,
) -> Result<()> {
    let payload = json!({
        "name": name,
        "stream_type": "logs",
        "stream_name": STREAM,
        "is_real_time": true,
        "enabled": true,
        "destinations": [destination],
        "row_template": row_template,
        "row_template_type": row_template_type,
        "query_condition": {
            "type": "custom",
            "conditions": [{"column": "level", "operator": "!=", "value": ""}],
        },
        "trigger_condition": {
            "period": 1,
            "operator": ">=",
            "threshold": 1,
            "frequency": 1,
            "frequency_type": "minutes",
            "silence": 0,
        },
    });

    let list: AlertList = client
        .get(format!("{base}/api/v2/{ORG}/alerts"))
        .basic_auth(user, Some(password))
        .send()
        .await?
        .json()
        .await
        .with_context(|| "list alerts")?;
    let existing = list.list.into_iter().find(|a| a.name == name);

    let resp = if let Some(a) = existing {
        let id = a.id.unwrap_or_default();
        client
            .put(format!("{base}/api/v2/{ORG}/alerts/{id}"))
            .basic_auth(user, Some(password))
            .json(&payload)
            .send()
            .await?
    } else {
        client
            .post(format!("{base}/api/v2/{ORG}/alerts"))
            .basic_auth(user, Some(password))
            .json(&payload)
            .send()
            .await?
    };
    if !resp.status().is_success() {
        let s = resp.status();
        let t = resp.text().await.unwrap_or_default();
        bail!("alert {name}: {s} {t}");
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct RumTokenResponse {
    data: RumTokenData,
}

#[derive(Serialize, Deserialize)]
struct RumTokenData {
    rum_token: String,
}

async fn fetch_rum_token(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
) -> Result<Option<String>> {
    let resp = client
        .get(format!("{base}/api/{ORG}/rumtoken"))
        .basic_auth(user, Some(password))
        .send()
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: Value = resp.json().await?;
    Ok(body
        .get("data")
        .and_then(|d| d.get("rum_token"))
        .and_then(|t| t.as_str())
        .map(ToString::to_string))
}
