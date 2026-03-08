use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::ServerCommand;
use crate::config::{EnvConfig, FleetConfig, Server};
use crate::ui;

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

fn resolve_ssh_key(server_key: Option<&str>, fleet_key: Option<&str>) -> Result<String> {
    if let Some(key) = server_key {
        let expanded = expand_tilde(key);
        if !Path::new(&expanded).exists() {
            bail!("SSH public key not found: {expanded}");
        }
        return Ok(expanded);
    }

    if let Some(key) = fleet_key {
        let expanded = expand_tilde(key);
        if !Path::new(&expanded).exists() {
            bail!("SSH public key not found: {expanded}");
        }
        return Ok(expanded);
    }

    let home = std::env::var("HOME").context("HOME not set")?;
    let ed25519 = format!("{home}/.ssh/id_ed25519.pub");
    if Path::new(&ed25519).exists() {
        return Ok(ed25519);
    }

    let rsa = format!("{home}/.ssh/id_rsa.pub");
    if Path::new(&rsa).exists() {
        return Ok(rsa);
    }

    bail!(
        "No SSH public key found. Provide ssh_key in fleet.toml, use --ssh-key, \
         or ensure ~/.ssh/id_ed25519.pub or ~/.ssh/id_rsa.pub exists"
    )
}

pub async fn run(config_path: &str, command: ServerCommand) -> Result<()> {
    match command {
        ServerCommand::Add {
            name,
            ip,
            host,
            user,
            ssh_user,
            ssh_key,
        } => {
            add(
                config_path,
                &name,
                &ip,
                host.as_deref(),
                &user,
                &ssh_user,
                ssh_key.as_deref(),
            )
            .await
        }
        ServerCommand::Remove { name } => remove(config_path, &name),
        ServerCommand::Check { name, ssh_user } => {
            check(config_path, name.as_deref(), &ssh_user).await
        }
    }
}

async fn ensure_ansible(project_dir: &Path) -> Result<String> {
    if let Some(path) = resolve_command("ansible-playbook").await {
        return Ok(path);
    }

    if let Some(pipx) = resolve_command("pipx").await {
        if !ui::confirm("ansible-playbook not found. Install via pipx? (y/N)") {
            bail!("ansible-playbook is required for server setup");
        }

        let status = tokio::process::Command::new(&pipx)
            .args(["install", "ansible-core"])
            .status()
            .await
            .context("Failed to run pipx")?;
        if !status.success() {
            bail!("Failed to install ansible-core via pipx");
        }
        ui::success("Installed ansible-core");
    } else if let Some(pip3) = resolve_command("pip3").await {
        if !ui::confirm("ansible-playbook not found. Install via pip3? (y/N)") {
            bail!("ansible-playbook is required for server setup");
        }

        let status = tokio::process::Command::new(&pip3)
            .args(["install", "--user", "ansible-core"])
            .status()
            .await
            .context("Failed to run pip3")?;
        if !status.success() {
            bail!("Failed to install ansible-core via pip3");
        }
        ui::success("Installed ansible-core");
    } else {
        bail!(
            "ansible-playbook not found and no installer available.\n  \
             Install manually: pip3 install ansible-core  OR  brew install ansible"
        );
    }

    install_ansible_roles(project_dir).await?;

    resolve_command("ansible-playbook")
        .await
        .context("ansible-playbook still not found after install")
}

async fn install_ansible_roles(project_dir: &Path) -> Result<()> {
    let requirements = project_dir.join("ansible/requirements.yml");
    if !requirements.exists() {
        return Ok(());
    }

    let galaxy = resolve_command("ansible-galaxy")
        .await
        .context("ansible-galaxy not found")?;

    let sp = ui::spinner("Installing Ansible roles...");
    let status = tokio::process::Command::new(&galaxy)
        .args(["install", "-r", "ansible/requirements.yml"])
        .current_dir(project_dir)
        .status()
        .await
        .context("Failed to run ansible-galaxy")?;
    sp.finish_and_clear();

    if !status.success() {
        bail!("Failed to install Ansible roles");
    }
    ui::success("Ansible roles installed");
    Ok(())
}

