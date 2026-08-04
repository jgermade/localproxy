use std::{net::IpAddr, process::Stdio, time::Duration};

use anyhow::{Context, Result, anyhow};
use tokio::{process::Command, time};
use tracing::{debug, warn};

use crate::{app::SharedState, config};

pub async fn run(state: SharedState) -> Result<()> {
    loop {
        if state.shutdown.is_cancelled() {
            return Ok(());
        }

        let cfg = state.config.read().await.clone();
        let interval_secs = config::gateway_poll_interval_secs(&cfg.upstream).max(1);

        if matches!(cfg.upstream, config::UpstreamConfig::Gateway { .. }) {
            match detect_default_gateway().await {
                Ok(gateway) => {
                    let mut current = state.gateway_ip.write().await;
                    if *current != Some(gateway) {
                        debug!(gateway = %gateway, "gateway actualizado");
                        *current = Some(gateway);
                    }
                }
                Err(error) => warn!(%error, "no se pudo detectar gateway por defecto"),
            }
        } else {
            let mut current = state.gateway_ip.write().await;
            *current = None;
        }

        tokio::select! {
            _ = state.shutdown.cancelled() => return Ok(()),
            _ = time::sleep(Duration::from_secs(interval_secs)) => {}
        }
    }
}

async fn detect_default_gateway() -> Result<IpAddr> {
    #[cfg(target_os = "macos")]
    {
        detect_macos_gateway().await
    }

    #[cfg(target_os = "linux")]
    {
        detect_linux_gateway().await
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Err(anyhow!("plataforma no soportada para detección de gateway"))
    }
}

#[cfg(target_os = "macos")]
async fn detect_macos_gateway() -> Result<IpAddr> {
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("falló route -n get default")?;

    if !output.status.success() {
        return Err(anyhow!(
            "route devolvió {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    parse_gateway_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "linux")]
async fn detect_linux_gateway() -> Result<IpAddr> {
    if let Ok(proc_contents) = tokio::fs::read_to_string("/proc/net/route").await {
        if let Some(ip) = parse_linux_proc_route(&proc_contents)? {
            return Ok(ip);
        }
    }

    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("falló ip route show default")?;

    if !output.status.success() {
        return Err(anyhow!(
            "ip route devolvió {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    parse_linux_ip_route(&String::from_utf8_lossy(&output.stdout))
}

fn parse_gateway_output(output: &str) -> Result<IpAddr> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("gateway:"))
        .map(str::trim)
        .ok_or_else(|| anyhow!("no se encontró la línea gateway"))?
        .parse()
        .context("gateway inválido")
}

#[cfg(target_os = "linux")]
fn parse_linux_proc_route(contents: &str) -> Result<Option<IpAddr>> {
    for line in contents.lines().skip(1) {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() > 2 && columns[1] == "00000000" {
            let hex = u32::from_str_radix(columns[2], 16).context("gateway hex inválido")?;
            let octets = hex.to_le_bytes();
            return Ok(Some(IpAddr::from(octets)));
        }
    }
    Ok(None)
}

#[cfg(target_os = "linux")]
fn parse_linux_ip_route(contents: &str) -> Result<IpAddr> {
    let gateway = contents
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(2)
        .find_map(|pair| (pair[0] == "via").then_some(pair[1]))
        .ok_or_else(|| anyhow!("no se encontró gateway en ip route"))?;

    gateway.parse().context("gateway inválido")
}
