use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::net::Ipv4Addr;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct FleetConfig {
    pub domain: Option<String>,
    pub ssh_key: Option<String>,
    #[serde(default = "default_network")]
    pub network: String,
    #[serde(default)]
    pub servers: HashMap<String, Server>,
    #[serde(default)]
    pub apps: HashMap<String, App>,
    #[serde(default)]
    pub runners: HashMap<String, Runner>,
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct ChartsConfig {
    #[serde(default)]
    pub chart: Vec<Chart>,
    #[serde(default)]
    pub apps: HashMap<String, AppCharts>,
}

#[derive(Debug, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct AppCharts {
    #[serde(default)]
    pub chart: Vec<Chart>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Chart {
    pub title: String,
    #[serde(rename = "type", default = "default_chart_type")]
    pub chart_type: String,
    pub stream: String,
    pub x: String,
    pub y: String,
    pub sql: String,
    pub width: Option<String>,
}

fn default_chart_type() -> String {
    "bar".to_string()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct WebhookConfig {
    pub server: String,
    pub domain: String,
}

fn default_network() -> String {
    "flow".to_string()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub host: String,
    pub ip: Option<String>,
    #[serde(default = "default_user")]
    pub user: String,
    pub ssh_key: Option<String>,
}

fn default_user() -> String {
    "deploy".to_string()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct App {
    pub image: String,
    #[serde(default)]
    pub servers: Vec<String>,
    pub port: Option<u16>,
    #[serde(default)]
    pub deploy_strategy: DeployStrategy,
    #[serde(default)]
    pub routing: Option<Routing>,
    #[serde(default)]
    pub services: Vec<Sidecar>,
    #[serde(default)]
    pub ports: Vec<PortMapping>,
    #[serde(default)]
    pub r2_buckets: Vec<R2Bucket>,
    #[serde(default)]
    pub volumes: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct R2Bucket {
    pub name: String,
    pub public_domain: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeployStrategy {
    #[default]
    Rolling,
    Recreate,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Routing {
    #[serde(default)]
    pub domains: Vec<String>,
    pub health_path: Option<String>,
    pub health_interval: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Sidecar {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub volumes: Vec<String>,
    pub healthcheck: Option<String>,
    pub depends_on: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct PortMapping {
    pub internal: u16,
    pub external: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

fn default_protocol() -> String {
    "tcp".to_string()
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Runner {
    pub server: String,
    pub scope: RunnerScope,
    pub target: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default = "default_ephemeral")]
    pub ephemeral: bool,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum RunnerScope {
    Org,
    Repo,
}

fn default_ephemeral() -> bool {
    true
}

#[derive(Debug, Deserialize, Default)]
pub struct EnvConfig {
    #[serde(default)]
    pub apps: HashMap<String, AppEnv>,
    #[serde(default)]
    pub fleet: FleetSecrets,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AppEnv {
    #[serde(flatten)]
    pub env: HashMap<String, toml::Value>,
    #[serde(default)]
    pub services: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct FleetSecrets {
    pub gh_token: Option<String>,
    pub gh_username: Option<String>,
    pub cloudflare_api_token: Option<String>,
    pub cloudflare_account_id: Option<String>,
    pub tail_url: Option<String>,
    pub tail_user: Option<String>,
    pub tail_password: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub oo_rum_token: Option<String>,
    pub github_webhook_secret: Option<String>,
}

#[derive(Debug)]
pub struct Fleet {
    pub domain: Option<String>,
    pub network: String,
    pub servers: HashMap<String, Server>,
    pub apps: HashMap<String, ResolvedApp>,
    pub runners: HashMap<String, Runner>,
    pub secrets: FleetSecrets,
    pub webhook: Option<WebhookConfig>,
    pub charts: Vec<Chart>,
}

impl Fleet {
    pub fn apps_with_r2(&self) -> Vec<&ResolvedApp> {
        let mut apps: Vec<&ResolvedApp> = self
            .apps
            .values()
            .filter(|a| !a.r2_buckets.is_empty())
            .collect();
        apps.sort_by(|a, b| a.name.cmp(&b.name));
        apps
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedApp {
    pub name: String,
    pub image: String,
    pub servers: Vec<String>,
    pub port: Option<u16>,
    pub deploy_strategy: DeployStrategy,
    pub routing: Option<Routing>,
    pub env: HashMap<String, String>,
    pub services: Vec<ResolvedSidecar>,
    pub ports: Vec<PortMapping>,
    pub r2_buckets: Vec<R2Bucket>,
    pub volumes: Vec<String>,
    pub charts: Vec<Chart>,
}

#[derive(Debug, Clone)]
pub struct ResolvedSidecar {
    pub name: String,
    pub image: String,
    pub volumes: Vec<String>,
    pub env: HashMap<String, String>,
    pub healthcheck: Option<String>,
    pub depends_on: Option<String>,
}

fn is_valid_r2_bucket_name(s: &str) -> bool {
    let len = s.len();
    if !(3..=63).contains(&len) {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes[0] == b'-' || bytes[len - 1] == b'-' {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_valid_caddy_duration(s: &str) -> bool {
    for suffix in &["ms", "s", "m", "h", "d"] {
        if let Some(num_part) = s.strip_suffix(suffix) {
            return !num_part.is_empty() && num_part.parse::<f64>().is_ok();
        }
    }
    false
}

fn validate(config: &FleetConfig) -> Result<()> {
    for (server_name, server) in &config.servers {
        if let Some(ref ip) = server.ip {
            if ip.parse::<Ipv4Addr>().is_err() {
                bail!("Server '{server_name}' has invalid IP '{ip}'");
            }
        }
    }

    let mut all_domains: Vec<(&str, &str)> = Vec::new();

    for (app_name, app) in &config.apps {
        if app.servers.is_empty() {
            bail!("App '{app_name}' has no servers");
        }

        if app.image.is_empty() {
            bail!("App '{app_name}' has an empty image");
        }

        if app.routing.is_some() && app.port.is_none() {
            bail!("App '{app_name}' has routing but no port");
        }

        if !app.ports.is_empty() && app.routing.is_some() {
            bail!("App '{app_name}' has both routing and ports (mutually exclusive)");
        }

        if let Some(port) = app.port {
            if port == 0 {
                bail!("App '{app_name}' has invalid port 0");
            }
        }
        for pm in &app.ports {
            if pm.internal == 0 || pm.external == 0 {
                bail!("App '{app_name}' has invalid port 0");
            }
            if pm.protocol != "tcp" && pm.protocol != "udp" {
                bail!(
                    "App '{app_name}' has invalid port protocol '{}' (must be tcp or udp)",
                    pm.protocol
                );
            }
        }

        if let Some(ref routing) = app.routing {
            for domain in &routing.domains {
                if domain.is_empty() {
                    bail!("App '{app_name}' has an empty domain");
                }
                if domain.contains(char::is_whitespace) {
                    bail!("App '{app_name}' has domain '{domain}' containing whitespace");
                }
                if domain.contains("://") {
                    bail!(
                        "App '{app_name}' has domain '{domain}' with protocol prefix (use bare hostname)"
                    );
                }
                if !domain.contains('.') {
                    bail!(
                        "App '{app_name}' has domain '{domain}' with no dot (expected hostname like example.com)"
                    );
                }
                all_domains.push((domain, app_name));
            }
            if let Some(ref health_path) = routing.health_path {
                if !health_path.starts_with('/') {
                    bail!(
                        "App '{app_name}' has invalid health_path '{health_path}' (must start with /)"
                    );
                }
            }
            if let Some(ref health_interval) = routing.health_interval {
                if !is_valid_caddy_duration(health_interval) {
                    bail!(
                        "App '{app_name}' has invalid health_interval '{health_interval}' (expected format: 5s, 1m, 500ms)"
                    );
                }
            }
        }

        let mut seen_bucket_names: HashSet<&str> = HashSet::new();
        for bucket in &app.r2_buckets {
            if bucket.name.is_empty() {
                bail!("App '{app_name}' has an R2 bucket with an empty name");
            }
            if !is_valid_r2_bucket_name(&bucket.name) {
                bail!(
                    "App '{app_name}' has invalid R2 bucket name '{}' (lowercase alphanumerics + hyphens, 3-63 chars)",
                    bucket.name
                );
            }
            if !seen_bucket_names.insert(&bucket.name) {
                bail!(
                    "App '{app_name}' has duplicate R2 bucket name '{}'",
                    bucket.name
                );
            }
            if let Some(ref domain) = bucket.public_domain {
                if domain.is_empty() {
                    bail!(
                        "App '{app_name}' R2 bucket '{}' has an empty public_domain",
                        bucket.name
                    );
                }
                if domain.contains("://") || domain.contains(char::is_whitespace) {
                    bail!(
                        "App '{app_name}' R2 bucket '{}' has invalid public_domain '{domain}'",
                        bucket.name
                    );
                }
                if !domain.contains('.') {
                    bail!(
                        "App '{app_name}' R2 bucket '{}' has public_domain '{domain}' with no dot",
                        bucket.name
                    );
                }
            }
        }

        let sidecar_names: Vec<&str> = app.services.iter().map(|s| s.name.as_str()).collect();
        let mut seen_sidecar_names: HashSet<&str> = HashSet::new();
        for name in &sidecar_names {
            if !seen_sidecar_names.insert(name) {
                bail!("App '{app_name}' has duplicate service name '{name}'");
            }
        }
        for svc in &app.services {
            if svc.image.is_empty() {
                bail!(
                    "Service '{}' in app '{}' has an empty image",
                    svc.name,
                    app_name
                );
            }
            if let Some(ref dep) = svc.depends_on {
                if !sidecar_names.contains(&dep.as_str()) {
                    bail!(
                        "Service '{}' in app '{}' depends on '{}' which doesn't exist",
                        svc.name,
                        app_name,
                        dep
                    );
                }
            }
        }
    }

    let mut seen_domains: HashMap<&str, &str> = HashMap::new();
    for (domain, app_name) in &all_domains {
        if let Some(other_app) = seen_domains.get(domain) {
            bail!("Duplicate domain '{domain}' in apps '{other_app}' and '{app_name}'");
        }
        seen_domains.insert(domain, app_name);
    }

    for (runner_name, runner) in &config.runners {
        if runner.target.is_empty() {
            bail!("Runner '{runner_name}' has an empty target");
        }
        if !config.servers.contains_key(&runner.server) {
            bail!(
                "Runner '{runner_name}' references unknown server '{}'",
                runner.server
            );
        }
    }

    if let Some(ref webhook) = config.webhook {
        if !config.servers.contains_key(&webhook.server) {
            bail!("[webhook] references unknown server '{}'", webhook.server);
        }
        if webhook.domain.is_empty() {
            bail!("[webhook] has an empty domain");
        }
        if !webhook.domain.contains('.') {
            bail!(
                "[webhook] has invalid domain '{}' (expected hostname like webhooks.example.com)",
                webhook.domain
            );
        }
    }

    Ok(())
}

pub fn load(config_path: &str) -> Result<Fleet> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let env_path = config_path.with_file_name("fleet.env.toml");
    let env_config: EnvConfig = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?
    } else {
        EnvConfig::default()
    };

    let charts_path = config_path.with_file_name("fleet.charts.toml");
    let charts_config: ChartsConfig = if charts_path.exists() {
        let charts_content = std::fs::read_to_string(&charts_path)
            .with_context(|| format!("Failed to read {}", charts_path.display()))?;
        toml::from_str(&charts_content)
            .with_context(|| format!("Failed to parse {}", charts_path.display()))?
    } else {
        ChartsConfig::default()
    };

    for (app_name, app) in &config.apps {
        for server in &app.servers {
            if !config.servers.contains_key(server) {
                bail!("App '{app_name}' references unknown server '{server}'");
            }
        }
    }

    validate(&config)?;

    let mut resolved_apps = HashMap::new();
    for (name, app) in config.apps {
        let mut env = HashMap::new();

        if let Some(app_env) = env_config.apps.get(&name) {
            for (k, v) in &app_env.env {
                if let toml::Value::String(s) = v {
                    env.insert(k.clone(), s.clone());
                }
            }
        }

        let resolved_services: Vec<ResolvedSidecar> = app
            .services
            .iter()
            .map(|svc| {
                let mut svc_env = HashMap::new();
                if let Some(app_env) = env_config.apps.get(&name) {
                    if let Some(svc_env_vals) = app_env.services.get(&svc.name) {
                        for (k, v) in svc_env_vals {
                            svc_env.insert(k.clone(), v.clone());
                        }
                    }
                }
                ResolvedSidecar {
                    name: svc.name.clone(),
                    image: svc.image.clone(),
                    volumes: svc.volumes.clone(),
                    env: svc_env,
                    healthcheck: svc.healthcheck.clone(),
                    depends_on: svc.depends_on.clone(),
                }
            })
            .collect();

        let app_charts = charts_config
            .apps
            .get(&name)
            .map(|c| c.chart.clone())
            .unwrap_or_default();

        resolved_apps.insert(
            name.clone(),
            ResolvedApp {
                name: name.clone(),
                image: app.image,
                servers: app.servers,
                port: app.port,
                deploy_strategy: app.deploy_strategy,
                routing: app.routing,
                env,
                services: resolved_services,
                ports: app.ports,
                r2_buckets: app.r2_buckets,
                volumes: app.volumes,
                charts: app_charts,
            },
        );
    }

    Ok(Fleet {
        domain: config.domain,
        network: config.network,
        servers: config.servers,
        apps: resolved_apps,
        runners: config.runners,
        secrets: env_config.fleet,
        webhook: config.webhook,
        charts: charts_config.chart,
    })
}
