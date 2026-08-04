use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::Confirm;

use crate::{app, config, control, service};

#[derive(Debug, Parser)]
#[command(
    name = "localproxy",
    author,
    version,
    about = "Local proxy daemon with dynamic upstream resolution"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
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
    /// Prints the proxy URL built from the listen address in config.toml.
    Url,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ServiceCommand {
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

/// Parses the process arguments and runs the requested command.
pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let paths = config::AppPaths::discover()?;
    dispatch(cli.command.unwrap_or(Command::Daemon), paths).await
}

/// Runs a single command against the given paths.
pub async fn dispatch(command: Command, paths: config::AppPaths) -> Result<()> {
    match command {
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
            print_paths(&paths);
            Ok(())
        }
        Command::Url => {
            println!("{}", config::load_or_create(&paths)?.listen.proxy_url());
            Ok(())
        }
    }
}

pub fn print_paths(paths: &config::AppPaths) {
    println!("config: {}", paths.config_file.display());
    println!("state: {}", paths.state_dir.display());
    println!("socket: {}", paths.control_socket().display());
    println!("pid: {}", paths.pid_file().display());
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
