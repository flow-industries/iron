use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CF_API: &str = "https://api.cloudflare.com/client/v4";

#[derive(Deserialize)]
struct CfResponse<T> {
    success: bool,
    result: T,
    #[serde(default)]
    errors: Vec<CfError>,
}

#[derive(Deserialize)]
struct CfError {
    message: String,
}

#[derive(Deserialize)]
struct Zone {
    id: String,
}

#[derive(Deserialize)]
pub struct DnsRecord {
    pub id: String,
    pub content: String,
}

#[derive(Serialize)]
struct CreateRecord {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    ttl: u32,
    proxied: bool,
}

#[derive(Serialize)]
struct UpdateRecord {
    content: String,
}

pub async fn ensure_dns_record(api_token: &str, hostname: &str, ip: &str) -> Result<()> {
    let client = reqwest::Client::new();

    let zone_name = extract_zone(hostname);

    let zone_id = get_zone_id(&client, api_token, &zone_name)
        .await
        .with_context(|| format!("Failed to find Cloudflare zone for {zone_name}"))?;

    let existing = get_record(&client, api_token, &zone_id, hostname).await?;

    match existing {
        Some(record) if record.content == ip => {}
        Some(record) => {
            let url = format!("{}/zones/{}/dns_records/{}", CF_API, zone_id, record.id);
            let resp: CfResponse<serde_json::Value> = client
                .patch(&url)
                .bearer_auth(api_token)
                .json(&UpdateRecord {
                    content: ip.to_string(),
                })
                .send()
                .await?
                .json()
                .await?;
            if !resp.success {
                let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Failed to update DNS record: {}", msgs.join(", "));
            }
        }
        None => {
            let url = format!("{CF_API}/zones/{zone_id}/dns_records");
            let resp: CfResponse<serde_json::Value> = client
                .post(&url)
                .bearer_auth(api_token)
                .json(&CreateRecord {
                    record_type: "A".to_string(),
                    name: hostname.to_string(),
                    content: ip.to_string(),
                    // Cloudflare auto TTL
                    ttl: 1,
                    proxied: true,
                })
                .send()
                .await?
                .json()
                .await?;
            if !resp.success {
                let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
                anyhow::bail!("Failed to create DNS record: {}", msgs.join(", "));
            }
        }
    }

    Ok(())
}

pub fn extract_zone(hostname: &str) -> String {
    let parts: Vec<&str> = hostname.split('.').collect();
    if parts.len() >= 2 {
        parts[parts.len() - 2..].join(".")
    } else {
        hostname.to_string()
    }
}

pub async fn get_zone_id(
    client: &reqwest::Client,
    api_token: &str,
    zone_name: &str,
) -> Result<String> {
    let url = format!("{CF_API}/zones?name={zone_name}");
    let resp: CfResponse<Vec<Zone>> = client
        .get(&url)
        .bearer_auth(api_token)
        .send()
        .await?
        .json()
        .await?;
    if !resp.success || resp.result.is_empty() {
        anyhow::bail!("Zone '{zone_name}' not found in Cloudflare");
    }
    Ok(resp.result[0].id.clone())
}

pub async fn get_record(
    client: &reqwest::Client,
    api_token: &str,
    zone_id: &str,
    hostname: &str,
) -> Result<Option<DnsRecord>> {
    let url = format!("{CF_API}/zones/{zone_id}/dns_records?type=A&name={hostname}");
    let resp: CfResponse<Vec<DnsRecord>> = client
        .get(&url)
        .bearer_auth(api_token)
        .send()
        .await?
        .json()
        .await?;
    Ok(resp.result.into_iter().next())
}
