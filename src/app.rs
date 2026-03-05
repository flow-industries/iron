use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::cli::AppCommand;
use crate::config::FleetConfig;
use crate::ui;

pub struct ParsedPortMap {
    pub internal: u16,
    pub external: u16,
    pub protocol: String,
}

fn parse_port_map(s: &str) -> Result<ParsedPortMap> {
    let (ports_part, protocol) = if let Some((p, proto)) = s.rsplit_once('/') {
        (p, proto.to_string())
    } else {
        (s, "tcp".to_string())
    };

    if protocol != "tcp" && protocol != "udp" {
        bail!("Invalid protocol '{protocol}' (must be tcp or udp)");
    }

    let (external_str, internal_str) = ports_part
        .split_once(':')
        .context("Port map must be in external:internal format")?;

    let external: u16 = external_str
        .parse()
        .context("Invalid external port number")?;
    let internal: u16 = internal_str
        .parse()
        .context("Invalid internal port number")?;

    if external == 0 || internal == 0 {
        bail!("Ports must be non-zero");
    }

    Ok(ParsedPortMap {
        internal,
        external,
        protocol,
    })
}

pub fn run(config_path: &str, command: AppCommand) -> Result<()> {
    match command {
        AppCommand::Add {
            name,
            image,
            server: servers,
            port,
            route: routes,
            health_path,
            health_interval,
            port_map: raw_port_maps,
            deploy_strategy,
        } => add(
            config_path,
            &name,
            &image,
            &servers,
            port,
            &routes,
            health_path.as_deref(),
            health_interval.as_deref(),
            &raw_port_maps,
            &deploy_strategy,
        ),
        AppCommand::AddService {
            app,
            name,
            image,
            volume: volumes,
            healthcheck,
            depends_on,
        } => add_service(
            config_path,
            &app,
            &name,
            &image,
            &volumes,
            healthcheck.as_deref(),
            depends_on.as_deref(),
        ),
        AppCommand::RemoveService { app, name } => remove_service(config_path, &app, &name),
    }
}

#[allow(clippy::too_many_arguments)]
fn add(
    config_path: &str,
    name: &str,
    image: &str,
    servers: &[String],
    port: Option<u16>,
    routes: &[String],
    health_path: Option<&str>,
    health_interval: Option<&str>,
    raw_port_maps: &[String],
    deploy_strategy: &str,
) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if config.apps.contains_key(name) {
        bail!("App '{name}' already exists");
    }

    for server in servers {
        if !config.servers.contains_key(server.as_str()) {
            bail!("Server '{server}' does not exist in fleet.toml");
        }
    }

    if !routes.is_empty() && !raw_port_maps.is_empty() {
        bail!("Cannot use both --route and --port-map (mutually exclusive)");
    }

    if !routes.is_empty() && port.is_none() {
        bail!("--port is required when using --route");
    }

    if routes.is_empty() && (health_path.is_some() || health_interval.is_some()) {
        bail!("--health-path and --health-interval require --route");
    }

    if deploy_strategy != "rolling" && deploy_strategy != "recreate" {
        bail!("Invalid deploy strategy '{deploy_strategy}' (must be 'rolling' or 'recreate')");
    }

    let port_maps: Vec<ParsedPortMap> = raw_port_maps
        .iter()
        .map(|s| parse_port_map(s))
        .collect::<Result<_>>()?;

    write_app_to_config(
        config_path,
        name,
        image,
        servers,
        port,
        routes,
        health_path,
        health_interval,
        &port_maps,
        deploy_strategy,
    )?;

    ui::success(&format!("App '{name}' added to fleet.toml"));
    ui::success(&format!("Run 'flow deploy {name}' to deploy"));
    Ok(())
}

fn add_service(
    config_path: &str,
    app_name: &str,
    service_name: &str,
    image: &str,
    volumes: &[String],
    healthcheck: Option<&str>,
    depends_on: Option<&str>,
) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let app = config
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("App '{app_name}' does not exist in fleet.toml"))?;

    if app.services.iter().any(|s| s.name == service_name) {
        bail!("Service '{service_name}' already exists in app '{app_name}'");
    }

    if let Some(dep) = depends_on {
        if !app.services.iter().any(|s| s.name == dep) {
            bail!("depends-on service '{dep}' does not exist in app '{app_name}'");
        }
    }

    write_service_to_config(
        config_path,
        app_name,
        service_name,
        image,
        volumes,
        healthcheck,
        depends_on,
    )?;

    ui::success(&format!(
        "Service '{service_name}' added to app '{app_name}'"
    ));
    Ok(())
}

