use std::path::Path;

use anyhow::{Context, Result, bail};
use comfy_table::{
    Cell, CellAlignment, Color, ContentArrangement, Table, modifiers::UTF8_ROUND_CORNERS,
    presets::UTF8_FULL_CONDENSED,
};
use serde::Deserialize;

use crate::cli::RunnerCommand;
use crate::config::{FleetConfig, Runner, RunnerScope};
use crate::ssh::SshPool;
use crate::ui;

pub async fn run(config_path: &str, command: RunnerCommand) -> Result<()> {
    match command {
        RunnerCommand::Add {
            name,
            server,
            scope,
            target,
            label: labels,
            ephemeral,
        } => {
            let interactive =
                name.is_none() && server.is_none() && scope.is_none() && target.is_none();
            if interactive {
                interactive_add(config_path)
            } else {
                let name = name.context("Runner name is required")?;
                let server = server.context("--server is required")?;
                let scope = scope.context("--scope is required")?;
                let target = target.context("--target is required")?;
                add(
                    config_path,
                    &name,
                    &server,
                    &scope,
                    &target,
                    &labels,
                    ephemeral,
                )
            }
        }
        RunnerCommand::Remove { name, yes } => remove(config_path, &name, yes).await,
        RunnerCommand::List => list(config_path).await,
    }
}

