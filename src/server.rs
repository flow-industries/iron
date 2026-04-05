use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::cli::ServerCommand;
use crate::config::{EnvConfig, FleetConfig, FleetSecrets, Server};
use crate::ssh::SshPool;
use crate::ui;

const CADDY_COMPOSE: &str = include_str!("../stacks/caddy/docker-compose.yml");
const ROLLOUT_SCRIPT: &str = include_str!("../stacks/wud/rollout.sh");

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
            let interactive = name.is_none() && ip.is_none();
            let (name, ip, host, user, ssh_user, ssh_key) = if interactive {
                interactive_add(config_path)?
            } else {
                let name = name.context("Server name is required")?;
                let ip = ip.context("--ip is required")?;
                let user = user.unwrap_or_else(|| "deploy".to_string());
                let ssh_user = ssh_user.unwrap_or_else(|| "root".to_string());
                (name, ip, host, user, ssh_user, ssh_key)
            };
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
    }
}

#[allow(clippy::type_complexity)]
fn interactive_add(
    config_path: &str,
) -> Result<(
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
)> {
    let config_path_p = Path::new(config_path);
    let content = std::fs::read_to_string(config_path_p)
        .with_context(|| format!("Failed to read {}", config_path_p.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path_p.display()))?;

    ui::header("Add server");

    let Some(name) = ui::prompt("Server name:") else {
        bail!("Server name is required");
    };

    let Some(ip) = ui::prompt("Server IP address:") else {
        bail!("IP address is required");
    };

    let default_host = config.domain.as_ref().map(|d| format!("{name}.{d}"));
    let host_prompt = if let Some(ref default) = default_host {
        format!("Hostname (empty for {default}):")
    } else {
        "Hostname:".to_string()
    };
    let host = ui::prompt(&host_prompt);

    let user =
        ui::prompt("Deploy user (empty for deploy):").unwrap_or_else(|| "deploy".to_string());

    let ssh_user = ui::prompt("SSH user for initial setup (empty for root):")
        .unwrap_or_else(|| "root".to_string());

    let ssh_key = ui::prompt("SSH public key path (empty to auto-detect):");

    Ok((name, ip, host, user, ssh_user, ssh_key))
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

fn generate_wud_compose(secrets: &FleetSecrets) -> String {
    let gh_username = secrets.gh_username.as_deref().unwrap_or("");
    let gh_token = secrets.gh_token.as_deref().unwrap_or("");

    let mut notify_env = String::new();
    if let (Some(bot_token), Some(chat_id)) =
        (&secrets.telegram_bot_token, &secrets.telegram_chat_id)
    {
        notify_env.push_str(&format!(
            "      NOTIFY_TELEGRAM_BOT_TOKEN: {bot_token}\n\
             \x20     NOTIFY_TELEGRAM_CHAT_ID: {chat_id}\n"
        ));
    }
    if let Some(webhook_url) = &secrets.discord_webhook_url {
        notify_env.push_str(&format!(
            "      NOTIFY_DISCORD_WEBHOOK_URL: {webhook_url}\n"
        ));
    }

    format!(
        r#"services:
  wud:
    image: getwud/wud:latest
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock
      - /usr/bin/docker:/usr/bin/docker:ro
      - /usr/libexec/docker/cli-plugins:/usr/libexec/docker/cli-plugins:ro
      - /opt/flow:/opt/flow:ro
      - ./rollout.sh:/rollout.sh:ro
    environment:
      WUD_LOG_LEVEL: debug
      WUD_WATCHER_LOCAL_CRON: "*/5 * * * * *"
      WUD_WATCHER_LOCAL_WATCHBYDEFAULT: "false"
      WUD_WATCHER_LOCAL_JITTER: "0"
      WUD_REGISTRY_GHCR_FLOW_USERNAME: {gh_username}
      WUD_REGISTRY_GHCR_FLOW_TOKEN: {gh_token}
      WUD_TRIGGER_COMMAND_ROLLOUT_CMD: /rollout.sh
      WUD_TRIGGER_COMMAND_ROLLOUT_SHELL: /bin/sh
      WUD_TRIGGER_COMMAND_ROLLOUT_TIMEOUT: "120000"
      WUD_TRIGGER_DOCKER_GAMEUPDATE_PRUNE: "true"
{notify_env}    restart: always
"#
    )
}

pub async fn deploy_infra(
    pool: &SshPool,
    server_name: &str,
    network: &str,
    secrets: &FleetSecrets,
) -> Result<()> {
    let sp = ui::spinner("Setting up infrastructure containers...");

    pool.exec(
        server_name,
        &format!("docker network create {network} 2>/dev/null || true"),
    )
    .await?;

    pool.exec(server_name, "mkdir -p /opt/flow/caddy/sites /opt/flow/wud")
        .await?;

    pool.upload_file(
        server_name,
        "/opt/flow/caddy/docker-compose.yml",
        CADDY_COMPOSE,
    )
    .await?;

    pool.exec(server_name, "cd /opt/flow/caddy && docker compose up -d")
        .await?;

    if secrets.gh_username.is_some() && secrets.gh_token.is_some() {
        let wud_compose = generate_wud_compose(secrets);
        pool.upload_file(
            server_name,
            "/opt/flow/wud/docker-compose.yml",
            &wud_compose,
        )
        .await?;
        pool.upload_file(server_name, "/opt/flow/wud/rollout.sh", ROLLOUT_SCRIPT)
            .await?;
        pool.exec(server_name, "chmod +x /opt/flow/wud/rollout.sh")
            .await?;
        pool.exec(server_name, "cd /opt/flow/wud && docker compose up -d")
            .await?;
        sp.finish_and_clear();
        ui::success("Caddy + WUD started");
    } else {
        sp.finish_and_clear();
        ui::success("Caddy started");
        ui::error(
            "WUD skipped: set gh_username and gh_token in fleet.env.toml (or use `flow login gh`)",
        );
    }

    Ok(())
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
    let fleet_secrets = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        let env_config: EnvConfig = toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?;
        env_config.fleet
    } else {
        FleetSecrets::default()
    };

    let cf_token = fleet_secrets
        .cloudflare_api_token
        .as_deref()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Cannot create DNS record: cloudflare_api_token not set in fleet.env.toml\n  \
                 Run `flow login cf` to set it"
            )
        })?;

    let sp = ui::spinner(&format!("Creating DNS record {hostname} → {ip}..."));
    crate::cloudflare::ensure_dns_record(cf_token, &hostname, ip).await?;
    sp.finish_and_clear();
    ui::success(&format!("{hostname} → {ip}"));

    let _ = tokio::process::Command::new("ssh-keygen")
        .args(["-R", ip])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await;

    ui::header("Ansible setup");
    let mut cmd = tokio::process::Command::new(&ansible_playbook);
    cmd.arg("ansible/setup.yml")
        .arg("-i")
        .arg(format!("{ip},"))
        .arg("-u")
        .arg(ssh_user)
        .arg("-e")
        .arg(format!("ssh_pub_key_path={resolved_key}"))
        .env("ANSIBLE_HOST_KEY_CHECKING", "False")
        .current_dir(project_dir);

    if let Some(ref token) = fleet_secrets.gh_token {
        cmd.arg("-e").arg(format!("gh_token={token}"));
    }

    let status = cmd
        .status()
        .await
        .context("Failed to run ansible-playbook")?;

    if !status.success() {
        bail!("Ansible setup failed (exit code: {status})");
    }

    ui::header("Infrastructure");
    let server_entry = Server {
        host: hostname.clone(),
        ip: Some(ip.to_string()),
        user: user.to_string(),
        ssh_key: cli_ssh_key.map(String::from),
    };
    let pool = SshPool::connect_one(name, &server_entry).await?;
    deploy_infra(&pool, name, &config.network, &fleet_secrets).await?;
    pool.close().await?;

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

