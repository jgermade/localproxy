use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use dialoguer::Confirm;

use crate::{app, config, control, service};

const BLOCK_BEGIN: &str =
    "# --- localproxy -----------------------------------------------------------";
const BLOCK_END: &str =
    "# --- end localproxy -------------------------------------------------------";

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
    /// Elimina el binario, la configuración y el snippet de shell.
    /// Pasa --confirm para omitir la confirmación interactiva.
    Purge {
        #[arg(long)]
        confirm: bool,
    },
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
        Command::Purge { confirm } => run_purge(paths, confirm).await,
    }
}

async fn run_purge(paths: config::AppPaths, confirm: bool) -> Result<()> {
    let home = dirs::home_dir().context("no se pudo resolver HOME")?;
    let exe = std::env::current_exe().context("no se pudo resolver la ruta del ejecutable")?;

    let profiles: Vec<PathBuf> = [
        home.join(".zshrc"),
        home.join(".bashrc"),
        home.join(".bash_profile"),
    ]
    .into_iter()
    .filter(|p| p.exists())
    .collect();

    println!("Se eliminarán los siguientes elementos:");
    println!("  binario:       {}", exe.display());
    println!("  configuración: {}", paths.config_dir.display());
    println!("  estado:        {}", paths.state_dir.display());
    for p in &profiles {
        println!("  snippet en:    {}", p.display());
    }

    let proceed = if confirm {
        true
    } else {
        Confirm::new()
            .with_prompt("¿Continuar? Esta operación no se puede deshacer")
            .default(false)
            .interact()?
    };

    if !proceed {
        println!("cancelado");
        return Ok(());
    }

    // Detener el daemon (best effort)
    let _ = control::send_command(paths.control_socket(), control::ControlCommand::Stop).await;

    // Desinstalar el servicio si está instalado (best effort)
    let _ = service::uninstall(&paths);

    // Eliminar directorio de configuración
    if paths.config_dir.exists() {
        fs::remove_dir_all(&paths.config_dir)
            .with_context(|| format!("no se pudo borrar {}", paths.config_dir.display()))?;
        println!("eliminado: {}", paths.config_dir.display());
    }

    // Eliminar directorio de estado
    if paths.state_dir.exists() {
        fs::remove_dir_all(&paths.state_dir)
            .with_context(|| format!("no se pudo borrar {}", paths.state_dir.display()))?;
        println!("eliminado: {}", paths.state_dir.display());
    }

    // Eliminar snippets de shell
    for p in &profiles {
        match strip_shell_block(p) {
            Ok(true) => println!("snippet eliminado de: {}", p.display()),
            Ok(false) => {}
            Err(e) => eprintln!("advertencia: no se pudo actualizar {}: {}", p.display(), e),
        }
    }

    // Eliminar el binario (al final, pues lo estamos ejecutando)
    if exe.exists() {
        fs::remove_file(&exe)
            .with_context(|| format!("no se pudo borrar el binario {}", exe.display()))?;
        println!("eliminado: {}", exe.display());
    }

    println!("purge completado");
    Ok(())
}

/// Elimina el bloque delimitado por BLOCK_BEGIN / BLOCK_END del fichero dado.
/// Devuelve true si se encontró y eliminó el bloque, false si no existía.
fn strip_shell_block(path: &PathBuf) -> Result<bool> {
    let content =
        fs::read_to_string(path).with_context(|| format!("no se pudo leer {}", path.display()))?;
    let lines: Vec<&str> = content.lines().collect();

    let Some(begin) = lines.iter().position(|l| *l == BLOCK_BEGIN) else {
        return Ok(false);
    };
    let end = lines[begin..]
        .iter()
        .position(|l| *l == BLOCK_END)
        .map(|i| begin + i)
        .unwrap_or(lines.len().saturating_sub(1));

    // Incluir la línea en blanco inmediatamente anterior al bloque si existe
    let start = if begin > 0 && lines[begin - 1].trim().is_empty() {
        begin - 1
    } else {
        begin
    };

    let mut new_lines: Vec<&str> = lines[..start].to_vec();
    new_lines.extend_from_slice(&lines[end + 1..]);

    // Eliminar líneas en blanco al final
    while new_lines.last().is_some_and(|l| l.trim().is_empty()) {
        new_lines.pop();
    }

    let new_content = if new_lines.is_empty() {
        String::new()
    } else {
        new_lines.join("\n") + "\n"
    };

    fs::write(path, new_content)
        .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    Ok(true)
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
