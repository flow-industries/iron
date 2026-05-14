use anyhow::Result;
use clap::Parser;
use iron::cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.version {
        return iron::version::run(&cli.config).await;
    }

    let Some(command) = cli.command else {
        return Ok(());
    };

    match command {
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
            iron::deploy::run(&cli.config, &fleet, app.as_deref(), force, &notifier).await
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
        Command::Runner { command } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            iron::runner::run(&cli.config, command, &notifier).await
        }
        Command::Init => iron::init::run(&cli.config).await,
        Command::Server { command } => iron::server::run(&cli.config, command).await,
        Command::App { command } => iron::app::run(&cli.config, command),
        Command::Db { command } => {
            let fleet = iron::config::load(&cli.config)?;
            iron::db::run(&fleet, command).await
        }
        Command::Env { args } => iron::env::run(&cli.config, &args),
        Command::Login { command } => iron::login::run(&cli.config, command.as_ref()).await,
        Command::ObserveSync => {
            let fleet = iron::config::load(&cli.config)?;
            iron::observe_sync::run(&cli.config, &fleet).await
        }
        Command::Tail {
            app,
            server,
            level,
            stream,
            since,
            limit,
            follow,
            sql,
            streams,
            schema,
            json,
        } => {
            let fleet = iron::config::load(&cli.config)?;
            let stream_list = if stream.is_empty() {
                vec!["app_logs".to_string(), "flow_events".to_string()]
            } else {
                stream
            };
            iron::tail::run(
                &fleet,
                iron::tail::TailOpts {
                    apps: app,
                    servers: server,
                    level,
                    streams: stream_list,
                    since,
                    limit,
                    follow,
                    sql,
                    list_streams: streams,
                    schema,
                    json,
                },
            )
            .await
        }
        Command::Upgrade {
            server,
            security_only,
            full,
            reboot_if_required,
            dry_run,
            autoremove,
            yes,
        } => {
            let fleet = iron::config::load(&cli.config)?;
            let notifier = iron::notify::Notifier::from_secrets(&fleet.secrets);
            let mode = if security_only {
                iron::upgrade::UpgradeMode::SecurityOnly
            } else if full {
                iron::upgrade::UpgradeMode::Full
            } else {
                iron::upgrade::UpgradeMode::Standard
            };
            iron::upgrade::run(
                &fleet,
                iron::upgrade::UpgradeOpts {
                    server,
                    mode,
                    reboot_if_required,
                    dry_run,
                    autoremove,
                    yes,
                },
                &notifier,
            )
            .await
        }
        Command::Update { crates, git_url } => iron::update::run(crates, git_url.as_deref()).await,
        Command::Version => iron::version::run(&cli.config).await,
    }
}
