use anyhow::Result;
use clap::Parser;
use iron::cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check {
            server,
            with_hardening,
        } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::check::run(&fleet, server.as_deref(), &notifier).await?;
            if with_hardening {
                iron::server::run_hardening(&cli.config, server.as_deref()).await?;
            }
            Ok(())
        }
        Command::Deploy { app, force } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::deploy::run(&fleet, app.as_deref(), force, &notifier).await
        }
        Command::Status {
            server,
            follow,
            image,
            ports,
            size,
        } => {
            let fleet = iron::config::load(&cli.config)?;
            let cols = iron::status::Columns { image, ports, size };
            iron::status::run(&fleet, server.as_deref(), follow, cols).await
        }
        Command::Logs {
            app,
            follow,
            server,
        } => {
            let fleet = iron::config::load(&cli.config)?;
            iron::logs::run(&fleet, &app, follow, server.as_deref()).await
        }
        Command::Stop { app, server } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::stop::run(&fleet, &app, server.as_deref(), &notifier).await
        }
        Command::Restart { app, server } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::restart::run(&fleet, &app, server.as_deref(), &notifier).await
        }
        Command::Remove { app, yes } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::remove::run(&cli.config, &app, yes, &notifier).await
        }
        Command::Runner { command } => iron::runner::run(&cli.config, command).await,
        Command::Init => iron::init::run(&cli.config).await,
        Command::Server { command } => iron::server::run(&cli.config, command).await,
        Command::App { command } => iron::app::run(&cli.config, command),
        Command::Db { command } => {
            let fleet = iron::config::load(&cli.config)?;
            iron::db::run(&fleet, command).await
        }
        Command::Env { args } => iron::env::run(&cli.config, &args),
        Command::Login { command } => iron::login::run(&cli.config, command.as_ref()).await,
        Command::Update => iron::update::run().await,
    }
}