fn remove_service(config_path: &str, app_name: &str, service_name: &str) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let app = config
        .apps
        .get(app_name)
        .ok_or_else(|| anyhow::anyhow!("App '{app_name}' does not exist in fleet.toml"))?;

    if !app.services.iter().any(|s| s.name == service_name) {
        bail!("Service '{service_name}' does not exist in app '{app_name}'");
    }

    remove_service_from_config(config_path, app_name, service_name)?;

    ui::success(&format!(
        "Service '{service_name}' removed from app '{app_name}'"
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn write_app_to_config(
    config_path: &Path,
    name: &str,
    image: &str,
    servers: &[String],
    port: Option<u16>,
    routes: &[String],
    health_path: Option<&str>,
    health_interval: Option<&str>,
    port_maps: &[ParsedPortMap],
    deploy_strategy: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let apps = doc
        .entry("apps")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("'apps' is not a table")?;

    let mut app_table = toml_edit::Table::new();
    app_table.insert("image", toml_edit::value(image));

    let mut servers_array = toml_edit::Array::new();
    for s in servers {
        servers_array.push(s.as_str());
    }
    app_table.insert("servers", toml_edit::value(servers_array));

    if let Some(p) = port {
        app_table.insert("port", toml_edit::value(i64::from(p)));
    }

    if deploy_strategy != "rolling" {
        app_table.insert("deploy_strategy", toml_edit::value(deploy_strategy));
    }

    if !routes.is_empty() {
        let mut routing_table = toml_edit::Table::new();
        let mut routes_array = toml_edit::Array::new();
        for r in routes {
            routes_array.push(r.as_str());
        }
        routing_table.insert("routes", toml_edit::value(routes_array));
        if let Some(hp) = health_path {
            routing_table.insert("health_path", toml_edit::value(hp));
        }
        if let Some(hi) = health_interval {
            routing_table.insert("health_interval", toml_edit::value(hi));
        }
        app_table.insert("routing", toml_edit::Item::Table(routing_table));
    }

    if !port_maps.is_empty() {
        let mut ports_array = toml_edit::ArrayOfTables::new();
        for pm in port_maps {
            let mut port_table = toml_edit::Table::new();
            port_table.insert("internal", toml_edit::value(i64::from(pm.internal)));
            port_table.insert("external", toml_edit::value(i64::from(pm.external)));
            if pm.protocol != "tcp" {
                port_table.insert("protocol", toml_edit::value(pm.protocol.as_str()));
            }
            ports_array.push(port_table);
        }
        app_table.insert("ports", toml_edit::Item::ArrayOfTables(ports_array));
    }

    apps.insert(name, toml_edit::Item::Table(app_table));

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn write_service_to_config(
    config_path: &Path,
    app_name: &str,
    service_name: &str,
    image: &str,
    volumes: &[String],
    healthcheck: Option<&str>,
    depends_on: Option<&str>,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let apps = doc
        .get_mut("apps")
        .and_then(|a| a.as_table_mut())
        .context("'apps' table not found")?;

    let app = apps
        .get_mut(app_name)
        .and_then(|a| a.as_table_mut())
        .with_context(|| format!("App '{app_name}' not found"))?;

    let services = app
        .entry("services")
        .or_insert_with(|| toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context("'services' is not an array of tables")?;

    let mut svc_table = toml_edit::Table::new();
    svc_table.insert("name", toml_edit::value(service_name));
    svc_table.insert("image", toml_edit::value(image));
    if !volumes.is_empty() {
        let mut vol_array = toml_edit::Array::new();
        for v in volumes {
            vol_array.push(v.as_str());
        }
        svc_table.insert("volumes", toml_edit::value(vol_array));
    }
    if let Some(hc) = healthcheck {
        svc_table.insert("healthcheck", toml_edit::value(hc));
    }
    if let Some(dep) = depends_on {
        svc_table.insert("depends_on", toml_edit::value(dep));
    }
    services.push(svc_table);

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn remove_service_from_config(
    config_path: &Path,
    app_name: &str,
    service_name: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let apps = doc
        .get_mut("apps")
        .and_then(|a| a.as_table_mut())
        .context("'apps' table not found")?;

    let app = apps
        .get_mut(app_name)
        .and_then(|a| a.as_table_mut())
        .with_context(|| format!("App '{app_name}' not found"))?;

    let services = app
        .get_mut("services")
        .and_then(|s| s.as_array_of_tables_mut())
        .with_context(|| format!("App '{app_name}' has no services"))?;

    let idx = (0..services.len())
        .find(|&i| {
            services
                .get(i)
                .and_then(|t| t.get("name"))
                .and_then(|n| n.as_str())
                == Some(service_name)
        })
        .with_context(|| format!("Service '{service_name}' not found in app '{app_name}'"))?;

    services.remove(idx);

    if services.is_empty() {
        app.remove("services");
    }

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}
