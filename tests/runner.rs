use std::fs;

use tempfile::TempDir;

fn base_config() -> &'static str {
    "[servers.fl-1]\nhost = \"fl-1.example.com\"\n"
}

#[test]
fn parse_runner_config_org() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[runners.ci]\nserver = \"fl-1\"\nscope = \"org\"\ntarget = \"flow-industries\"\nlabels = [\"linux\", \"x64\"]\n",
            base_config()
        ),
    )
    .unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    assert_eq!(fleet.runners.len(), 1);
    let runner = &fleet.runners["ci"];
    assert_eq!(runner.server, "fl-1");
    assert_eq!(runner.scope, iron::config::RunnerScope::Org);
    assert_eq!(runner.target, "flow-industries");
    assert_eq!(runner.labels, vec!["linux", "x64"]);
    assert!(runner.ephemeral);
}

#[test]
fn parse_runner_config_repo() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[runners.build]\nserver = \"fl-1\"\nscope = \"repo\"\ntarget = \"org/repo\"\nephemeral = false\n",
            base_config()
        ),
    )
    .unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    let runner = &fleet.runners["build"];
    assert_eq!(runner.scope, iron::config::RunnerScope::Repo);
    assert_eq!(runner.target, "org/repo");
    assert!(!runner.ephemeral);
}

#[test]
fn validate_runner_unknown_server() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[runners.ci]\nserver = \"nonexistent\"\nscope = \"org\"\ntarget = \"org\"\n",
            base_config()
        ),
    )
    .unwrap();

    let err = iron::config::load(path.to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("unknown server"));
}

#[test]
fn validate_runner_empty_target() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[runners.ci]\nserver = \"fl-1\"\nscope = \"org\"\ntarget = \"\"\n",
            base_config()
        ),
    )
    .unwrap();

    let err = iron::config::load(path.to_str().unwrap()).unwrap_err();
    assert!(err.to_string().contains("empty target"));
}

#[test]
fn add_writes_runner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(&path, base_config()).unwrap();

    iron::runner::write_runner_to_config(
        &path,
        "ci",
        "fl-1",
        "org",
        "flow-industries",
        &["linux".to_string(), "x64".to_string()],
        true,
    )
    .unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    assert_eq!(fleet.runners.len(), 1);
    let runner = &fleet.runners["ci"];
    assert_eq!(runner.server, "fl-1");
    assert_eq!(runner.scope, iron::config::RunnerScope::Org);
    assert_eq!(runner.target, "flow-industries");
    assert_eq!(runner.labels, vec!["linux", "x64"]);
    assert!(runner.ephemeral);
}

#[test]
fn add_preserves_existing_content() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[apps.site]\nimage = \"nginx:latest\"\nservers = [\"fl-1\"]\n",
            base_config()
        ),
    )
    .unwrap();

    iron::runner::write_runner_to_config(&path, "ci", "fl-1", "org", "myorg", &[], true).unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    assert!(fleet.apps.contains_key("site"));
    assert!(fleet.runners.contains_key("ci"));
}

#[test]
fn add_non_ephemeral_writes_field() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(&path, base_config()).unwrap();

    iron::runner::write_runner_to_config(&path, "ci", "fl-1", "org", "myorg", &[], false).unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    assert!(!fleet.runners["ci"].ephemeral);
}

#[test]
fn remove_deletes_runner() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("fleet.toml");
    fs::write(
        &path,
        format!(
            "{}\n[runners.ci]\nserver = \"fl-1\"\nscope = \"org\"\ntarget = \"myorg\"\n",
            base_config()
        ),
    )
    .unwrap();

    iron::runner::remove_runner_from_config(&path, "ci").unwrap();

    let fleet = iron::config::load(path.to_str().unwrap()).unwrap();
    assert!(fleet.runners.is_empty());
}

#[test]
fn generate_compose_org() {
    let runner = iron::config::Runner {
        server: "fl-1".to_string(),
        scope: iron::config::RunnerScope::Org,
        target: "flow-industries".to_string(),
        labels: vec!["linux".to_string(), "x64".to_string()],
        ephemeral: true,
    };
    let yaml = iron::runner::generate_compose("ci", &runner);
    assert!(yaml.contains("RUNNER_SCOPE: org"));
    assert!(yaml.contains("ORG_NAME: flow-industries"));
    assert!(yaml.contains("RUNNER_NAME: ci"));
    assert!(yaml.contains("RUNNER_LABELS: linux,x64"));
    assert!(yaml.contains("EPHEMERAL: \"true\""));
    assert!(yaml.contains("restart: unless-stopped"));
    assert!(yaml.contains("/var/run/docker.sock"));
}

#[test]
fn generate_compose_repo() {
    let runner = iron::config::Runner {
        server: "fl-1".to_string(),
        scope: iron::config::RunnerScope::Repo,
        target: "org/repo".to_string(),
        labels: vec![],
        ephemeral: false,
    };
    let yaml = iron::runner::generate_compose("build", &runner);
    assert!(yaml.contains("RUNNER_SCOPE: repo"));
    assert!(yaml.contains("REPO_URL: https://github.com/org/repo"));
    assert!(yaml.contains("EPHEMERAL: \"false\""));
    assert!(yaml.contains("restart: always"));
    assert!(!yaml.contains("RUNNER_LABELS"));
}

#[test]
fn generate_env_contains_token() {
    let env = iron::runner::generate_env("ghp_abc123");
    assert_eq!(env, "ACCESS_TOKEN=ghp_abc123\n");
}
