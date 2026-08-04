mod app;
mod config;
mod control;
mod gateway;
mod proxy;
mod stream;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

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
    Paths,
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
        Err(error) => println!("config guardada; daemon no notificado: {error}"),
    }

    Ok(())
}

async fn run_control(socket_path: PathBuf, command: control::ControlCommand) -> Result<()> {
    let response = control::send_command(socket_path, command).await?;
    println!("{response}");
    Ok(())
}
