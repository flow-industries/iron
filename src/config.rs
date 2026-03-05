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
    pub routes: Vec<String>,
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
    pub ghcr_token: Option<String>,
    pub cloudflare_api_token: Option<String>,
}

#[derive(Debug)]
pub struct Fleet {
    pub domain: Option<String>,
    pub network: String,
    pub servers: HashMap<String, Server>,
    pub apps: HashMap<String, ResolvedApp>,
    pub secrets: FleetSecrets,
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

    let mut all_routes: Vec<(&str, &str)> = Vec::new();

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
            for route in &routing.routes {
                if route.is_empty() {
                    bail!("App '{app_name}' has an empty route");
                }
                if route.contains(char::is_whitespace) {
                    bail!("App '{app_name}' has route '{route}' containing whitespace");
                }
                if route.contains("://") {
                    bail!(
                        "App '{app_name}' has route '{route}' with protocol prefix (use bare hostname)"
                    );
                }
                if !route.contains('.') {
                    bail!(
                        "App '{app_name}' has route '{route}' with no domain (expected hostname like example.com)"
                    );
                }
                all_routes.push((route, app_name));
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

    let mut seen_routes: HashMap<&str, &str> = HashMap::new();
    for (route, app_name) in &all_routes {
        if let Some(other_app) = seen_routes.get(route) {
            bail!("Duplicate route '{route}' in apps '{other_app}' and '{app_name}'");
        }
        seen_routes.insert(route, app_name);
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
            },
        );
    }

    Ok(Fleet {
        domain: config.domain,
        network: config.network,
        servers: config.servers,
        apps: resolved_apps,
        secrets: env_config.fleet,
    })
}
