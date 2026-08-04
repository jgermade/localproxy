use std::{fmt, path::PathBuf};

use anyhow::{Context, Result};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};
use tracing::{debug, info};

use crate::{app::SharedState, config};

#[derive(Debug, Clone, Copy)]
pub enum ControlCommand {
    Status,
    Reload,
    Stop,
}

impl fmt::Display for ControlCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            ControlCommand::Status => "status",
            ControlCommand::Reload => "reload",
            ControlCommand::Stop => "stop",
        };
        write!(f, "{text}")
    }
}

impl std::str::FromStr for ControlCommand {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "status" => Ok(Self::Status),
            "reload" => Ok(Self::Reload),
            "stop" => Ok(Self::Stop),
            other => Err(anyhow::anyhow!("comando no soportado: {other}")),
        }
    }
}

pub async fn serve(state: SharedState) -> Result<()> {
    let socket_path = state.paths.control_socket();
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("no se pudo bindear {}", socket_path.display()))?;
    info!(socket = %socket_path.display(), "control socket listo");

    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => {
                let _ = std::fs::remove_file(&socket_path);
                return Ok(());
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let request_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_client(stream, request_state).await {
                        debug!(%error, "falló petición de control");
                    }
                });
            }
        }
    }
}

pub async fn send_command(socket_path: PathBuf, command: ControlCommand) -> Result<String> {
    let stream = UnixStream::connect(&socket_path)
        .await
        .with_context(|| format!("no se pudo conectar a {}", socket_path.display()))?;
    let (reader, mut writer) = stream.into_split();
    writer.write_all(command.to_string().as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.shutdown().await?;

    let mut response = String::new();
    let mut reader = BufReader::new(reader);
    reader.read_to_string(&mut response).await?;
    Ok(response.trim().to_string())
}

async fn handle_client(stream: UnixStream, state: SharedState) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let command: ControlCommand = line.parse()?;
    let response = match command {
        ControlCommand::Status => {
            let cfg = state.config.read().await.clone();
            let gateway = *state.gateway_ip.read().await;
            config::summarize(&cfg, gateway)
        }
        ControlCommand::Reload => {
            let reloaded = config::load_or_create(&state.paths)?;
            let summary = config::summarize(&reloaded, *state.gateway_ip.read().await);
            *state.config.write().await = reloaded;
            format!("reloaded: {summary}")
        }
        ControlCommand::Stop => {
            state.shutdown.cancel();
            "stopping".to_string()
        }
    };

    writer.write_all(response.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
