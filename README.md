# iron

A CLI that deploys Docker Compose apps to bare-metal servers with automatic HTTPS, DNS, and zero-downtime updates.

## Installation

```bash
cargo install flow-iron
```

Installs two identical binaries — `flow` and `iron`. Use whichever you prefer.

Build from source:

```bash
git clone https://github.com/flow-industries/iron.git
cd iron
cargo install --path .
```

## Documentation

Full documentation at **[flow.industries/en/iron](https://flow.industries/en/iron)**.

- [Getting started](https://flow.industries/en/iron/getting-started) — install, init, first deploy
- [Concepts](https://flow.industries/en/iron/concepts) — fleet, servers, apps, sidecars, routing, watcher
- [Configuration](https://flow.industries/en/iron/configuration) — `fleet.toml` and `fleet.env.toml` reference
- [Deploy pipeline](https://flow.industries/en/iron/deploy) — what `flow deploy` does end-to-end
- [Servers](https://flow.industries/en/iron/servers), [Apps](https://flow.industries/en/iron/apps), [Env](https://flow.industries/en/iron/env), [Databases](https://flow.industries/en/iron/databases), [Runners](https://flow.industries/en/iron/runners)
- [Lifecycle](https://flow.industries/en/iron/lifecycle), [Status](https://flow.industries/en/iron/status), [Check](https://flow.industries/en/iron/check), [Logs](https://flow.industries/en/iron/logs), [Login](https://flow.industries/en/iron/login)

## License

MIT
