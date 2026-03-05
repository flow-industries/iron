# iron

Infrastructure-as-code for the Flow ecosystem. The `flow` CLI reads `fleet.toml` and handles deployment, DNS, reverse proxy, and server bootstrapping.

## Stack

- **CLI:** Rust (`flow`)
- **Config:** `fleet.toml` (servers, apps, routing) + `fleet.env.toml` (secrets, gitignored)
- **Containers:** Docker Compose
- **Auto-deploy:** WUD (What's Up Docker) + docker-rollout for zero-downtime
- **Reverse proxy:** Caddy (automatic HTTPS)
- **DNS:** Cloudflare API
- **Server setup:** Ansible (hardening, Docker, firewall, fail2ban)

## Prerequisites

- Rust toolchain
- SSH agent with key for target servers
- `fleet.env.toml` with `cloudflare_api_token` and `ghcr_token`

## Quick Start

```bash
cargo build --release

# Deploy everything
flow deploy

# Deploy a single app
flow deploy site

# Fleet status
flow status
flow status --server flow-1

# Tail logs
flow logs site
flow logs site -f
flow logs site --server flow-1

# Add a new server (creates DNS, bootstraps via Ansible)
flow server add fl-2 --ip 164.90.130.5
flow server add fl-2 --ip 164.90.130.5 --host custom.flow.industries
flow server add fl-2 --ip 164.90.130.5 --ssh-user ubuntu

# Remove a server
flow server remove fl-2

# Check server health
flow server check
flow server check flow-1
```

## Services

| App | Domain | Server | Strategy |
|-----|--------|--------|----------|
| site | flow.industries | flow-1 | rolling |
| auth | id.flow.industries | flow-1 | rolling |
| talk | flow.talk | flow-1 | rolling |
| game-web | flow.game | game-1 | rolling |
| game-server | *(direct TCP :9999)* | game-1 | recreate |

## How Deploy Works

```
flow deploy <app>
  → generate docker-compose.yml + .env from fleet.toml/fleet.env.toml
  → SSH to target server
  → upload files to /opt/flow/<app>/
  → docker compose pull + docker rollout (or recreate)
  → generate Caddy fragment, reload Caddy
  → ensure Cloudflare DNS A record
```

## Auto-Deploy

```
Service repo pushes code → CI builds image → pushes to GHCR
WUD detects new image → docker compose pull → docker rollout (zero-downtime)
```

## Adding a Service

1. Create service repo with Dockerfile + CI pushing to GHCR
2. Add `[apps.<name>]` to `fleet.toml` with image, server, port, routing
3. Add env vars to `fleet.env.toml` if needed
4. Run `flow deploy <name>`

## Project Structure

```
src/
  main.rs         CLI entrypoint
  lib.rs          public module re-exports
  cli.rs          clap command definitions
  config.rs       fleet.toml parsing and validation
  compose.rs      docker-compose.yml generation
  caddy.rs        Caddy reverse proxy fragments
  cloudflare.rs   DNS A record management
  deploy.rs       full deploy pipeline
  server.rs       server add/remove/check
  ssh.rs          SSH connection pool
  status.rs       fleet status display
  logs.rs         log tailing
  ui.rs           terminal output helpers
tests/
  config.rs       config parsing and validation
  compose.rs      compose generation
  caddy.rs        Caddy fragment generation
  cloudflare.rs   Cloudflare API
  server.rs       server management
ansible/
  setup.yml       server bootstrapping playbook
  ansible.cfg     Ansible configuration
  requirements.yml  Galaxy role dependencies
  group_vars/     host group variables
stacks/
  caddy/
    docker-compose.yml  shared Caddy reverse proxy
  wud/
    docker-compose.yml  WUD auto-deploy watcher
    rollout.sh          zero-downtime deploy script
fleet.toml        server and app definitions
fleet.env.toml    secrets and env vars (gitignored)
```
