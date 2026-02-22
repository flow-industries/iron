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

| Repo | Domain | Description |
|------|--------|-------------|
| flow-site | flow.industries | Marketing/landing website |
| flow-auth | id.flow.industries | Authentication + identity |
| flow-game | flow.game | Multiplayer game |
| flow-talk | flow.talk | Decentralized social platform |

## Deploy Flow

```
Service repos push code → CI builds image → pushes to GHCR
Watchtower on servers detects new image → pulls → recreates container
Infrastructure changes → edit this repo → run ansible-playbook
```

## Adding a Server

1. Provision server, ensure hostname resolves
2. Add to `ansible/inventory.yml` under the right group
3. Run `ansible-playbook ansible/main.yml`

## Adding a Service

1. Create service repo with Dockerfile + CI pushing to GHCR
2. Add image to the appropriate `compose/` file
3. Add domain routing to inventory + update Caddyfile template
4. Run `ansible-playbook ansible/deploy.yml`

## Useful Commands

```bash
ansible all -a "docker ps"                    # What's running
ansible all -a "df -h"                        # Disk space
ansible web -a "docker logs flow-auth --tail 50"  # Service logs
ansible-playbook deploy.yml                   # Re-deploy only
ansible-playbook dns.yml                      # DNS only
```
