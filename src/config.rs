use std::{
    fs,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
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
    #[serde(default, rename = "proxy", skip_serializing_if = "Vec::is_empty")]
    pub proxies: Vec<SavedProxy>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen: ListenConfig::default(),
            upstream: UpstreamConfig::None,
            fallback: FallbackConfig::Direct,
            proxies: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn find_proxy(&self, name: &str) -> Option<&SavedProxy> {
        self.proxies.iter().find(|proxy| proxy.name == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedProxy {
    pub name: String,
    #[serde(default)]
    pub protocol: ProxyProtocol,
    pub host: String,
    pub port: u16,
    #[serde(default = "default_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
}

impl SavedProxy {
    pub fn endpoint(&self) -> ProxyEndpoint {
        ProxyEndpoint {
            protocol: self.protocol,
            host: self.host.clone(),
            port: self.port,
            connect_timeout_ms: self.connect_timeout_ms,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpstreamConfig {
    #[default]
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
    Saved {
        name: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FallbackConfig {
    None,
    #[default]
    Direct,
    Saved {
        name: String,
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

    let proxies = prompt_proxy_list(&theme, current.proxies.clone())?;
    let upstream = prompt_upstream(&theme, &current.upstream, &proxies)?;
    let fallback = prompt_fallback(&theme, &current.fallback, &proxies)?;

    Ok(AppConfig {
        listen: ListenConfig {
            host: listen_host,
            port: listen_port,
        },
        upstream,
        fallback,
        proxies,
    })
}

pub fn resolve_upstream_endpoint(
    config: &AppConfig,
    gateway_ip: Option<IpAddr>,
) -> Option<ProxyEndpoint> {
    match &config.upstream {
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
        UpstreamConfig::Saved { name } => config.find_proxy(name).map(SavedProxy::endpoint),
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

pub fn resolve_fallback_endpoint(config: &AppConfig) -> Option<ProxyEndpoint> {
    match &config.fallback {
        FallbackConfig::None | FallbackConfig::Direct => None,
        FallbackConfig::Saved { name } => config.find_proxy(name).map(SavedProxy::endpoint),
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

fn prompt_upstream(
    theme: &ColorfulTheme,
    current: &UpstreamConfig,
    proxies: &[SavedProxy],
) -> Result<UpstreamConfig> {
    let mut modes = vec![UpstreamKind::None, UpstreamKind::Gateway];
    if !proxies.is_empty() {
        modes.push(UpstreamKind::Saved);
    }
    modes.push(UpstreamKind::Static);

    let current_kind = UpstreamKind::of(current);
    let current_index = modes
        .iter()
        .position(|kind| *kind == current_kind)
        .unwrap_or(0);
    let labels: Vec<&str> = modes.iter().map(|kind| kind.label()).collect();
    let mode = Select::with_theme(theme)
        .with_prompt("Upstream type")
        .default(current_index)
        .items(&labels)
        .interact()?;

    match modes[mode] {
        UpstreamKind::None => Ok(UpstreamConfig::None),
        UpstreamKind::Gateway => {
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
        UpstreamKind::Saved => {
            let current_name = match current {
                UpstreamConfig::Saved { name } => Some(name.as_str()),
                _ => None,
            };
            let name = prompt_saved_selection(theme, "Upstream proxy", proxies, current_name)?;
            Ok(UpstreamConfig::Saved { name })
        }
        UpstreamKind::Static => {
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
    }
}

fn prompt_fallback(
    theme: &ColorfulTheme,
    current: &FallbackConfig,
    proxies: &[SavedProxy],
) -> Result<FallbackConfig> {
    let mut modes = vec![FallbackKind::None, FallbackKind::Direct];
    if !proxies.is_empty() {
        modes.push(FallbackKind::Saved);
    }
    modes.push(FallbackKind::Static);

    let current_kind = FallbackKind::of(current);
    let current_index = modes
        .iter()
        .position(|kind| *kind == current_kind)
        .unwrap_or(0);
    let labels: Vec<&str> = modes.iter().map(|kind| kind.label()).collect();
    let mode = Select::with_theme(theme)
        .with_prompt("Fallback type")
        .default(current_index)
        .items(&labels)
        .interact()?;

    match modes[mode] {
        FallbackKind::None => Ok(FallbackConfig::None),
        FallbackKind::Direct => Ok(FallbackConfig::Direct),
        FallbackKind::Saved => {
            let current_name = match current {
                FallbackConfig::Saved { name } => Some(name.as_str()),
                _ => None,
            };
            let name = prompt_saved_selection(theme, "Fallback proxy", proxies, current_name)?;
            Ok(FallbackConfig::Saved { name })
        }
        FallbackKind::Static => {
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpstreamKind {
    None,
    Gateway,
    Saved,
    Static,
}

impl UpstreamKind {
    fn of(config: &UpstreamConfig) -> Self {
        match config {
            UpstreamConfig::None => Self::None,
            UpstreamConfig::Gateway { .. } => Self::Gateway,
            UpstreamConfig::Saved { .. } => Self::Saved,
            UpstreamConfig::Static { .. } => Self::Static,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gateway => "gateway",
            Self::Saved => "saved (lista de proxies)",
            Self::Static => "static",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FallbackKind {
    None,
    Direct,
    Saved,
    Static,
}

impl FallbackKind {
    fn of(config: &FallbackConfig) -> Self {
        match config {
            FallbackConfig::None => Self::None,
            FallbackConfig::Direct => Self::Direct,
            FallbackConfig::Saved { .. } => Self::Saved,
            FallbackConfig::Static { .. } => Self::Static,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Direct => "direct",
            Self::Saved => "saved (lista de proxies)",
            Self::Static => "static",
        }
    }
}

fn prompt_proxy_list(
    theme: &ColorfulTheme,
    mut proxies: Vec<SavedProxy>,
) -> Result<Vec<SavedProxy>> {
    loop {
        let mut items: Vec<String> = proxies.iter().map(describe_saved_proxy).collect();
        let add_index = items.len();
        items.push("+ añadir proxy".to_string());
        let remove_index = items.len();
        if !proxies.is_empty() {
            items.push("- eliminar proxy".to_string());
        }
        let done_index = items.len();
        items.push("continuar".to_string());

        let selection = Select::with_theme(theme)
            .with_prompt("Proxies guardados (edita, añade o continúa)")
            .default(done_index)
            .items(&items)
            .interact()?;

        if selection == done_index {
            return Ok(proxies);
        }
        if selection == add_index {
            let proxy = prompt_saved_proxy(theme, None)?;
            match proxies.iter().position(|item| item.name == proxy.name) {
                Some(index) => proxies[index] = proxy,
                None => proxies.push(proxy),
            }
            continue;
        }
        if !proxies.is_empty() && selection == remove_index {
            let labels: Vec<String> = proxies.iter().map(describe_saved_proxy).collect();
            let target = Select::with_theme(theme)
                .with_prompt("¿Qué proxy quieres eliminar?")
                .default(0)
                .items(&labels)
                .interact()?;
            proxies.remove(target);
            continue;
        }

        let updated = prompt_saved_proxy(theme, Some(&proxies[selection]))?;
        proxies[selection] = updated;
    }
}

fn prompt_saved_proxy(theme: &ColorfulTheme, current: Option<&SavedProxy>) -> Result<SavedProxy> {
    let name: String = Input::with_theme(theme)
        .with_prompt("Nombre del proxy")
        .default(
            current
                .map(|proxy| proxy.name.clone())
                .unwrap_or_else(|| "proxy".to_string()),
        )
        .interact_text()?;
    let name = name.trim().to_string();
    if name.is_empty() {
        bail!("el nombre del proxy no puede estar vacío");
    }

    let protocol = prompt_protocol(
        theme,
        current.map(|proxy| proxy.protocol).unwrap_or_default(),
    )?;
    let host: String = Input::with_theme(theme)
        .with_prompt("Host")
        .default(
            current
                .map(|proxy| proxy.host.clone())
                .unwrap_or_else(|| "127.0.0.1".to_string()),
        )
        .interact_text()?;
    let port: u16 = Input::with_theme(theme)
        .with_prompt("Puerto")
        .default(current.map(|proxy| proxy.port).unwrap_or(8080))
        .interact_text()?;
    let connect_timeout_ms: u64 = Input::with_theme(theme)
        .with_prompt("Connect timeout (ms)")
        .default(
            current
                .map(|proxy| proxy.connect_timeout_ms)
                .unwrap_or_else(default_connect_timeout_ms),
        )
        .interact_text()?;

    Ok(SavedProxy {
        name,
        protocol,
        host: host.trim().to_string(),
        port,
        connect_timeout_ms,
    })
}

fn prompt_saved_selection(
    theme: &ColorfulTheme,
    prompt: &str,
    proxies: &[SavedProxy],
    current_name: Option<&str>,
) -> Result<String> {
    if proxies.is_empty() {
        bail!("no hay proxies guardados");
    }
    let labels: Vec<String> = proxies.iter().map(describe_saved_proxy).collect();
    let default_index = current_name
        .and_then(|name| proxies.iter().position(|proxy| proxy.name == name))
        .unwrap_or(0);
    let selection = Select::with_theme(theme)
        .with_prompt(prompt)
        .default(default_index)
        .items(&labels)
        .interact()?;
    Ok(proxies[selection].name.clone())
}

fn describe_saved_proxy(proxy: &SavedProxy) -> String {
    format!(
        "{} ({}://{}:{})",
        proxy.name,
        protocol_name(proxy.protocol),
        proxy.host,
        proxy.port
    )
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
        UpstreamConfig::None | UpstreamConfig::Saved { .. } => ProxyProtocol::Http,
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
        UpstreamConfig::None | UpstreamConfig::Saved { .. } => None,
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
        UpstreamConfig::Saved { name } => format!("saved:{name}"),
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
        FallbackConfig::Saved { name } => format!("saved:{name}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    fn paths_in(dir: &std::path::Path) -> AppPaths {
        let config_dir = dir.join("config");
        AppPaths {
            config_file: config_dir.join("config.toml"),
            config_dir,
            state_dir: dir.join("state"),
        }
    }

    fn static_proxy(name: &str, protocol: ProxyProtocol) -> SavedProxy {
        SavedProxy {
            name: name.to_string(),
            protocol,
            host: "10.0.0.1".to_string(),
            port: 3128,
            connect_timeout_ms: 1_500,
        }
    }

    #[test]
    fn app_paths_derive_runtime_files_from_state_dir() {
        let paths = paths_in(std::path::Path::new("/tmp/zproxy-test"));

        assert_eq!(paths.control_socket(), paths.state_dir.join("zproxy.sock"));
        assert_eq!(paths.pid_file(), paths.state_dir.join("zproxy.pid"));
        assert_eq!(paths.lock_file(), paths.state_dir.join("zproxy.lock"));
        assert_eq!(paths.log_file(), paths.state_dir.join("zproxy.log"));
    }

    #[test]
    fn ensure_dirs_creates_config_and_state_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());

        paths.ensure_dirs().unwrap();

        assert!(paths.config_dir.is_dir());
        assert!(paths.state_dir.is_dir());
    }

    #[test]
    fn default_config_listens_on_localhost_and_goes_direct() {
        let config = AppConfig::default();

        assert_eq!(config.listen.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.listen.port, 8888);
        assert_eq!(
            config.listen.socket_addr(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8888)
        );
        assert!(matches!(config.upstream, UpstreamConfig::None));
        assert!(matches!(config.fallback, FallbackConfig::Direct));
        assert!(config.proxies.is_empty());
    }

    #[test]
    fn listen_config_supports_ipv6() {
        let listen = ListenConfig {
            host: IpAddr::V6(Ipv6Addr::LOCALHOST),
            port: 9999,
        };

        assert_eq!(
            listen.socket_addr(),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9999)
        );
    }

    #[test]
    fn empty_toml_falls_back_to_defaults() {
        let config: AppConfig = toml::from_str("").unwrap();

        assert_eq!(config.listen.port, 8888);
        assert!(matches!(config.upstream, UpstreamConfig::None));
        assert!(matches!(config.fallback, FallbackConfig::Direct));
    }

    #[test]
    fn toml_roundtrip_preserves_every_section() {
        let config = AppConfig {
            listen: ListenConfig {
                host: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
                port: 1234,
            },
            upstream: UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Socks5,
                port: 1080,
                poll_interval_secs: 30,
                connect_timeout_ms: 250,
            },
            fallback: FallbackConfig::Static {
                protocol: ProxyProtocol::Http,
                host: "proxy.example.com".to_string(),
                port: 8080,
                connect_timeout_ms: 400,
            },
            proxies: vec![static_proxy("corp", ProxyProtocol::Socks5)],
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&serialized).unwrap();

        assert_eq!(parsed.listen.port, 1234);
        assert_eq!(describe_upstream(&parsed.upstream), "gateway:socks5:1080");
        assert_eq!(
            describe_fallback(&parsed.fallback),
            "static:http:proxy.example.com:8080"
        );
        assert_eq!(parsed.proxies.len(), 1);
        assert_eq!(parsed.proxies[0].name, "corp");
        assert_eq!(parsed.proxies[0].connect_timeout_ms, 1_500);
    }

    #[test]
    fn saved_proxies_are_omitted_when_empty() {
        let serialized = toml::to_string_pretty(&AppConfig::default()).unwrap();

        assert!(!serialized.contains("[[proxy]]"));
    }

    #[test]
    fn optional_fields_use_defaults_when_missing() {
        let raw = r#"
            [upstream]
            type = "gateway"
            port = 8080

            [[proxy]]
            name = "corp"
            host = "10.0.0.1"
            port = 3128
        "#;

        let config: AppConfig = toml::from_str(raw).unwrap();

        assert_eq!(gateway_poll_interval_secs(&config.upstream), 5);
        assert!(matches!(
            config.upstream,
            UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Http,
                connect_timeout_ms: 3_000,
                ..
            }
        ));
        assert!(matches!(config.proxies[0].protocol, ProxyProtocol::Http));
        assert_eq!(config.proxies[0].connect_timeout_ms, 3_000);
    }

    #[test]
    fn find_proxy_matches_by_name() {
        let config = AppConfig {
            proxies: vec![
                static_proxy("corp", ProxyProtocol::Http),
                static_proxy("home", ProxyProtocol::Socks5),
            ],
            ..AppConfig::default()
        };

        assert_eq!(config.find_proxy("home").unwrap().name, "home");
        assert!(config.find_proxy("missing").is_none());
    }

    #[test]
    fn saved_proxy_endpoint_copies_every_field() {
        let endpoint = static_proxy("corp", ProxyProtocol::Socks5).endpoint();

        assert!(matches!(endpoint.protocol, ProxyProtocol::Socks5));
        assert_eq!(endpoint.address(), "10.0.0.1:3128");
        assert_eq!(endpoint.connect_timeout_ms, 1_500);
    }

    #[test]
    fn load_or_create_writes_the_default_config_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());

        let config = load_or_create(&paths).unwrap();

        assert!(paths.config_file.is_file());
        assert_eq!(config.listen.port, 8888);
    }

    #[test]
    fn load_or_create_reads_an_existing_config() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        paths.ensure_dirs().unwrap();
        fs::write(
            &paths.config_file,
            "[listen]\nhost = \"0.0.0.0\"\nport = 9000\n",
        )
        .unwrap();

        let config = load_or_create(&paths).unwrap();

        assert_eq!(config.listen.port, 9000);
    }

    #[test]
    fn load_or_create_reports_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        paths.ensure_dirs().unwrap();
        fs::write(&paths.config_file, "this is not toml =").unwrap();

        let error = load_or_create(&paths).unwrap_err();

        assert!(error.to_string().contains("config TOML inválida"));
    }

    #[test]
    fn save_then_load_returns_the_same_config() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        let config = AppConfig {
            upstream: UpstreamConfig::Saved {
                name: "corp".to_string(),
            },
            proxies: vec![static_proxy("corp", ProxyProtocol::Http)],
            ..AppConfig::default()
        };

        save(&paths, &config).unwrap();
        let loaded = load_or_create(&paths).unwrap();

        assert_eq!(describe_upstream(&loaded.upstream), "saved:corp");
        assert_eq!(loaded.proxies.len(), 1);
    }

    #[test]
    fn upstream_none_resolves_to_no_endpoint() {
        let config = AppConfig::default();

        assert!(resolve_upstream_endpoint(&config, None).is_none());
    }

    #[test]
    fn gateway_upstream_needs_a_detected_gateway() {
        let config = AppConfig {
            upstream: UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Http,
                port: 8080,
                poll_interval_secs: 5,
                connect_timeout_ms: 3_000,
            },
            ..AppConfig::default()
        };

        assert!(resolve_upstream_endpoint(&config, None).is_none());

        let gateway = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let endpoint = resolve_upstream_endpoint(&config, Some(gateway)).unwrap();

        assert_eq!(endpoint.address(), "192.168.1.1:8080");
        assert!(matches!(endpoint.protocol, ProxyProtocol::Http));
    }

    #[test]
    fn saved_upstream_resolves_through_the_proxy_list() {
        let config = AppConfig {
            upstream: UpstreamConfig::Saved {
                name: "corp".to_string(),
            },
            proxies: vec![static_proxy("corp", ProxyProtocol::Socks5)],
            ..AppConfig::default()
        };

        let endpoint = resolve_upstream_endpoint(&config, None).unwrap();
        assert_eq!(endpoint.address(), "10.0.0.1:3128");

        let orphan = AppConfig {
            upstream: UpstreamConfig::Saved {
                name: "missing".to_string(),
            },
            ..AppConfig::default()
        };
        assert!(resolve_upstream_endpoint(&orphan, None).is_none());
    }

    #[test]
    fn static_upstream_resolves_directly() {
        let config = AppConfig {
            upstream: UpstreamConfig::Static {
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".to_string(),
                port: 1080,
                connect_timeout_ms: 100,
            },
            ..AppConfig::default()
        };

        let endpoint = resolve_upstream_endpoint(&config, None).unwrap();

        assert_eq!(endpoint.address(), "127.0.0.1:1080");
        assert_eq!(endpoint.connect_timeout_ms, 100);
    }

    #[test]
    fn none_and_direct_fallbacks_have_no_endpoint() {
        let mut config = AppConfig::default();
        assert!(resolve_fallback_endpoint(&config).is_none());

        config.fallback = FallbackConfig::None;
        assert!(resolve_fallback_endpoint(&config).is_none());
    }

    #[test]
    fn saved_and_static_fallbacks_resolve_to_endpoints() {
        let config = AppConfig {
            fallback: FallbackConfig::Saved {
                name: "corp".to_string(),
            },
            proxies: vec![static_proxy("corp", ProxyProtocol::Http)],
            ..AppConfig::default()
        };
        assert_eq!(
            resolve_fallback_endpoint(&config).unwrap().address(),
            "10.0.0.1:3128"
        );

        let missing = AppConfig {
            fallback: FallbackConfig::Saved {
                name: "nope".to_string(),
            },
            ..AppConfig::default()
        };
        assert!(resolve_fallback_endpoint(&missing).is_none());

        let statik = AppConfig {
            fallback: FallbackConfig::Static {
                protocol: ProxyProtocol::Http,
                host: "proxy".to_string(),
                port: 8080,
                connect_timeout_ms: 50,
            },
            ..AppConfig::default()
        };
        assert_eq!(
            resolve_fallback_endpoint(&statik).unwrap().address(),
            "proxy:8080"
        );
    }

    #[test]
    fn gateway_poll_interval_defaults_outside_gateway_mode() {
        assert_eq!(gateway_poll_interval_secs(&UpstreamConfig::None), 5);
        assert_eq!(
            gateway_poll_interval_secs(&UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Http,
                port: 8080,
                poll_interval_secs: 42,
                connect_timeout_ms: 3_000,
            }),
            42
        );
    }

    #[test]
    fn only_the_direct_fallback_allows_direct_connections() {
        assert!(fallback_allows_direct(&FallbackConfig::Direct));
        assert!(!fallback_allows_direct(&FallbackConfig::None));
        assert!(!fallback_allows_direct(&FallbackConfig::Saved {
            name: "corp".to_string()
        }));
    }

    #[test]
    fn summarize_renders_every_field() {
        let config = AppConfig {
            upstream: UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Http,
                port: 8080,
                poll_interval_secs: 5,
                connect_timeout_ms: 3_000,
            },
            ..AppConfig::default()
        };

        assert_eq!(
            summarize(&config, Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))),
            "listen=127.0.0.1:8888 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1"
        );
        assert!(summarize(&config, None).ends_with("gateway=unknown"));
    }

    #[test]
    fn descriptions_cover_every_upstream_and_fallback_variant() {
        assert_eq!(describe_upstream(&UpstreamConfig::None), "none");
        assert_eq!(
            describe_upstream(&UpstreamConfig::Saved {
                name: "corp".to_string()
            }),
            "saved:corp"
        );
        assert_eq!(
            describe_upstream(&UpstreamConfig::Static {
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".to_string(),
                port: 1080,
                connect_timeout_ms: 3_000,
            }),
            "static:socks5:127.0.0.1:1080"
        );

        assert_eq!(describe_fallback(&FallbackConfig::None), "none");
        assert_eq!(describe_fallback(&FallbackConfig::Direct), "direct");
        assert_eq!(
            describe_fallback(&FallbackConfig::Saved {
                name: "corp".to_string()
            }),
            "saved:corp"
        );
        assert_eq!(
            describe_fallback(&FallbackConfig::Static {
                protocol: ProxyProtocol::Http,
                host: "proxy".to_string(),
                port: 8080,
                connect_timeout_ms: 3_000,
            }),
            "static:http:proxy:8080"
        );
    }

    #[test]
    fn wizard_helpers_extract_current_values() {
        let gateway = UpstreamConfig::Gateway {
            protocol: ProxyProtocol::Socks5,
            port: 1080,
            poll_interval_secs: 11,
            connect_timeout_ms: 3_000,
        };
        let statik = UpstreamConfig::Static {
            protocol: ProxyProtocol::Http,
            host: "proxy".to_string(),
            port: 8080,
            connect_timeout_ms: 3_000,
        };

        assert!(matches!(
            protocol_from_upstream(&gateway),
            ProxyProtocol::Socks5
        ));
        assert!(matches!(
            protocol_from_upstream(&UpstreamConfig::None),
            ProxyProtocol::Http
        ));
        assert_eq!(host_from_upstream(&statik).as_deref(), Some("proxy"));
        assert!(host_from_upstream(&gateway).is_none());
        assert_eq!(port_from_upstream(&gateway), Some(1080));
        assert!(port_from_upstream(&UpstreamConfig::None).is_none());
        assert_eq!(poll_interval_from_upstream(&gateway), Some(11));
        assert!(poll_interval_from_upstream(&statik).is_none());

        let fallback_static = FallbackConfig::Static {
            protocol: ProxyProtocol::Socks5,
            host: "proxy".to_string(),
            port: 8080,
            connect_timeout_ms: 3_000,
        };
        assert!(matches!(
            protocol_from_fallback(&fallback_static),
            ProxyProtocol::Socks5
        ));
        assert!(matches!(
            protocol_from_fallback(&FallbackConfig::Direct),
            ProxyProtocol::Http
        ));
        assert_eq!(
            host_from_fallback(&fallback_static).as_deref(),
            Some("proxy")
        );
        assert!(host_from_fallback(&FallbackConfig::None).is_none());
        assert_eq!(port_from_fallback(&fallback_static), Some(8080));
        assert!(port_from_fallback(&FallbackConfig::Direct).is_none());
    }

    #[test]
    fn kind_helpers_map_variants_to_labels() {
        assert_eq!(UpstreamKind::of(&UpstreamConfig::None), UpstreamKind::None);
        assert_eq!(
            UpstreamKind::of(&UpstreamConfig::Saved {
                name: "corp".to_string()
            }),
            UpstreamKind::Saved
        );
        assert_eq!(UpstreamKind::Gateway.label(), "gateway");
        assert_eq!(UpstreamKind::Static.label(), "static");
        assert_eq!(UpstreamKind::None.label(), "none");
        assert!(UpstreamKind::Saved.label().starts_with("saved"));

        assert_eq!(
            FallbackKind::of(&FallbackConfig::Direct),
            FallbackKind::Direct
        );
        assert_eq!(
            FallbackKind::of(&FallbackConfig::Static {
                protocol: ProxyProtocol::Http,
                host: "proxy".to_string(),
                port: 8080,
                connect_timeout_ms: 3_000,
            }),
            FallbackKind::Static
        );
        assert_eq!(FallbackKind::None.label(), "none");
        assert_eq!(FallbackKind::Direct.label(), "direct");
        assert_eq!(FallbackKind::Static.label(), "static");
        assert!(FallbackKind::Saved.label().starts_with("saved"));
    }

    #[test]
    fn describe_saved_proxy_renders_protocol_host_and_port() {
        assert_eq!(
            describe_saved_proxy(&static_proxy("corp", ProxyProtocol::Socks5)),
            "corp (socks5://10.0.0.1:3128)"
        );
    }

    #[test]
    fn protocol_name_maps_both_protocols() {
        assert_eq!(protocol_name(ProxyProtocol::Http), "http");
        assert_eq!(protocol_name(ProxyProtocol::Socks5), "socks5");
    }
}
