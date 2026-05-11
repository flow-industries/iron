use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use console::style;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::Fleet;

const ORG: &str = "default";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct TailOpts {
    pub apps: Vec<String>,
    pub servers: Vec<String>,
    pub level: Option<String>,
    pub streams: Vec<String>,
    pub since: String,
    pub limit: u32,
    pub follow: bool,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    hits: Vec<HashMap<String, Value>>,
}

pub async fn run(fleet: &Fleet, opts: TailOpts) -> Result<()> {
    let app = fleet
        .apps
        .get("observe")
        .context("observe app not in fleet.toml")?;
    let domain = app
        .routing
        .as_ref()
        .and_then(|r| r.domains.first())
        .context("observe app has no routing.domains")?;
    let user = app
        .env
        .get("ZO_ROOT_USER_EMAIL")
        .context("ZO_ROOT_USER_EMAIL missing in [apps.observe] env")?;
    let password = app
        .env
        .get("ZO_ROOT_USER_PASSWORD")
        .context("ZO_ROOT_USER_PASSWORD missing in [apps.observe] env")?;

    if opts.streams.is_empty() {
        bail!("no streams specified");
    }

    let base = format!("https://{domain}");
    let client = reqwest::Client::new();
    let lookback_us = parse_duration_us(&opts.since)?;

    if !opts.follow {
        let end_us = now_us();
        let start_us = end_us.saturating_sub(lookback_us);
        let merged = fan_out_search(
            &client,
            &base,
            user,
            password,
            &opts.streams,
            &opts,
            start_us,
            end_us,
            opts.limit,
        )
        .await?;
        for (_, h) in &merged {
            print_hit(h);
        }
        return Ok(());
    }

    let initial_cursor = now_us().saturating_sub(lookback_us);
    let mut cursors: Vec<u128> = vec![initial_cursor; opts.streams.len()];
    loop {
        let end_us = now_us();
        let mut tasks = Vec::with_capacity(opts.streams.len());
        for (i, stream) in opts.streams.iter().enumerate() {
            let cursor = cursors[i];
            if cursor >= end_us {
                tasks.push(None);
                continue;
            }
            let where_clause = build_where(&opts, stream);
            tasks.push(Some(search_with_where(
                &client,
                &base,
                user,
                password,
                stream,
                where_clause,
                cursor,
                end_us,
                1000,
            )));
        }

        let mut merged: Vec<(usize, HashMap<String, Value>)> = Vec::new();
        let active: Vec<_> = tasks
            .into_iter()
            .enumerate()
            .filter_map(|(i, t)| t.map(|t| (i, t)))
            .collect();
        let (idxs, futs): (Vec<_>, Vec<_>) = active.into_iter().unzip();
        let results = futures::future::join_all(futs).await;
        for (i, result) in idxs.into_iter().zip(results.into_iter()) {
            for h in result? {
                merged.push((i, h));
            }
        }
        merged.sort_by_key(|(_, h)| h.get("_timestamp").and_then(Value::as_i64).unwrap_or(0));

        for (stream_idx, h) in &merged {
            print_hit(h);
            if let Some(ts) = h.get("_timestamp").and_then(Value::as_i64) {
                if let Ok(ts_u) = u128::try_from(ts) {
                    if ts_u >= cursors[*stream_idx] {
                        cursors[*stream_idx] = ts_u + 1;
                    }
                }
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn fan_out_search(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    streams: &[String],
    opts: &TailOpts,
    start_us: u128,
    end_us: u128,
    limit: u32,
) -> Result<Vec<(usize, HashMap<String, Value>)>> {
    let futs = streams.iter().map(|stream| {
        let where_clause = build_where(opts, stream);
        search_with_where(
            client,
            base,
            user,
            password,
            stream,
            where_clause,
            start_us,
            end_us,
            limit,
        )
    });
    let results = futures::future::join_all(futs).await;
    let mut merged: Vec<(usize, HashMap<String, Value>)> = Vec::new();
    for (i, result) in results.into_iter().enumerate() {
        for h in result? {
            merged.push((i, h));
        }
    }
    merged.sort_by_key(|(_, h)| h.get("_timestamp").and_then(Value::as_i64).unwrap_or(0));
    Ok(merged)
}

fn build_where(opts: &TailOpts, stream: &str) -> String {
    let mut conds = Vec::new();
    if !opts.apps.is_empty() {
        let parts: Vec<String> = opts
            .apps
            .iter()
            .map(|a| match stream {
                "flow_events" => format!("app = '{}'", escape_sql(a)),
                _ => format!("container_name LIKE '{}%'", escape_sql(a)),
            })
            .collect();
        conds.push(format!("({})", parts.join(" OR ")));
    }
    if !opts.servers.is_empty() {
        let parts: Vec<String> = opts
            .servers
            .iter()
            .map(|s| format!("server = '{}'", escape_sql(s)))
            .collect();
        conds.push(format!("({})", parts.join(" OR ")));
    }
    if let Some(level) = &opts.level {
        conds.push(format!("level = '{}'", escape_sql(level)));
    }
    if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

#[allow(clippy::too_many_arguments)]
async fn search_with_where(
    client: &reqwest::Client,
    base: &str,
    user: &str,
    password: &str,
    stream: &str,
    where_clause: String,
    start_us: u128,
    end_us: u128,
    limit: u32,
) -> Result<Vec<HashMap<String, Value>>> {
    let sql = format!("SELECT * FROM \"{stream}\"{where_clause} ORDER BY _timestamp DESC");
    let payload = json!({
        "query": {
            "sql": sql,
            "start_time": i64::try_from(start_us).unwrap_or(0),
            "end_time": i64::try_from(end_us).unwrap_or(i64::MAX),
            "from": 0,
            "size": limit,
        }
    });
    let resp = client
        .post(format!("{base}/api/{ORG}/_search?type=logs"))
        .basic_auth(user, Some(password))
        .json(&payload)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        bail!("search failed: {status} {t}");
    }
    let body: SearchResponse = resp.json().await?;
    Ok(body.hits)
}

fn now_us() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

fn parse_duration_us(s: &str) -> Result<u128> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration");
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let n: u64 = num_part
        .parse()
        .with_context(|| format!("invalid duration '{s}': expected like 5m, 1h, 24h"))?;
    let secs = match unit {
        "s" => n,
        "m" => n.saturating_mul(60),
        "h" => n.saturating_mul(3600),
        "d" => n.saturating_mul(86400),
        _ => bail!("unknown duration unit '{unit}': use s, m, h, or d"),
    };
    Ok(u128::from(secs).saturating_mul(1_000_000))
}

fn format_hms(ts_us: i64) -> String {
    let Ok(ts) = u64::try_from(ts_us) else {
        return "??:??:??".to_string();
    };
    let secs = ts / 1_000_000;
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn print_hit(h: &HashMap<String, Value>) {
    let ts_us = h.get("_timestamp").and_then(Value::as_i64).unwrap_or(0);
    let ts_str = format_hms(ts_us);

    let level = h
        .get("level")
        .and_then(Value::as_str)
        .unwrap_or("info")
        .to_lowercase();
    let level_label = format!("{:5}", level.to_uppercase());
    let level_styled = match level.as_str() {
        "error" | "err" | "fatal" => style(level_label).red().bold().to_string(),
        "warn" | "warning" => style(level_label).yellow().to_string(),
        "success" => style(level_label).green().to_string(),
        "info" => style(level_label).cyan().to_string(),
        "debug" | "trace" => style(level_label).dim().to_string(),
        _ => level_label,
    };

    let server = h.get("server").and_then(Value::as_str).unwrap_or("?");
    let app = h
        .get("app")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            h.get("container_name")
                .and_then(Value::as_str)
                .map(|c| c.split('-').next().unwrap_or(c).to_string())
        })
        .unwrap_or_else(|| "?".to_string());

    let msg = h
        .get("msg")
        .or_else(|| h.get("message"))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        })
        .unwrap_or_default();

    let is_access_log = h
        .get("logger")
        .and_then(Value::as_str)
        .is_some_and(|s| s.starts_with("http.log.access"));

    let body = if is_access_log {
        let method = h
            .get("request_method")
            .and_then(Value::as_str)
            .unwrap_or("?");
        let uri = h.get("request_uri").and_then(Value::as_str).unwrap_or("");
        let status = h.get("status").and_then(Value::as_i64).unwrap_or(0);
        let dur_ms = h.get("duration").and_then(Value::as_f64).unwrap_or(0.0) * 1000.0;
        format!("{method} {uri} {status} {dur_ms:.1}ms")
    } else {
        let action = h.get("action").and_then(Value::as_str);
        match (action, msg.as_str()) {
            (Some(a), "") => style(a).bold().to_string(),
            (Some(a), m) => format!("{} {m}", style(a).bold()),
            (None, m) => m.to_string(),
        }
    };

    println!(
        "{} {} {} {}",
        style(ts_str).dim(),
        level_styled,
        style(format!("{server}/{app}")).magenta(),
        body
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_where_no_filters_is_empty() {
        let opts = TailOpts {
            apps: vec![],
            servers: vec![],
            level: None,
            streams: vec!["app_logs".into()],
            since: "5m".into(),
            limit: 100,
            follow: false,
        };
        assert_eq!(build_where(&opts, "app_logs"), "");
    }

    #[test]
    fn build_where_combines_filters() {
        let opts = TailOpts {
            apps: vec!["paper".into(), "auth".into()],
            servers: vec!["fl-1".into()],
            level: Some("error".into()),
            streams: vec!["app_logs".into()],
            since: "5m".into(),
            limit: 100,
            follow: false,
        };
        assert_eq!(
            build_where(&opts, "app_logs"),
            " WHERE (container_name LIKE 'paper%' OR container_name LIKE 'auth%') AND (server = 'fl-1') AND level = 'error'"
        );
        assert_eq!(
            build_where(&opts, "flow_events"),
            " WHERE (app = 'paper' OR app = 'auth') AND (server = 'fl-1') AND level = 'error'"
        );
    }

    #[test]
    fn build_where_escapes_quotes() {
        let opts = TailOpts {
            apps: vec!["a'b".into()],
            servers: vec![],
            level: None,
            streams: vec!["app_logs".into()],
            since: "5m".into(),
            limit: 100,
            follow: false,
        };
        assert_eq!(
            build_where(&opts, "app_logs"),
            " WHERE (container_name LIKE 'a''b%')"
        );
    }

    #[test]
    fn parse_duration_basic() {
        assert_eq!(parse_duration_us("30s").unwrap(), 30_000_000);
        assert_eq!(parse_duration_us("5m").unwrap(), 300_000_000);
        assert_eq!(parse_duration_us("2h").unwrap(), 7_200_000_000);
        assert_eq!(parse_duration_us("1d").unwrap(), 86_400_000_000);
    }

    #[test]
    fn parse_duration_rejects_bad_input() {
        assert!(parse_duration_us("").is_err());
        assert!(parse_duration_us("5x").is_err());
        assert!(parse_duration_us("xm").is_err());
    }

    #[test]
    fn format_hms_handles_negative() {
        assert_eq!(format_hms(-1), "??:??:??");
    }

    #[test]
    fn format_hms_formats_known_value() {
        let one_oclock_us = 3600 * 1_000_000;
        assert_eq!(format_hms(one_oclock_us), "01:00:00");
    }
}