async fn resolve_command(name: &str) -> Option<String> {
    let output = tokio::process::Command::new("which")
        .arg(name)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?;
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

async fn add(
    config_path: &str,
    name: &str,
    ip: &str,
    host_override: Option<&str>,
    user: &str,
    ssh_user: &str,
    cli_ssh_key: Option<&str>,
) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if config.servers.contains_key(name) {
        bail!("Server '{name}' already exists");
    }

    let hostname = if let Some(h) = host_override {
        h.to_string()
    } else {
        let domain = config.domain.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot derive hostname: no 'domain' in fleet.toml (use --host to specify)"
            )
        })?;
        format!("{name}.{domain}")
    };

    let project_dir = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let ansible_dir = project_dir.join("ansible");
    if !ansible_dir.join("setup.yml").exists() {
        bail!(
            "Ansible playbook not found at {}",
            ansible_dir.join("setup.yml").display()
        );
    }

    let resolved_key = resolve_ssh_key(cli_ssh_key, config.ssh_key.as_deref())?;

    let ansible_playbook = ensure_ansible(project_dir).await?;

    let env_path = config_path.with_file_name("fleet.env.toml");
    let (ghcr_token, cf_token) = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        let env_config: EnvConfig = toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?;
        (
            env_config.fleet.ghcr_token.filter(|t| !t.is_empty()),
            env_config
                .fleet
                .cloudflare_api_token
                .filter(|t| !t.is_empty()),
        )
    } else {
        (None, None)
    };

    let cf_token = cf_token.ok_or_else(|| {
        anyhow::anyhow!(
            "Cannot create DNS record: cloudflare_api_token not set in fleet.env.toml\n  \
             Run `flow login cf` to set it"
        )
    })?;

    let sp = ui::spinner(&format!("Creating DNS record {hostname} → {ip}..."));
    crate::cloudflare::ensure_dns_record(&cf_token, &hostname, ip).await?;
    sp.finish_and_clear();
    ui::success(&format!("{hostname} → {ip}"));

    ui::header("Ansible setup");
    let mut cmd = tokio::process::Command::new(&ansible_playbook);
    cmd.arg("ansible/setup.yml")
        .arg("-i")
        .arg(format!("{ip},"))
        .arg("-u")
        .arg(ssh_user)
        .arg("-e")
        .arg(format!("ssh_pub_key_path={resolved_key}"))
        .current_dir(project_dir);

    if let Some(ref token) = ghcr_token {
        cmd.arg("-e").arg(format!("ghcr_token={token}"));
    }

    let status = cmd
        .status()
        .await
        .context("Failed to run ansible-playbook")?;

    if !status.success() {
        bail!("Ansible setup failed (exit code: {status})");
    }

    write_server_to_config(config_path, name, &hostname, ip, user, cli_ssh_key)?;
    ui::success(&format!("Server '{name}' added and bootstrapped"));
    Ok(())
}

fn remove(config_path: &str, name: &str) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;

    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if !config.servers.contains_key(name) {
        bail!("Server '{name}' does not exist");
    }

    let referencing_apps: Vec<&String> = config
        .apps
        .iter()
        .filter(|(_, app)| app.servers.contains(&name.to_string()))
        .map(|(app_name, _)| app_name)
        .collect();

    if !referencing_apps.is_empty() {
        let app_list: Vec<&str> = referencing_apps.iter().map(|s| s.as_str()).collect();
        bail!(
            "Cannot remove server '{}': referenced by apps: {}",
            name,
            app_list.join(", ")
        );
    }

    remove_server_from_config(config_path, name)?;
    ui::success(&format!("Server '{name}' removed"));
    Ok(())
}

async fn check(config_path: &str, name: Option<&str>, ssh_user: &str) -> Result<()> {
    let config_path = Path::new(config_path);
    let fleet = crate::config::load(config_path.to_str().unwrap_or("fleet.toml"))?;

    let project_dir = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));

    let ansible_dir = project_dir.join("ansible");
    if !ansible_dir.join("setup.yml").exists() {
        bail!(
            "Ansible playbook not found at {}",
            ansible_dir.join("setup.yml").display()
        );
    }

    let ansible_playbook = ensure_ansible(project_dir).await?;
    let resolved_key = resolve_ssh_key(None, fleet.domain.as_deref().and(None))?;

    let env_path = config_path.with_file_name("fleet.env.toml");
    let ghcr_token = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        let env_config: EnvConfig = toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?;
        env_config.fleet.ghcr_token.filter(|t| !t.is_empty())
    } else {
        None
    };

    let servers_to_check: Vec<(String, Server)> = if let Some(name) = name {
        let server = fleet
            .servers
            .get(name)
            .with_context(|| format!("Server '{name}' not found"))?;
        vec![(name.to_string(), server.clone())]
    } else {
        fleet
            .servers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    };

    for (name, server) in &servers_to_check {
        let ip = server
            .ip
            .as_deref()
            .with_context(|| format!("Server '{name}' has no IP address"))?;

        ui::header(&format!("Server: {name} ({} / {ip})", server.host));

        let mut cmd = tokio::process::Command::new(&ansible_playbook);
        cmd.arg("ansible/setup.yml")
            .arg("-i")
            .arg(format!("{ip},"))
            .arg("-u")
            .arg(ssh_user)
            .arg("-e")
            .arg(format!("ssh_pub_key_path={resolved_key}"))
            .current_dir(project_dir);

        if let Some(ref token) = ghcr_token {
            cmd.arg("-e").arg(format!("ghcr_token={token}"));
        }

        let status = cmd
            .status()
            .await
            .context("Failed to run ansible-playbook")?;

        if status.success() {
            ui::success(&format!("Server '{name}' is up to date"));
        } else {
            ui::error(&format!("Server '{name}' check failed"));
        }
    }

    Ok(())
}

pub fn write_server_to_config(
    config_path: &Path,
    name: &str,
    host: &str,
    ip: &str,
    user: &str,
    ssh_key: Option<&str>,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let servers = doc
        .entry("servers")
        .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()))
        .as_table_mut()
        .context("'servers' is not a table")?;

    let mut server_table = toml_edit::Table::new();
    server_table.insert("host", toml_edit::value(host));
    server_table.insert("ip", toml_edit::value(ip));
    server_table.insert("user", toml_edit::value(user));
    if let Some(key) = ssh_key {
        server_table.insert("ssh_key", toml_edit::value(key));
    }
    servers.insert(name, toml_edit::Item::Table(server_table));

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn remove_server_from_config(config_path: &Path, name: &str) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let servers = doc
        .get_mut("servers")
        .and_then(|s| s.as_table_mut())
        .context("'servers' table not found")?;

    servers.remove(name);

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}
