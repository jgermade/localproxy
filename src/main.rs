mod app;
mod config;
mod control;
mod gateway;
mod proxy;
mod service;
mod stream;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::Confirm;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Local proxy daemon with dynamic upstream resolution"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Daemon,
    Config,
    Status,
    Stop,
    Reload,
    Start {
        #[arg(long)]
        detached: bool,
    },
    Logs {
        #[arg(long, default_value_t = 100)]
        lines: usize,
        #[arg(long)]
        follow: bool,
        #[arg(long)]
        detached: bool,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    Paths,
}

#[derive(Debug, Clone, Subcommand)]
enum ServiceCommand {
    Install,
    Start,
    Restart,
    Status,
    Stop,
    Logs {
        #[arg(long, default_value_t = 100)]
        lines: usize,
        #[arg(long)]
        follow: bool,
    },
    Uninstall,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,zproxy=debug".into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let paths = config::AppPaths::discover()?;

    match cli.command.unwrap_or(Command::Daemon) {
        Command::Daemon => app::run_daemon(paths).await,
        Command::Config => run_config(paths).await,
        Command::Status => {
            run_control(paths.control_socket(), control::ControlCommand::Status).await
        }
        Command::Stop => run_control(paths.control_socket(), control::ControlCommand::Stop).await,
        Command::Reload => {
            run_control(paths.control_socket(), control::ControlCommand::Reload).await
        }
        Command::Start { detached } => run_start(paths, detached),
        Command::Logs {
            lines,
            follow,
            detached,
        } => run_logs(paths, lines, follow, detached),
        Command::Service { command } => run_service(paths, command),
        Command::Paths => {
            println!("config: {}", paths.config_file.display());
            println!("state: {}", paths.state_dir.display());
            println!("socket: {}", paths.control_socket().display());
            println!("pid: {}", paths.pid_file().display());
            Ok(())
        }
    }
}

async fn run_config(paths: config::AppPaths) -> Result<()> {
    let current = config::load_or_create(&paths)?;
    let updated = config::run_wizard(current)?;
    config::save(&paths, &updated)?;

    match control::send_command(paths.control_socket(), control::ControlCommand::Reload).await {
        Ok(response) => println!("{response}"),
        Err(error) => println!("config saved; daemon not notified: {error}"),
    }

    Ok(())
}

async fn run_control(socket_path: PathBuf, command: control::ControlCommand) -> Result<()> {
    let response = control::send_command(socket_path, command).await?;
    println!("{response}");
    Ok(())
}

fn run_start(paths: config::AppPaths, detached: bool) -> Result<()> {
    if detached {
        let pid = app::start_detached(&paths)?;
        println!("daemon started in background with pid {pid}");
        return Ok(());
    }

    if service::is_installed(&paths)? {
        service::start(&paths)?;
        println!("service started");
        return Ok(());
    }

    let confirm = Confirm::new()
        .with_prompt("No service installed. Do you want to run start --detached?")
        .default(true)
        .interact()?;

    if confirm {
        let pid = app::start_detached(&paths)?;
        println!("daemon started in background with pid {pid}");
    } else {
        println!("cancelled");
    }

    Ok(())
}

fn run_logs(paths: config::AppPaths, lines: usize, follow: bool, detached: bool) -> Result<()> {
    if !detached && service::is_installed(&paths)? {
        return service::logs(&paths, lines, follow);
    }

    service::tail_file(paths.log_file().as_path(), lines, follow)
}

fn run_service(paths: config::AppPaths, command: ServiceCommand) -> Result<()> {
    match command {
        ServiceCommand::Install => service::install(&paths),
        ServiceCommand::Start => {
            service::start(&paths)?;
            println!("service started");
            Ok(())
        }
        ServiceCommand::Restart => {
            service::restart(&paths)?;
            println!("service restarted");
            Ok(())
        }
        ServiceCommand::Status => {
            let status = service::status(&paths)?;
            println!("{status}");
            Ok(())
        }
        ServiceCommand::Stop => {
            service::stop(&paths)?;
            println!("service stopped");
            Ok(())
        }
        ServiceCommand::Logs { lines, follow } => service::logs(&paths, lines, follow),
        ServiceCommand::Uninstall => service::uninstall(&paths),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn the_cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_falls_back_to_the_daemon() {
        assert!(parse(&["zproxy"]).command.is_none());
    }

    #[test]
    fn plain_subcommands_are_parsed() {
        assert!(matches!(
            parse(&["zproxy", "daemon"]).command,
            Some(Command::Daemon)
        ));
        assert!(matches!(
            parse(&["zproxy", "config"]).command,
            Some(Command::Config)
        ));
        assert!(matches!(
            parse(&["zproxy", "status"]).command,
            Some(Command::Status)
        ));
        assert!(matches!(
            parse(&["zproxy", "reload"]).command,
            Some(Command::Reload)
        ));
        assert!(matches!(
            parse(&["zproxy", "stop"]).command,
            Some(Command::Stop)
        ));
        assert!(matches!(
            parse(&["zproxy", "paths"]).command,
            Some(Command::Paths)
        ));
    }

    #[test]
    fn start_is_attached_unless_detached_is_requested() {
        assert!(matches!(
            parse(&["zproxy", "start"]).command,
            Some(Command::Start { detached: false })
        ));
        assert!(matches!(
            parse(&["zproxy", "start", "--detached"]).command,
            Some(Command::Start { detached: true })
        ));
    }

    #[test]
    fn logs_defaults_to_one_hundred_lines_without_following() {
        assert!(matches!(
            parse(&["zproxy", "logs"]).command,
            Some(Command::Logs {
                lines: 100,
                follow: false,
                detached: false,
            })
        ));
        assert!(matches!(
            parse(&["zproxy", "logs", "--lines", "5", "--follow", "--detached"]).command,
            Some(Command::Logs {
                lines: 5,
                follow: true,
                detached: true,
            })
        ));
    }

    #[test]
    fn service_subcommands_are_parsed() {
        for (arg, expected) in [
            ("install", ServiceCommand::Install),
            ("start", ServiceCommand::Start),
            ("restart", ServiceCommand::Restart),
            ("status", ServiceCommand::Status),
            ("stop", ServiceCommand::Stop),
            ("uninstall", ServiceCommand::Uninstall),
        ] {
            let parsed = parse(&["zproxy", "service", arg]).command;
            let Some(Command::Service { command }) = parsed else {
                panic!("se esperaba un subcomando de service para {arg}");
            };
            assert_eq!(
                std::mem::discriminant(&command),
                std::mem::discriminant(&expected)
            );
        }
    }

    #[test]
    fn service_logs_accepts_the_same_flags_as_logs() {
        let parsed = parse(&["zproxy", "service", "logs", "--lines", "20", "--follow"]).command;

        assert!(matches!(
            parsed,
            Some(Command::Service {
                command: ServiceCommand::Logs {
                    lines: 20,
                    follow: true,
                }
            })
        ));
    }

    #[test]
    fn unknown_subcommands_and_flags_are_rejected() {
        assert!(Cli::try_parse_from(["zproxy", "restart"]).is_err());
        assert!(Cli::try_parse_from(["zproxy", "logs", "--lines", "abc"]).is_err());
        assert!(Cli::try_parse_from(["zproxy", "service"]).is_err());
        assert!(Cli::try_parse_from(["zproxy", "service", "nope"]).is_err());
    }
}
