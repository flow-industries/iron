use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "flow",
    about = "Deploy and manage the Flow fleet",
    arg_required_else_help = true,
    disable_version_flag = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print version information and exit
    #[arg(short = 'V', long = "version")]
    pub version: bool,

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

        /// Keep refreshing every second
        #[arg(short = 'f', long)]
        follow: bool,

        /// Show image column
        #[arg(long)]
        image: bool,

        /// Show ports column
        #[arg(long)]
        ports: bool,

        /// Show size column
        #[arg(long)]
        size: bool,
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

    /// Manage GitHub Actions self-hosted runners
    Runner {
        #[command(subcommand)]
        command: RunnerCommand,
    },

    /// Login to external services (runs all if no subcommand given)
    Login {
        #[command(subcommand)]
        command: Option<LoginCommand>,
    },

    /// Manage app databases
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },

    /// Manage environment variables in fleet.env.toml
    Env {
        /// [app] [key=value...] — list/set env vars
        args: Vec<String>,
    },

    /// Update flow CLI to the latest version
    Update {
        /// Install from the git repository instead of crates.io
        #[arg(long)]
        git: bool,

        /// Git repository URL (implies --git, defaults to the flow-industries/iron upstream)
        #[arg(long, value_name = "URL")]
        git_url: Option<String>,
    },

    /// Print version information and the latest watcher image tag on GHCR
    Version,
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
pub enum DbCommand {
    /// Open an interactive psql shell
    Shell {
        /// App name (defaults to first app with postgres)
        app: Option<String>,

        /// Server to connect to (defaults to first server)
        #[arg(long)]
        server: Option<String>,
    },

    /// Dump the database to a local file
    Dump {
        /// App name (defaults to first app with postgres)
        app: Option<String>,

        /// Output file path (default: {app}.sql.gz)
        #[arg(short, long)]
        output: Option<String>,

        /// Server to dump from (defaults to first server)
        #[arg(long)]
        server: Option<String>,
    },

    /// Restore the database from a local SQL file
    Restore {
        /// App name (defaults to first app with postgres)
        app: Option<String>,

        /// Path to .sql or .sql.gz file
        file: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,

        /// Server to restore to (defaults to first server)
        #[arg(long)]
        server: Option<String>,
    },

    /// List available backups on the server
    List {
        /// App name (defaults to first app with postgres)
        app: Option<String>,

        /// Server to list backups from (defaults to first server)
        #[arg(long)]
        server: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RunnerCommand {
    /// Add a self-hosted runner (interactive wizard if no args given)
    Add {
        /// Runner name (used as identifier in fleet.toml)
        name: Option<String>,

        /// Server to deploy to (must exist in fleet.toml)
        #[arg(long)]
        server: Option<String>,

        /// Runner scope: org or repo
        #[arg(long)]
        scope: Option<String>,

        /// Target org name or owner/repo
        #[arg(long)]
        target: Option<String>,

        /// Runner label(s) (repeatable)
        #[arg(long)]
        label: Vec<String>,

        /// Single-job ephemeral mode (default: true)
        #[arg(long)]
        ephemeral: bool,
    },

    /// Remove a runner from fleet.toml and clean up on server
    Remove {
        /// Runner name to remove
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// List runners and their status from GitHub API
    List,
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

        /// Domain hostname(s) for Caddy reverse proxy (repeatable)
        #[arg(long)]
        domain: Vec<String>,

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