fn interactive_add(config_path: &str) -> Result<()> {
    let config_path_p = Path::new(config_path);
    let content = std::fs::read_to_string(config_path_p)
        .with_context(|| format!("Failed to read {}", config_path_p.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path_p.display()))?;

    ui::header("Add runner");

    let Some(name) = ui::prompt("Runner name:") else {
        bail!("Runner name is required");
    };

    if config.runners.contains_key(&name) {
        bail!("Runner '{name}' already exists");
    }

    let available_servers: Vec<&str> = config.servers.keys().map(String::as_str).collect();
    if available_servers.is_empty() {
        bail!("No servers in fleet.toml — add one with 'flow server add' first");
    }
    println!("  Available servers: {}", available_servers.join(", "));

    let server = loop {
        let Some(s) = ui::prompt("Server:") else {
            ui::error("Server is required");
            continue;
        };
        if !config.servers.contains_key(s.as_str()) {
            ui::error(&format!("Server '{s}' not in fleet.toml"));
            continue;
        }
        break s;
    };

    let scope = loop {
        let Some(s) = ui::prompt("Scope (org/repo):") else {
            ui::error("Scope is required");
            continue;
        };
        if s != "org" && s != "repo" {
            ui::error("Scope must be 'org' or 'repo'");
            continue;
        }
        break s;
    };

    let target_hint = if scope == "org" {
        "Org name (e.g. flow-industries):"
    } else {
        "Repository (e.g. owner/repo):"
    };
    let Some(target) = ui::prompt(target_hint) else {
        bail!("Target is required");
    };

    let labels_str = ui::prompt("Labels (comma-separated, empty for defaults):");
    let labels: Vec<String> = labels_str
        .map(|s| {
            s.split(',')
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let ephemeral = !ui::confirm("Persistent runner? (y/N)");

    add(
        config_path,
        &name,
        &server,
        &scope,
        &target,
        &labels,
        ephemeral,
    )
}

fn add(
    config_path: &str,
    name: &str,
    server: &str,
    scope: &str,
    target: &str,
    labels: &[String],
    ephemeral: bool,
) -> Result<()> {
    let config_path = Path::new(config_path);
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let config: FleetConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    if config.runners.contains_key(name) {
        bail!("Runner '{name}' already exists");
    }

    if !config.servers.contains_key(server) {
        bail!("Server '{server}' does not exist in fleet.toml");
    }

    if scope != "org" && scope != "repo" {
        bail!("Invalid scope '{scope}' (must be 'org' or 'repo')");
    }

    if target.is_empty() {
        bail!("Target cannot be empty");
    }

    write_runner_to_config(config_path, name, server, scope, target, labels, ephemeral)?;

    ui::success(&format!("Runner '{name}' added to fleet.toml"));
    ui::success(&format!("Run 'flow deploy {name}' to deploy"));
    Ok(())
}

async fn remove(config_path: &str, name: &str, yes: bool) -> Result<()> {
    let fleet = crate::config::load(config_path)?;
    let runner = fleet
        .runners
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Runner '{name}' not found in fleet.toml"))?;

    if !yes && !ui::confirm(&format!("Remove runner '{name}'? (y/N)")) {
        return Ok(());
    }

    let server = fleet
        .servers
        .get(&runner.server)
        .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", runner.server))?;

    let servers = std::collections::HashMap::from([(runner.server.clone(), server.clone())]);
    let sp = ui::spinner("Connecting...");
    let pool = SshPool::connect(&servers).await?;
    sp.finish_and_clear();

    let runner_dir = format!("/opt/flow/runner-{name}");
    let sp = ui::spinner(&format!("Stopping runner-{name}..."));
    let _ = pool
        .exec(
            &runner.server,
            &format!("cd {runner_dir} && docker compose down 2>/dev/null; rm -rf {runner_dir}"),
        )
        .await;
    sp.finish_and_clear();
    pool.close().await?;

    remove_runner_from_config(Path::new(config_path), name)?;
    ui::success(&format!("Runner '{name}' removed"));
    Ok(())
}

async fn list(config_path: &str) -> Result<()> {
    let fleet = crate::config::load(config_path)?;

    let gh_token = fleet.secrets.gh_token.as_deref().ok_or_else(|| {
        anyhow::anyhow!("gh_token not set in fleet.env.toml (run `flow login gh`)")
    })?;

    if fleet.runners.is_empty() {
        println!("No runners configured in fleet.toml");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec![
            Cell::new("Runner"),
            Cell::new("Server"),
            Cell::new("Scope"),
            Cell::new("Target"),
            Cell::new("Status"),
            Cell::new("Busy"),
            Cell::new("Labels"),
        ]);

    let sp = ui::spinner("Fetching runner status from GitHub...");

    for (name, runner) in &fleet.runners {
        let github_runners = fetch_github_runners(&client, gh_token, runner).await;

        let (status, busy, gh_labels) = match github_runners {
            Ok(runners) => {
                if let Some(gh) = runners.iter().find(|r| r.name == *name) {
                    let status_color = if gh.status == "online" {
                        Color::Green
                    } else {
                        Color::Red
                    };
                    let labels: Vec<&str> = gh.labels.iter().map(|l| l.name.as_str()).collect();
                    (
                        Cell::new(&gh.status).fg(status_color),
                        Cell::new(if gh.busy { "yes" } else { "" }),
                        labels.join(", "),
                    )
                } else {
                    (
                        Cell::new("not registered").fg(Color::Yellow),
                        Cell::new(""),
                        runner.labels.join(", "),
                    )
                }
            }
            Err(_) => (
                Cell::new("error").fg(Color::Red),
                Cell::new(""),
                runner.labels.join(", "),
            ),
        };

        let scope_str = match runner.scope {
            RunnerScope::Org => "org",
            RunnerScope::Repo => "repo",
        };

        table.add_row(vec![
            Cell::new(name),
            Cell::new(&runner.server),
            Cell::new(scope_str),
            Cell::new(&runner.target),
            status,
            busy,
            Cell::new(&gh_labels).set_alignment(CellAlignment::Left),
        ]);
    }

    sp.finish_and_clear();
    println!("{table}");
    Ok(())
}

#[derive(Deserialize)]
struct GitHubRunnerList {
    runners: Vec<GitHubRunner>,
}

#[derive(Deserialize)]
struct GitHubRunner {
    name: String,
    status: String,
    busy: bool,
    labels: Vec<GitHubRunnerLabel>,
}

#[derive(Deserialize)]
struct GitHubRunnerLabel {
    name: String,
}

async fn fetch_github_runners(
    client: &reqwest::Client,
    token: &str,
    runner: &Runner,
) -> Result<Vec<GitHubRunner>> {
    let url = match runner.scope {
        RunnerScope::Org => format!(
            "https://api.github.com/orgs/{}/actions/runners",
            runner.target
        ),
        RunnerScope::Repo => format!(
            "https://api.github.com/repos/{}/actions/runners",
            runner.target
        ),
    };

    let mut all_runners = Vec::new();
    let mut page = 1u32;

    loop {
        let resp = client
            .get(&url)
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .bearer_auth(token)
            .header("User-Agent", "flow-iron")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;

        if !resp.status().is_success() {
            bail!("GitHub API error: {}", resp.status());
        }

        let list: GitHubRunnerList = resp.json().await?;
        let done = list.runners.len() < 100;
        all_runners.extend(list.runners);

        if done {
            break;
        }
        page += 1;
    }

    Ok(all_runners)
}

pub fn generate_compose(name: &str, runner: &Runner) -> String {
    let mut out = String::from("services:\n");
    out.push_str("  runner:\n");
    out.push_str("    image: myoung34/github-runner:latest\n");
    out.push_str("    environment:\n");
    out.push_str("      ACCESS_TOKEN: ${ACCESS_TOKEN}\n");

    match runner.scope {
        RunnerScope::Org => {
            out.push_str("      RUNNER_SCOPE: org\n");
            out.push_str(&format!("      ORG_NAME: {}\n", runner.target));
        }
        RunnerScope::Repo => {
            out.push_str("      RUNNER_SCOPE: repo\n");
            out.push_str(&format!(
                "      REPO_URL: https://github.com/{}\n",
                runner.target
            ));
        }
    }

    out.push_str(&format!("      RUNNER_NAME: {name}\n"));

    if !runner.labels.is_empty() {
        out.push_str(&format!(
            "      RUNNER_LABELS: {}\n",
            runner.labels.join(",")
        ));
    }

    out.push_str(&format!(
        "      EPHEMERAL: \"{}\"\n",
        if runner.ephemeral { "true" } else { "false" }
    ));
    out.push_str("      DISABLE_AUTO_UPDATE: \"true\"\n");

    out.push_str("    volumes:\n");
    out.push_str("      - /var/run/docker.sock:/var/run/docker.sock\n");

    if runner.ephemeral {
        out.push_str("    restart: unless-stopped\n");
    } else {
        out.push_str("    restart: always\n");
    }

    out
}

pub fn generate_env(gh_token: &str) -> String {
    format!("ACCESS_TOKEN={gh_token}\n")
}

pub fn write_runner_to_config(
    config_path: &Path,
    name: &str,
    server: &str,
    scope: &str,
    target: &str,
    labels: &[String],
    ephemeral: bool,
) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let runners = doc
        .entry("runners")
        .or_insert_with(|| {
            let mut t = toml_edit::Table::new();
            t.set_implicit(true);
            toml_edit::Item::Table(t)
        })
        .as_table_mut()
        .context("'runners' is not a table")?;

    let mut runner_table = toml_edit::Table::new();
    runner_table.insert("server", toml_edit::value(server));
    runner_table.insert("scope", toml_edit::value(scope));
    runner_table.insert("target", toml_edit::value(target));

    if !labels.is_empty() {
        let mut labels_array = toml_edit::Array::new();
        for l in labels {
            labels_array.push(l.as_str());
        }
        runner_table.insert("labels", toml_edit::value(labels_array));
    }

    if !ephemeral {
        runner_table.insert("ephemeral", toml_edit::value(false));
    }

    runners.insert(name, toml_edit::Item::Table(runner_table));

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn remove_runner_from_config(config_path: &Path, name: &str) -> Result<()> {
    let content = std::fs::read_to_string(config_path)
        .with_context(|| format!("Failed to read {}", config_path.display()))?;
    let mut doc = content
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", config_path.display()))?;

    let runners = doc
        .get_mut("runners")
        .and_then(|s| s.as_table_mut())
        .context("'runners' table not found")?;

    runners.remove(name);

    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}
