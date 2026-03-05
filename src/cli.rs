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
    },

    /// Manage servers
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
}

#[derive(Subcommand)]
pub enum ServerCommand {
    /// Add a new server and run Ansible to bootstrap it
    Add {
        /// Server name (used as identifier in fleet.toml)
        name: String,

        /// Server IP address
        #[arg(long)]
        ip: String,

        /// Override hostname (default: {name}.{domain})
        #[arg(long)]
        host: Option<String>,

        /// Deploy user (created by Ansible, used for future SSH)
        #[arg(long, default_value = "deploy")]
        user: String,

        /// SSH user for initial Ansible connection
        #[arg(long, default_value = "root")]
        ssh_user: String,
    },

    /// Remove a server from fleet.toml
    Remove {
        /// Server name to remove
        name: String,
    },

    /// Verify a server is properly set up
    Check {
        /// Server name (checks all if omitted)
        name: Option<String>,
    },
}
