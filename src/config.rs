use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result, anyhow, bail};
use dialoguer::{Input, Select, theme::ColorfulTheme};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub config_file: PathBuf,
    pub state_dir: PathBuf,
}

impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home = dirs::home_dir().context("no se pudo resolver HOME")?;
        let config_dir = home.join(".config").join("zproxy");
        let state_dir = home.join(".local").join("state").join("zproxy");

        Ok(Self {
            config_file: config_dir.join("config.toml"),
            config_dir,
            state_dir,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)?;
        fs::create_dir_all(&self.state_dir)?;
        Ok(())
    }

    pub fn control_socket(&self) -> PathBuf {
        self.state_dir.join("zproxy.sock")
    }

    pub fn pid_file(&self) -> PathBuf {
        self.state_dir.join("zproxy.pid")
    }

    pub fn lock_file(&self) -> PathBuf {
        self.state_dir.join("zproxy.lock")
    }

    pub fn log_file(&self) -> PathBuf {
        self.state_dir.join("zproxy.log")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub listen: ListenConfig,
    #[serde(default)]
    pub upstream: UpstreamConfig,
    #[serde(default)]
    pub fallback: FallbackConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen: ListenConfig::default(),
            upstream: UpstreamConfig::None,
            fallback: FallbackConfig::Direct,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenConfig {
    #[serde(default = "default_listen_host")]
    pub host: IpAddr,
    #[serde(default = "default_listen_port")]
    pub port: u16,
}

impl Default for ListenConfig {
    fn default() -> Self {
        Self {
            host: default_listen_host(),
            port: default_listen_port(),
        }
    }
}

impl ListenConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpstreamConfig {
    None,
    Gateway {
        #[serde(default)]
        protocol: ProxyProtocol,
        port: u16,
        #[serde(default = "default_poll_interval_secs")]
        poll_interval_secs: u64,
        #[serde(default = "default_connect_timeout_ms")]
        connect_timeout_ms: u64,
    },
    Static {
        #[serde(default)]
        protocol: ProxyProtocol,
        host: String,
        port: u16,
        #[serde(default = "default_connect_timeout_ms")]
        connect_timeout_ms: u64,
    },
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FallbackConfig {
    None,
    Direct,
    Static {
        #[serde(default)]
        protocol: ProxyProtocol,
        host: String,
        port: u16,
        #[serde(default = "default_connect_timeout_ms")]
        connect_timeout_ms: u64,
    },
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self::Direct
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProxyProtocol {
    #[default]
    Http,
    Socks5,
}

#[derive(Debug, Clone)]
pub struct ProxyEndpoint {
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    pub connect_timeout_ms: u64,
}

impl ProxyEndpoint {
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub fn load_or_create(paths: &AppPaths) -> Result<AppConfig> {
    paths.ensure_dirs()?;

    if !paths.config_file.exists() {
        let config = AppConfig::default();
        save(paths, &config)?;
        return Ok(config);
    }

    let raw = fs::read_to_string(&paths.config_file)
        .with_context(|| format!("no se pudo leer {}", paths.config_file.display()))?;
    let config: AppConfig = toml::from_str(&raw)
        .with_context(|| format!("config TOML inválida en {}", paths.config_file.display()))?;
    Ok(config)
}

pub fn save(paths: &AppPaths, config: &AppConfig) -> Result<()> {
    paths.ensure_dirs()?;
    let serialized = toml::to_string_pretty(config)?;
    fs::write(&paths.config_file, serialized)
        .with_context(|| format!("no se pudo escribir {}", paths.config_file.display()))?;
    Ok(())
}

pub fn run_wizard(current: AppConfig) -> Result<AppConfig> {
    let theme = ColorfulTheme::default();

    let listen_host: IpAddr = Input::with_theme(&theme)
        .with_prompt("Listen host")
        .default(current.listen.host)
        .interact_text()?;
    let listen_port: u16 = Input::with_theme(&theme)
        .with_prompt("Listen port")
        .default(current.listen.port)
        .interact_text()?;

    let upstream = prompt_upstream(&theme, &current.upstream)?;
    let fallback = prompt_fallback(&theme, &current.fallback)?;

    Ok(AppConfig {
        listen: ListenConfig {
            host: listen_host,
            port: listen_port,
        },
        upstream,
        fallback,
    })
}

pub fn resolve_upstream_endpoint(
    config: &UpstreamConfig,
    gateway_ip: Option<IpAddr>,
) -> Option<ProxyEndpoint> {
    match config {
        UpstreamConfig::None => None,
        UpstreamConfig::Gateway {
            protocol,
            port,
            connect_timeout_ms,
            ..
        } => gateway_ip.map(|ip| ProxyEndpoint {
            protocol: *protocol,
            host: ip.to_string(),
            port: *port,
            connect_timeout_ms: *connect_timeout_ms,
        }),
        UpstreamConfig::Static {
            protocol,
            host,
            port,
            connect_timeout_ms,
        } => Some(ProxyEndpoint {
            protocol: *protocol,
            host: host.clone(),
            port: *port,
            connect_timeout_ms: *connect_timeout_ms,
        }),
    }
}

pub fn resolve_fallback_endpoint(config: &FallbackConfig) -> Option<ProxyEndpoint> {
    match config {
        FallbackConfig::None | FallbackConfig::Direct => None,
        FallbackConfig::Static {
            protocol,
            host,
            port,
            connect_timeout_ms,
        } => Some(ProxyEndpoint {
            protocol: *protocol,
            host: host.clone(),
            port: *port,
            connect_timeout_ms: *connect_timeout_ms,
        }),
    }
}

pub fn gateway_poll_interval_secs(config: &UpstreamConfig) -> u64 {
    match config {
        UpstreamConfig::Gateway {
            poll_interval_secs, ..
        } => *poll_interval_secs,
        _ => default_poll_interval_secs(),
    }
}

pub fn fallback_allows_direct(config: &FallbackConfig) -> bool {
    matches!(config, FallbackConfig::Direct)
}

pub fn summarize(config: &AppConfig, gateway: Option<IpAddr>) -> String {
    format!(
        "listen={} upstream={} fallback={} gateway={}",
        config.listen.socket_addr(),
        describe_upstream(&config.upstream),
        describe_fallback(&config.fallback),
        gateway
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )
}

fn prompt_upstream(theme: &ColorfulTheme, current: &UpstreamConfig) -> Result<UpstreamConfig> {
    let modes = ["none", "gateway", "static"];
    let current_index = match current {
        UpstreamConfig::None => 0,
        UpstreamConfig::Gateway { .. } => 1,
        UpstreamConfig::Static { .. } => 2,
    };
    let mode = Select::with_theme(theme)
        .with_prompt("Upstream type")
        .default(current_index)
        .items(modes)
        .interact()?;

    match mode {
        0 => Ok(UpstreamConfig::None),
        1 => {
            let protocol = prompt_protocol(theme, protocol_from_upstream(current))?;
            let port = Input::with_theme(theme)
                .with_prompt("Gateway upstream port")
                .default(port_from_upstream(current).unwrap_or(8080))
                .interact_text()?;
            let poll_interval_secs = Input::with_theme(theme)
                .with_prompt("Gateway poll interval (seconds)")
                .default(
                    poll_interval_from_upstream(current).unwrap_or(default_poll_interval_secs()),
                )
                .interact_text()?;
            Ok(UpstreamConfig::Gateway {
                protocol,
                port,
                poll_interval_secs,
                connect_timeout_ms: default_connect_timeout_ms(),
            })
        }
        2 => {
            let protocol = prompt_protocol(theme, protocol_from_upstream(current))?;
            let host = Input::with_theme(theme)
                .with_prompt("Static upstream host")
                .default(host_from_upstream(current).unwrap_or_else(|| "127.0.0.1".to_string()))
                .interact_text()?;
            let port = Input::with_theme(theme)
                .with_prompt("Static upstream port")
                .default(port_from_upstream(current).unwrap_or(8080))
                .interact_text()?;
            Ok(UpstreamConfig::Static {
                protocol,
                host,
                port,
                connect_timeout_ms: default_connect_timeout_ms(),
            })
        }
        _ => bail!("tipo de upstream no soportado"),
    }
}

fn prompt_fallback(theme: &ColorfulTheme, current: &FallbackConfig) -> Result<FallbackConfig> {
    let modes = ["none", "direct", "static"];
    let current_index = match current {
        FallbackConfig::None => 0,
        FallbackConfig::Direct => 1,
        FallbackConfig::Static { .. } => 2,
    };
    let mode = Select::with_theme(theme)
        .with_prompt("Fallback type")
        .default(current_index)
        .items(modes)
        .interact()?;

    match mode {
        0 => Ok(FallbackConfig::None),
        1 => Ok(FallbackConfig::Direct),
        2 => {
            let protocol = prompt_protocol(theme, protocol_from_fallback(current))?;
            let host = Input::with_theme(theme)
                .with_prompt("Fallback host")
                .default(host_from_fallback(current).unwrap_or_else(|| "127.0.0.1".to_string()))
                .interact_text()?;
            let port = Input::with_theme(theme)
                .with_prompt("Fallback port")
                .default(port_from_fallback(current).unwrap_or(8080))
                .interact_text()?;
            Ok(FallbackConfig::Static {
                protocol,
                host,
                port,
                connect_timeout_ms: default_connect_timeout_ms(),
            })
        }
        _ => bail!("tipo de fallback no soportado"),
    }
}

fn prompt_protocol(theme: &ColorfulTheme, current: ProxyProtocol) -> Result<ProxyProtocol> {
    let protocols = ["http", "socks5"];
    let default_index = match current {
        ProxyProtocol::Http => 0,
        ProxyProtocol::Socks5 => 1,
    };
    let selected = Select::with_theme(theme)
        .with_prompt("Proxy protocol")
        .default(default_index)
        .items(protocols)
        .interact()?;
    match selected {
        0 => Ok(ProxyProtocol::Http),
        1 => Ok(ProxyProtocol::Socks5),
        _ => bail!("protocolo no soportado"),
    }
}

fn protocol_from_upstream(config: &UpstreamConfig) -> ProxyProtocol {
    match config {
        UpstreamConfig::Gateway { protocol, .. } | UpstreamConfig::Static { protocol, .. } => {
            *protocol
        }
        UpstreamConfig::None => ProxyProtocol::Http,
    }
}

fn protocol_from_fallback(config: &FallbackConfig) -> ProxyProtocol {
    match config {
        FallbackConfig::Static { protocol, .. } => *protocol,
        _ => ProxyProtocol::Http,
    }
}

fn host_from_upstream(config: &UpstreamConfig) -> Option<String> {
    match config {
        UpstreamConfig::Static { host, .. } => Some(host.clone()),
        _ => None,
    }
}

fn host_from_fallback(config: &FallbackConfig) -> Option<String> {
    match config {
        FallbackConfig::Static { host, .. } => Some(host.clone()),
        _ => None,
    }
}

fn port_from_upstream(config: &UpstreamConfig) -> Option<u16> {
    match config {
        UpstreamConfig::Gateway { port, .. } | UpstreamConfig::Static { port, .. } => Some(*port),
        UpstreamConfig::None => None,
    }
}

fn port_from_fallback(config: &FallbackConfig) -> Option<u16> {
    match config {
        FallbackConfig::Static { port, .. } => Some(*port),
        _ => None,
    }
}

fn poll_interval_from_upstream(config: &UpstreamConfig) -> Option<u64> {
    match config {
        UpstreamConfig::Gateway {
            poll_interval_secs, ..
        } => Some(*poll_interval_secs),
        _ => None,
    }
}

fn describe_upstream(config: &UpstreamConfig) -> String {
    match config {
        UpstreamConfig::None => "none".to_string(),
        UpstreamConfig::Gateway { protocol, port, .. } => {
            format!("gateway:{}:{}", protocol_name(*protocol), port)
        }
        UpstreamConfig::Static {
            protocol,
            host,
            port,
            ..
        } => format!("static:{}:{}:{}", protocol_name(*protocol), host, port),
    }
}

fn describe_fallback(config: &FallbackConfig) -> String {
    match config {
        FallbackConfig::None => "none".to_string(),
        FallbackConfig::Direct => "direct".to_string(),
        FallbackConfig::Static {
            protocol,
            host,
            port,
            ..
        } => format!("static:{}:{}:{}", protocol_name(*protocol), host, port),
    }
}

fn protocol_name(protocol: ProxyProtocol) -> &'static str {
    match protocol {
        ProxyProtocol::Http => "http",
        ProxyProtocol::Socks5 => "socks5",
    }
}

fn default_listen_host() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn default_listen_port() -> u16 {
    8888
}

fn default_poll_interval_secs() -> u64 {
    5
}

fn default_connect_timeout_ms() -> u64 {
    3_000
}
