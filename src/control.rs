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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{test_paths, test_state};
    use std::time::Duration;
    use tokio::time::timeout;

    async fn spawn_server(dir: &std::path::Path) -> (crate::app::SharedState, PathBuf) {
        let paths = test_paths(dir);
        paths.ensure_dirs().unwrap();
        let state = test_state(paths.clone(), config::AppConfig::default());
        let socket_path = paths.control_socket();

        let server_state = state.clone();
        tokio::spawn(async move { serve(server_state).await });

        for _ in 0..200 {
            if UnixStream::connect(&socket_path).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        (state, socket_path)
    }

    #[test]
    fn commands_parse_from_their_wire_representation() {
        assert!(matches!(
            "status".parse::<ControlCommand>().unwrap(),
            ControlCommand::Status
        ));
        assert!(matches!(
            " reload \n".parse::<ControlCommand>().unwrap(),
            ControlCommand::Reload
        ));
        assert!(matches!(
            "stop".parse::<ControlCommand>().unwrap(),
            ControlCommand::Stop
        ));
    }

    #[test]
    fn unknown_commands_are_rejected() {
        let error = "restart".parse::<ControlCommand>().unwrap_err();

        assert!(error.to_string().contains("comando no soportado: restart"));
    }

    #[test]
    fn commands_render_back_to_their_wire_representation() {
        assert_eq!(ControlCommand::Status.to_string(), "status");
        assert_eq!(ControlCommand::Reload.to_string(), "reload");
        assert_eq!(ControlCommand::Stop.to_string(), "stop");
    }

    #[tokio::test]
    async fn status_returns_the_running_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let (state, socket_path) = spawn_server(dir.path()).await;

        let response = send_command(socket_path, ControlCommand::Status)
            .await
            .unwrap();

        assert_eq!(
            response,
            "listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown"
        );
        state.shutdown.cancel();
    }

    #[tokio::test]
    async fn reload_picks_up_changes_written_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let (state, socket_path) = spawn_server(dir.path()).await;

        let updated = config::AppConfig {
            upstream: config::UpstreamConfig::Static {
                protocol: config::ProxyProtocol::Socks5,
                host: "127.0.0.1".to_string(),
                port: 1080,
                connect_timeout_ms: 3_000,
            },
            ..config::AppConfig::default()
        };
        config::save(&state.paths, &updated).unwrap();

        let response = send_command(socket_path.clone(), ControlCommand::Reload)
            .await
            .unwrap();
        assert!(response.starts_with("reloaded: "));
        assert!(response.contains("upstream=static:socks5:127.0.0.1:1080"));

        let status = send_command(socket_path, ControlCommand::Status)
            .await
            .unwrap();
        assert!(status.contains("upstream=static:socks5:127.0.0.1:1080"));

        state.shutdown.cancel();
    }

    #[tokio::test]
    async fn stop_cancels_the_shutdown_token_and_removes_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let (state, socket_path) = spawn_server(dir.path()).await;

        let response = send_command(socket_path.clone(), ControlCommand::Stop)
            .await
            .unwrap();

        assert_eq!(response, "stopping");
        timeout(Duration::from_secs(5), state.shutdown.cancelled())
            .await
            .expect("el daemon debería cancelarse");
    }

    #[tokio::test]
    async fn unknown_commands_close_the_connection_without_a_response() {
        let dir = tempfile::tempdir().unwrap();
        let (state, socket_path) = spawn_server(dir.path()).await;

        let stream = UnixStream::connect(&socket_path).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        writer.write_all(b"restart\n").await.unwrap();
        writer.shutdown().await.unwrap();

        let mut response = String::new();
        BufReader::new(reader)
            .read_to_string(&mut response)
            .await
            .unwrap();

        assert!(response.is_empty());
        state.shutdown.cancel();
    }

    #[tokio::test]
    async fn sending_to_a_missing_socket_fails() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("zproxy.sock");

        let error = send_command(missing.clone(), ControlCommand::Status)
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no se pudo conectar"));
    }

    #[tokio::test]
    async fn serve_replaces_a_stale_socket_file() {
        let dir = tempfile::tempdir().unwrap();
        let paths = test_paths(dir.path());
        paths.ensure_dirs().unwrap();
        std::fs::write(paths.control_socket(), b"stale").unwrap();

        let (state, socket_path) = spawn_server(dir.path()).await;

        let response = send_command(socket_path, ControlCommand::Status)
            .await
            .unwrap();
        assert!(response.starts_with("listen="));
        state.shutdown.cancel();
    }
}
