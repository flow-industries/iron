use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde::Deserialize;

use crate::config::ResolvedApp;

pub struct PackageRelease {
    pub tag: String,
    pub published: String,
}

pub fn parse_ghcr_image(image: &str) -> Option<(&str, &str)> {
    let rest = image.strip_prefix("ghcr.io/")?;
    let (owner, package_with_tag) = rest.split_once('/')?;
    let package = package_with_tag.split(':').next()?;
    if owner.is_empty() || package.is_empty() {
        return None;
    }
    Some((owner, package))
}

pub fn select_tag(tags: &[String]) -> String {
    let dominated = ["latest", "main", "master"];
    let is_sha = |t: &str| t.len() >= 32 && t.chars().all(|c| c.is_ascii_hexdigit());

    if let Some(tag) = tags
        .iter()
        .find(|t| !dominated.contains(&t.as_str()) && !is_sha(t))
    {
        return tag.clone();
    }

    if let Some(sha) = tags.iter().find(|t| is_sha(t)) {
        return sha[..7].to_string();
    }

    "latest".to_string()
}

pub fn format_relative_time(timestamp: &str) -> String {
    let Some(secs) = parse_iso8601_to_epoch(timestamp) else {
        return String::from("—");
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());

    if now < secs {
        return String::from("just now");
    }

    let delta = Duration::from_secs(now - secs);
    let seconds = delta.as_secs();

    if seconds < 60 {
        return String::from("just now");
    }

    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }

    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h ago");
    }

    let days = hours / 24;
    if days < 30 {
        return format!("{days}d ago");
    }

    let months = days / 30;
    if months < 12 {
        return format!("{months}mo ago");
    }

    let years = months / 12;
    format!("{years}y ago")
}

fn parse_iso8601_to_epoch(s: &str) -> Option<u64> {
    let s = s.trim_end_matches('Z');
    let (date_part, time_part) = s.split_once('T')?;
    let date_parts: Vec<&str> = date_part.split('-').collect();
    if date_parts.len() != 3 {
        return None;
    }
    let year: u32 = date_parts[0].parse().ok()?;
    let month: u32 = date_parts[1].parse().ok()?;
    let day: u32 = date_parts[2].parse().ok()?;

    if year < 1970 || month == 0 || month > 12 || day == 0 || day > 31 {
        return None;
    }

    let time_str = time_part.split('+').next()?.split('-').next()?;
    let time_parts: Vec<&str> = time_str.split(':').collect();
    if time_parts.len() < 2 {
        return None;
    }
    let hour: u64 = time_parts[0].parse().ok()?;
    let minute: u64 = time_parts[1].parse().ok()?;
    let second: u64 = if time_parts.len() > 2 {
        time_parts[2].split('.').next()?.parse().ok()?
    } else {
        0
    };

    let mut total_days: u64 = 0;
    for y in 1970..year {
        total_days += if is_leap_year(y) { 366 } else { 365 };
    }

    let month_days: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        total_days += month_days[(m - 1) as usize];
        if m == 2 && is_leap_year(year) {
            total_days += 1;
        }
    }
    total_days += u64::from(day - 1);

    Some(total_days * 86400 + hour * 3600 + minute * 60 + second)
}

fn is_leap_year(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

#[derive(Deserialize)]
struct GhcrVersion {
    metadata: GhcrMetadata,
    updated_at: String,
}

#[derive(Deserialize)]
struct GhcrMetadata {
    container: GhcrContainer,
}

#[derive(Deserialize)]
struct GhcrContainer {
    tags: Vec<String>,
}

async fn fetch_one(
    client: &reqwest::Client,
    token: &str,
    owner: &str,
    package: &str,
) -> Option<PackageRelease> {
    let url = format!(
        "https://api.github.com/orgs/{owner}/packages/container/{package}/versions?per_page=1"
    );

    let resp = client
        .get(&url)
        .bearer_auth(token)
        .header("User-Agent", "flow-iron")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let versions: Vec<GhcrVersion> = resp.json().await.ok()?;
    let version = versions.into_iter().next()?;

    let tag = select_tag(&version.metadata.container.tags);
    let published = format_relative_time(&version.updated_at);

    Some(PackageRelease { tag, published })
}

pub async fn fetch_latest_release(
    token: Option<&str>,
    owner: &str,
    package: &str,
) -> Option<PackageRelease> {
    let client = reqwest::Client::new();
    fetch_one(&client, token?, owner, package).await
}

#[allow(clippy::implicit_hasher)]
pub async fn fetch_releases(
    token: Option<&str>,
    apps: &HashMap<String, ResolvedApp>,
) -> HashMap<String, PackageRelease> {
    let Some(token) = token else {
        return HashMap::new();
    };

    let client = reqwest::Client::new();
    let mut futs = Vec::new();

    for (name, app) in apps {
        if let Some((owner, package)) = parse_ghcr_image(&app.image) {
            let client = &client;
            let name = name.clone();
            let owner = owner.to_string();
            let package = package.to_string();
            futs.push(async move {
                let release = fetch_one(client, token, &owner, &package).await;
                (name, release)
            });
        }
    }

    let results = futures::future::join_all(futs).await;

    results
        .into_iter()
        .filter_map(|(name, release)| release.map(|r| (name, r)))
        .collect()
}