pub async fn run_hardening(config_path: &str, server_filter: Option<&str>) -> Result<()> {
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
    let gh_token = if env_path.exists() {
        let env_content = std::fs::read_to_string(&env_path)
            .with_context(|| format!("Failed to read {}", env_path.display()))?;
        let env_config: EnvConfig = toml::from_str(&env_content)
            .with_context(|| format!("Failed to parse {}", env_path.display()))?;
        env_config.fleet.gh_token.filter(|t| !t.is_empty())
    } else {
        None
    };

    let servers: Vec<(String, Server)> = if let Some(name) = server_filter {
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

    for (name, server) in &servers {
        let ip = server
            .ip
            .as_deref()
            .with_context(|| format!("Server '{name}' has no IP address"))?;

        ui::header(&format!("Hardening: {name} ({} / {ip})", server.host));

        let mut cmd = tokio::process::Command::new(&ansible_playbook);
        cmd.arg("ansible/setup.yml")
            .arg("-i")
            .arg(format!("{ip},"))
            .arg("-u")
            .arg("root")
            .arg("-e")
            .arg(format!("ssh_pub_key_path={resolved_key}"))
            .current_dir(project_dir);

        if let Some(ref token) = gh_token {
            cmd.arg("-e").arg(format!("gh_token={token}"));
        }

        let status = cmd
            .status()
            .await
            .context("Failed to run ansible-playbook")?;

        if status.success() {
            ui::success(&format!("Server '{name}' hardening up to date"));
        } else {
            ui::error(&format!("Server '{name}' hardening failed"));
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
        .or_insert_with(|| {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            toml_edit::Item::Table(t)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wud_compose_includes_telegram_when_configured() {
        let secrets = FleetSecrets {
            gh_username: Some("org".into()),
            gh_token: Some("tok".into()),
            telegram_bot_token: Some("123:ABC".into()),
            telegram_chat_id: Some("-100999".into()),
            ..Default::default()
        };
        let output = generate_wud_compose(&secrets);
        assert!(output.contains("NOTIFY_TELEGRAM_BOT_TOKEN: 123:ABC"));
        assert!(output.contains("NOTIFY_TELEGRAM_CHAT_ID: -100999"));
        assert!(!output.contains("NOTIFY_DISCORD_WEBHOOK_URL"));
    }

    #[test]
    fn wud_compose_includes_discord_when_configured() {
        let secrets = FleetSecrets {
            gh_username: Some("org".into()),
            gh_token: Some("tok".into()),
            discord_webhook_url: Some("https://discord.com/api/webhooks/123/abc".into()),
            ..Default::default()
        };
        let output = generate_wud_compose(&secrets);
        assert!(
            output.contains("NOTIFY_DISCORD_WEBHOOK_URL: https://discord.com/api/webhooks/123/abc")
        );
        assert!(!output.contains("NOTIFY_TELEGRAM"));
    }

    #[test]
    fn wud_compose_omits_notifications_when_not_configured() {
        let secrets = FleetSecrets {
            gh_username: Some("org".into()),
            gh_token: Some("tok".into()),
            ..Default::default()
        };
        let output = generate_wud_compose(&secrets);
        assert!(!output.contains("NOTIFY_"));
        assert!(output.contains("WUD_REGISTRY_GHCR_FLOW_USERNAME: org"));
    }

    #[test]
    fn wud_compose_includes_both_when_configured() {
        let secrets = FleetSecrets {
            gh_username: Some("org".into()),
            gh_token: Some("tok".into()),
            telegram_bot_token: Some("123:ABC".into()),
            telegram_chat_id: Some("-100999".into()),
            discord_webhook_url: Some("https://discord.com/api/webhooks/123/abc".into()),
            ..Default::default()
        };
        let output = generate_wud_compose(&secrets);
        assert!(output.contains("NOTIFY_TELEGRAM_BOT_TOKEN: 123:ABC"));
        assert!(output.contains("NOTIFY_TELEGRAM_CHAT_ID: -100999"));
        assert!(
            output.contains("NOTIFY_DISCORD_WEBHOOK_URL: https://discord.com/api/webhooks/123/abc")
        );
    }
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
