use anyhow::Result;
use clap::Parser;
use flow::cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Deploy { app } => {
            let fleet = flow::config::load(&cli.config)?;
            flow::deploy::run(&fleet, app.as_deref()).await
        }
        Command::Status { server } => {
            let fleet = flow::config::load(&cli.config)?;
            flow::status::run(&fleet, server.as_deref()).await
        }
        Command::Logs { app, follow, server } => {
            let fleet = flow::config::load(&cli.config)?;
            flow::logs::run(&fleet, &app, follow, server.as_deref()).await
        }
        Command::Server { command } => flow::server::run(&cli.config, command).await,
    }
}
