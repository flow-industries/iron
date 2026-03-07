# iron

A CLI that deploys Docker Compose apps to bare-metal servers with automatic HTTPS, DNS, and zero-downtime updates.

## Features

- **Single config file** — define servers, apps, routing, and sidecars in `fleet.toml`
- **Docker Compose** — generates compose files and deploys via SSH
- **Caddy reverse proxy** — automatic HTTPS with generated per-app config fragments
- **Cloudflare DNS** — creates and manages A records automatically
- **Zero-downtime deploys** — rolling updates via docker-rollout, or stop-and-replace for stateful services
- **Server bootstrapping** — provisions new machines with Ansible (Docker, firewall, fail2ban, hardening)
- **Auto-deploy** — optional WUD (What's Up Docker) integration watches for new images and triggers deploys
- **Sidecar services** — attach databases, caches, or any container alongside your app
- **Direct TCP** — expose non-HTTP services (game servers, databases) with port mappings
- **Secrets management** — env vars live in `fleet.env.toml` (gitignored), deployed as `.env` files

## Installation

```bash
cargo install flow-iron
```

This installs two binaries — `flow` and `iron` — which are identical. Use whichever you prefer.

To build from source:

```bash
git clone https://github.com/flow-industries/iron.git
cd iron
cargo install --path .
```

## Prerequisites

- SSH agent with key for target servers
- Cloudflare API token (for DNS management)
- GHCR token (for pulling private images)

## Quick Start

```bash
# Initialize a new fleet.toml (prompts for Cloudflare token)
flow init

# Or add/update Cloudflare token separately
flow login cf

# Add a server (creates DNS record, bootstraps via Ansible)
flow server add srv-1 --ip 164.90.130.5

# Add an app with routing
flow app add site --image ghcr.io/org/site:latest --server srv-1 --port 3000 \
    --route example.com --health-path /health

# Add a worker (no routing needed)
flow app add worker --image ghcr.io/org/worker:latest --server srv-1

# Add a game server with direct TCP
flow app add game --image ghcr.io/org/game:latest --server srv-1 \
    --deploy-strategy recreate --port-map 9999:9999/tcp

# Add sidecar services
flow app add-service site postgres --image postgres:17 \
    --volume pgdata:/var/lib/postgresql/data --healthcheck "pg_isready -U app"

# Deploy
flow deploy         # deploy all apps
flow deploy site    # deploy a single app
```

## Commands

```bash
flow init                        # create fleet.toml
flow deploy [app]                # deploy all or one app
flow status [--server srv-1]     # fleet status and container info
flow check [--server srv-1]      # verify fleet.toml matches servers
flow stop <app> [--server srv-1] # stop an app
flow restart <app>               # restart containers
flow remove <app> [--yes]        # tear down app, remove from fleet.toml
flow logs <app> [-f]             # tail logs

flow app add <app> ...           # add app to fleet.toml
flow app add-service <app> ...   # add sidecar service
flow app remove-service <app> .. # remove sidecar service

flow server add <name> --ip ..   # add and bootstrap a server
flow server remove <name>        # remove a server
flow server check [name]         # check server health

flow login cf                    # set/update Cloudflare API token
```

## How It Works

```
flow deploy <app>
  1. Parse fleet.toml + fleet.env.toml
  2. Generate docker-compose.yml and .env
  3. SSH to target server
  4. Upload files to /opt/flow/<app>/
  5. Pull images, rolling deploy (or recreate)
  6. Generate Caddy config fragment, reload Caddy
  7. Ensure Cloudflare DNS A record
```

## Configuration

`fleet.toml` is the single source of truth for your infrastructure. `fleet.env.toml` (gitignored) holds all environment variables and secrets.

```toml
[fleet]
domain = "example.com"
cloudflare_zone_id = "your-zone-id"

[servers.srv-1]
ip = "164.90.130.5"

[apps.site]
image = "ghcr.io/org/site:latest"
servers = ["srv-1"]
port = 3000

[apps.site.routing]
routes = ["example.com", "www.example.com"]
health_path = "/health"

[[apps.site.services]]
name = "postgres"
image = "postgres:17"
volumes = ["pgdata:/var/lib/postgresql/data"]
healthcheck = "pg_isready -U app"
```

## License

MIT
