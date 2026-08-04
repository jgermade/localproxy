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
    Status,
    Stop,
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
        Err(error) => println!("config guardada; daemon no notificado: {error}"),
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
        println!("daemon iniciado en background con pid {pid}");
        return Ok(());
    }

    if service::is_installed(&paths)? {
        service::start(&paths)?;
        println!("servicio iniciado");
        return Ok(());
    }

    let confirm = Confirm::new()
        .with_prompt("No hay servicio instalado. ¿Quieres ejecutar start --detached?")
        .default(true)
        .interact()?;

    if confirm {
        let pid = app::start_detached(&paths)?;
        println!("daemon iniciado en background con pid {pid}");
    } else {
        println!("cancelado");
    }

    Ok(())
}

fn run_service(paths: config::AppPaths, command: ServiceCommand) -> Result<()> {
    match command {
        ServiceCommand::Install => service::install(&paths),
        ServiceCommand::Start => {
            service::start(&paths)?;
            println!("servicio iniciado");
            Ok(())
        }
        ServiceCommand::Status => {
            let status = service::status(&paths)?;
            println!("{status}");
            Ok(())
        }
        ServiceCommand::Stop => {
            service::stop(&paths)?;
            println!("servicio detenido");
            Ok(())
        }
        ServiceCommand::Uninstall => service::uninstall(&paths),
    }
}
