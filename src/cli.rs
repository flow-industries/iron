use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "flow", about = "Deploy and manage the Flow fleet")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Path to fleet.toml (default: fleet.toml in current directory)
    #[arg(long, global = true, default_value = "fleet.toml")]
    pub config: String,
}

#[derive(Subcommand)]
pub enum Command {
    /// Deploy an app (or all apps if no name given)
    Deploy {
        /// App name to deploy (deploys all if omitted)
        app: Option<String>,

        /// Force recreate containers instead of rolling deploy
        #[arg(long)]
        force: bool,
    },

    /// Verify fleet.toml matches reality on servers
    Check {
        /// Filter by server name
        #[arg(long)]
        server: Option<String>,

        /// Re-run Ansible hardening playbook
        #[arg(long)]
        with_hardening: bool,
    },

    /// Show fleet-wide status and container info
    Status {
        /// Filter by server name
        #[arg(long)]
        server: Option<String>,
    },

    /// Tail logs from an app
    Logs {
        /// App name
        app: String,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,

        /// Server to tail logs from (defaults to first server)
        #[arg(long)]
        server: Option<String>,
    },

    /// Stop an app's containers (keeps config, files, and DNS intact)
    Stop {
        /// App name to stop
        app: String,

        /// Stop only on this server (defaults to all assigned servers)
        #[arg(long)]
        server: Option<String>,
    },

    /// Restart an app's containers without redeploying
    Restart {
        /// App name to restart
        app: String,

        /// Restart only on this server (defaults to all assigned servers)
        #[arg(long)]
        server: Option<String>,
    },

    /// Remove an app: stop containers, clean up files, DNS, and fleet.toml
    Remove {
        /// App name to remove
        app: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// Initialize a new fleet.toml in the current directory
    Init,

    /// Manage servers
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },

    /// Manage apps in fleet.toml
    App {
        #[command(subcommand)]
        command: AppCommand,
    },

    /// Login to external services (runs all if no subcommand given)
    Login {
        #[command(subcommand)]
        command: Option<LoginCommand>,
    },

    /// Manage environment variables in fleet.env.toml
    Env {
        /// [app] [key=value...] — list/set env vars
        args: Vec<String>,
    },

    /// Update flow CLI to the latest version
    Update,
}

#[derive(Subcommand)]
pub enum LoginCommand {
    /// Set Cloudflare API token
    Cf,

    /// Set GitHub Container Registry token
    Gh,
}

#[derive(Subcommand)]
pub enum ServerCommand {
    /// Add a new server and run Ansible to bootstrap it (interactive wizard if no args given)
    Add {
        /// Server name (used as identifier in fleet.toml)
        name: Option<String>,

        /// Server IP address
        #[arg(long)]
        ip: Option<String>,

        /// Override hostname (default: {name}.{domain})
        #[arg(long)]
        host: Option<String>,

        /// Deploy user (created by Ansible, used for future SSH)
        #[arg(long)]
        user: Option<String>,

        /// SSH user for initial Ansible connection
        #[arg(long)]
        ssh_user: Option<String>,

        /// Path to SSH public key for the deploy user
        #[arg(long)]
        ssh_key: Option<String>,
    },

    /// Remove a server from fleet.toml
    Remove {
        /// Server name to remove
        name: String,
    },
}

#[derive(Subcommand)]
pub enum AppCommand {
    /// Add a new app to fleet.toml (interactive wizard if no args given)
    Add {
        /// App name (used as identifier in fleet.toml)
        name: Option<String>,

        /// Docker image (e.g., ghcr.io/org/app:latest)
        #[arg(long)]
        image: Option<String>,

        /// Server(s) to deploy to (must exist in fleet.toml, repeatable)
        #[arg(long)]
        server: Vec<String>,

        /// Container port (required if routing is used)
        #[arg(long)]
        port: Option<u16>,

        /// Route hostname(s) for Caddy reverse proxy (repeatable)
        #[arg(long)]
        route: Vec<String>,

        /// Health check path (e.g., /health)
        #[arg(long)]
        health_path: Option<String>,

        /// Health check interval (e.g., 5s, 1m)
        #[arg(long)]
        health_interval: Option<String>,

        /// Direct port mapping(s) in external:internal[/protocol] format (repeatable)
        #[arg(long, value_name = "EXTERNAL:INTERNAL[/PROTOCOL]")]
        port_map: Vec<String>,

        /// Deploy strategy: rolling (default) or recreate
        #[arg(long)]
        deploy_strategy: Option<String>,
    },

    /// Add a sidecar service to an existing app
    AddService {
        /// App name (must exist in fleet.toml)
        app: String,

        /// Service name
        name: String,

        /// Docker image for the service
        #[arg(long)]
        image: String,

        /// Volume mount(s) in name:path format (repeatable)
        #[arg(long)]
        volume: Vec<String>,

        /// Healthcheck command
        #[arg(long)]
        healthcheck: Option<String>,

        /// Service this depends on (must exist in same app)
        #[arg(long)]
        depends_on: Option<String>,
    },

    /// Remove a sidecar service from an app
    RemoveService {
        /// App name
        app: String,

        /// Service name to remove
        name: String,
    },
}
