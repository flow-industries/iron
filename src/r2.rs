use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const CF_API: &str = "https://api.cloudflare.com/client/v4";

#[derive(Deserialize)]
struct CfResponse<T> {
    success: bool,
    // serde derive on a generic struct propagates `T: Default` from bare `#[serde(default)]`
    // even though `Option<T>::default()` doesn't need it; the helper sidesteps that bound.
    #[serde(default = "default_result")]
    result: Option<T>,
    #[serde(default)]
    errors: Vec<CfError>,
}

fn default_result<T>() -> Option<T> {
    None
}

#[derive(Deserialize)]
struct CfError {
    #[serde(default)]
    code: u32,
    message: String,
}

#[derive(Debug, Clone)]
pub struct R2Credentials {
    pub token_id: String,
    pub access_key_id: String,
    pub secret_access_key: String,
}

#[derive(Serialize)]
struct CreateBucket<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct CreateCustomDomain<'a> {
    domain: &'a str,
    enabled: bool,
    #[serde(rename = "minTLS")]
    min_tls: &'a str,
}

#[derive(Serialize)]
struct CreateApiToken<'a> {
    name: &'a str,
    policies: Vec<TokenPolicy<'a>>,
}

#[derive(Serialize)]
struct TokenPolicy<'a> {
    permission: &'a str,
    #[serde(rename = "buckets")]
    buckets: Vec<&'a str>,
}

#[derive(Deserialize)]
struct ApiTokenResult {
    id: String,
    #[serde(rename = "accessKeyId")]
    access_key_id: String,
    #[serde(rename = "secretAccessKey")]
    secret_access_key: String,
}

pub async fn verify_r2_access(api_token: &str, account_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{CF_API}/accounts/{account_id}/r2/buckets?per_page=1");
    let resp: CfResponse<serde_json::Value> = client
        .get(&url)
        .bearer_auth(api_token)
        .send()
        .await?
        .json()
        .await?;
    if !resp.success {
        let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!(
            "Cloudflare API token lacks R2 access for account {account_id}: {}",
            msgs.join(", ")
        );
    }
    Ok(())
}

pub async fn bucket_exists(
    client: &reqwest::Client,
    api_token: &str,
    account_id: &str,
    name: &str,
) -> Result<bool> {
    let url = format!("{CF_API}/accounts/{account_id}/r2/buckets/{name}");
    let resp = client.get(&url).bearer_auth(api_token).send().await?;
    let status = resp.status();
    if status.is_success() {
        return Ok(true);
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(false);
    }
    let body: CfResponse<serde_json::Value> = resp.json().await?;
    let already_missing = body.errors.iter().any(|e| e.code == 10006);
    if already_missing {
        return Ok(false);
    }
    let msgs: Vec<_> = body.errors.iter().map(|e| e.message.as_str()).collect();
    anyhow::bail!(
        "Failed to query R2 bucket '{name}' (status {status}): {}",
        msgs.join(", ")
    );
}

pub async fn ensure_bucket(api_token: &str, account_id: &str, name: &str) -> Result<()> {
    let client = reqwest::Client::new();

    if bucket_exists(&client, api_token, account_id, name).await? {
        return Ok(());
    }

    let create_url = format!("{CF_API}/accounts/{account_id}/r2/buckets");
    let resp: CfResponse<serde_json::Value> = client
        .post(&create_url)
        .bearer_auth(api_token)
        .json(&CreateBucket { name })
        .send()
        .await?
        .json()
        .await?;
    if !resp.success {
        let already_exists = resp.errors.iter().any(|e| e.code == 10004);
        if already_exists {
            return Ok(());
        }
        let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!("Failed to create R2 bucket '{name}': {}", msgs.join(", "));
    }
    Ok(())
}

pub async fn attach_custom_domain(
    api_token: &str,
    account_id: &str,
    bucket: &str,
    domain: &str,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{CF_API}/accounts/{account_id}/r2/buckets/{bucket}/custom_domains");
    let resp: CfResponse<serde_json::Value> = client
        .post(&url)
        .bearer_auth(api_token)
        .json(&CreateCustomDomain {
            domain,
            enabled: true,
            min_tls: "1.2",
        })
        .send()
        .await?
        .json()
        .await?;
    if !resp.success {
        let already_attached = resp
            .errors
            .iter()
            .any(|e| e.code == 10071 || e.message.to_ascii_lowercase().contains("already"));
        if already_attached {
            return Ok(());
        }
        let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!(
            "Failed to attach custom domain '{domain}' to R2 bucket '{bucket}': {}",
            msgs.join(", ")
        );
    }
    Ok(())
}

pub async fn mint_app_token(
    api_token: &str,
    account_id: &str,
    app_name: &str,
    bucket_names: &[&str],
) -> Result<R2Credentials> {
    let client = reqwest::Client::new();
    let url = format!("{CF_API}/accounts/{account_id}/r2/api_tokens");
    let token_name = format!("iron-{app_name}");
    let body = CreateApiToken {
        name: &token_name,
        policies: vec![TokenPolicy {
            permission: "admin-read-write",
            buckets: bucket_names.to_vec(),
        }],
    };
    let resp: CfResponse<ApiTokenResult> = client
        .post(&url)
        .bearer_auth(api_token)
        .json(&body)
        .send()
        .await?
        .json()
        .await?;
    if !resp.success {
        let msgs: Vec<_> = resp.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!(
            "Failed to mint R2 API token for app '{app_name}': {}",
            msgs.join(", ")
        );
    }
    let result = resp
        .result
        .context("Cloudflare returned success without a result body for R2 token creation")?;
    Ok(R2Credentials {
        token_id: result.id,
        access_key_id: result.access_key_id,
        secret_access_key: result.secret_access_key,
    })
}

pub async fn revoke_token(api_token: &str, account_id: &str, token_id: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!("{CF_API}/accounts/{account_id}/r2/api_tokens/{token_id}");
    let resp = client.delete(&url).bearer_auth(api_token).send().await?;
    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(());
    }
    let body: CfResponse<serde_json::Value> = resp.json().await?;
    if !body.success {
        let msgs: Vec<_> = body.errors.iter().map(|e| e.message.as_str()).collect();
        anyhow::bail!(
            "Failed to revoke R2 token '{token_id}': {}",
            msgs.join(", ")
        );
    }
    Ok(())
}

pub fn s3_endpoint(account_id: &str) -> String {
    format!("https://{account_id}.r2.cloudflarestorage.com")
}

pub fn custom_domain_cname_target(bucket: &str, account_id: &str) -> String {
    format!("{bucket}.{account_id}.r2.cloudflarestorage.com")
}
