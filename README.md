# iron

Infrastructure-as-code for the Flow ecosystem. Single source of truth for all server configuration, deployment, and networking.

## Stack

- **Config management:** Ansible (push-based, SSH)
- **Containers:** Docker Compose
- **Auto-deploy:** Watchtower (polls GHCR every 5s)
- **Reverse proxy:** Caddy (automatic HTTPS)
- **Secrets:** ansible-vault
- **Hardening:** devsec.hardening (os + ssh)

## Prerequisites

- Ansible 2.16+
- `ansible-galaxy install -r ansible/requirements.yml`
- SSH agent with key for target servers

## Quick Start

```bash
# Install dependencies
ansible-galaxy install -r ansible/requirements.yml

# Create vault secrets
ansible-vault create ansible/group_vars/secrets.yml

# Run everything (setup + deploy + DNS)
ansible-playbook ansible/main.yml

# Or run individual playbooks
ansible-playbook ansible/setup.yml    # OS hardening, Docker, deploy user
ansible-playbook ansible/deploy.yml   # Deploy compose stacks
ansible-playbook ansible/dns.yml      # Cloudflare DNS records

# Dry run
ansible-playbook ansible/main.yml --check --diff
```

## Services

| Stack | Domain | Description |
|-------|--------|-------------|
| site | flow.industries | Marketing/landing website |
| auth | id.flow.industries | Authentication + identity + database |
| talk | flow.talk | Decentralized social platform |
| game | flow.game | Multiplayer game server |

## Repository Structure

```
stacks/
  site/            flow-site container
  auth/            flow-auth + postgres + backup
  talk/            flow-talk container
  game/            flow-game server (templated per host)
  caddy/           shared reverse proxy (one per server)
  watchtower/      auto-deploy (one per server)
ansible/
  inventory.yml    hosts + which stacks they run
  setup.yml        OS hardening, Docker, deploy user
  deploy.yml       deploy stacks per host
  dns.yml          Cloudflare DNS from inventory
  main.yml         runs all above in order
```

## Deploy Flow

```
Service repos push code → CI builds image → pushes to GHCR
Watchtower on servers detects new image → pulls → recreates container
Infrastructure changes → edit this repo → run ansible-playbook
```

## Adding a Server

1. Provision server, ensure hostname resolves
2. Add to `ansible/inventory.yml` with its `stacks` list and `domains`
3. Run `ansible-playbook ansible/main.yml`

## Moving a Service

1. Add the stack name to the new host's `stacks` list
2. Move its `domains` entry to the new host
3. Run `ansible-playbook ansible/main.yml`

## Adding a Service

1. Create service repo with Dockerfile + CI pushing to GHCR
2. Create `stacks/<name>/docker-compose.yml`
3. Add to a host's `stacks` list in inventory
4. Add domain routing to the host's `domains` list
5. Run `ansible-playbook ansible/deploy.yml`

## Useful Commands

```bash
ansible all -a "docker ps"                         # What's running
ansible all -a "df -h"                              # Disk space
ansible flow-1 -a "docker logs auth --tail 50"      # Service logs
ansible-playbook ansible/deploy.yml                 # Re-deploy only
ansible-playbook ansible/dns.yml                    # DNS only
```
