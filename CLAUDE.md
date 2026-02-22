# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Repo Is

Infrastructure-as-code for the Flow ecosystem. Ansible deploys Docker Compose stacks to servers. WUD (What's Up Docker) detects new images on GHCR and triggers docker-rollout for zero-downtime deploys. Caddy handles reverse proxy, HTTPS, and health-checked load balancing. This repo never contains Dockerfiles — service repos own those.

## Commands

```bash
# Install Ansible Galaxy dependencies (required before running playbooks)
ansible-galaxy install -r ansible/requirements.yml

# Run everything: server setup + deploy stacks + DNS
ansible-playbook ansible/main.yml

# Individual playbooks
ansible-playbook ansible/setup.yml     # OS hardening, Docker, deploy user
ansible-playbook ansible/deploy.yml    # Deploy compose stacks per host
ansible-playbook ansible/dns.yml       # Cloudflare DNS from inventory

# Dry run
ansible-playbook ansible/main.yml --check --diff

# Syntax check
ansible-playbook --syntax-check ansible/deploy.yml

# Lint
ansible-lint ansible/

# Vault
ansible-vault create ansible/group_vars/secrets.yml
ansible-vault edit ansible/group_vars/secrets.yml
```

## Architecture

**Inventory drives everything.** Each host declares `stacks` (what to deploy) and `domains` (what Caddy routes). DNS records are auto-generated from inventory. Adding a server or moving a service is an inventory change + playbook run.

**Playbook order:** `setup.yml` (bootstrap, runs once) → `deploy.yml` (stacks) → `dns.yml` (Cloudflare). Orchestrated by `main.yml`.

**Docker networking:** All HTTP services and Caddy join a shared `flow` network. Caddy reaches services by Docker DNS name (e.g., `site:3000`). Game servers use direct UDP — no Caddy.

**deploy.yml is a single play on `hosts: all`.** It uses `when: "'<stack>' in stacks"` conditions to deploy only what each host needs. Caddy and WUD always deploy.

**Templates vs static files:** Most compose files are static (`copy`). Jinja2 templates: `stacks/caddy/Caddyfile.j2` (from domains), `stacks/auth/.env.j2` (vault secrets), `stacks/wud/.env.j2` (GHCR token), `stacks/game/docker-compose.yml.j2` (per-host game config).

## Conventions

- Compose files use Docker's `${VAR}` syntax for secrets, not Jinja2. Ansible templates `.env` files that Docker Compose reads.
- Every `file`, `template`, and `copy` task must have an explicit `mode` (ansible-lint enforces this).
- Handler names start with uppercase (e.g., `Restart auth`).
- All `import_playbook` entries must have a `name` field.
- Use `ansible.builtin.*` fully qualified module names.
- WUD only watches containers with the `wud.watch=true` label. Database containers must have `wud.watch=false`.
- HTTP services use `wud.trigger.include=rollout` for zero-downtime deploys via docker-rollout. Game server uses `wud.trigger.include=gameupdate` for stop-and-replace (UDP port conflict prevents scaling).

## Secrets

`ansible/group_vars/secrets.yml` is ansible-vault encrypted. Required vars: `db_password`, `better_auth_secret`, `ghcr_token`, `cloudflare_api_token`. The placeholder file has creation instructions.
