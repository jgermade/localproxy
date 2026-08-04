//! Configuration: paths, defaults, TOML round trips and endpoint resolution.

use std::{
    fs,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
};

use localproxy::config::{
    AppConfig, AppPaths, FallbackConfig, ListenConfig, NotificationsConfig, ProxyProtocol,
    SavedProxy, UpstreamConfig, fallback_allows_direct, gateway_poll_interval_secs, load_or_create,
    resolve_fallback_endpoint, resolve_upstream_endpoint, save, summarize,
};

fn paths_in(dir: &Path) -> AppPaths {
    localproxy::testing::paths(dir)
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
    let paths = paths_in(Path::new("/tmp/localproxy-test"));

    assert_eq!(
        paths.control_socket(),
        paths.state_dir.join("localproxy.sock")
    );
    assert_eq!(paths.pid_file(), paths.state_dir.join("localproxy.pid"));
    assert_eq!(paths.lock_file(), paths.state_dir.join("localproxy.lock"));
    assert_eq!(paths.log_file(), paths.state_dir.join("localproxy.log"));
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
    assert_eq!(config.listen.port, 1234);
    assert_eq!(
        config.listen.socket_addr(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234)
    );
    assert!(matches!(config.upstream, UpstreamConfig::None));
    assert!(matches!(config.fallback, FallbackConfig::Direct));
    assert!(config.notifications.enabled);
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

    assert_eq!(config.listen.port, 1234);
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
        notifications: NotificationsConfig {
            enabled: false,
            icon: Some("/tmp/custom-icon.png".to_string()),
        },
        proxies: vec![static_proxy("corp", ProxyProtocol::Socks5)],
    };

    let serialized = toml::to_string_pretty(&config).unwrap();
    let parsed: AppConfig = toml::from_str(&serialized).unwrap();

    assert_eq!(parsed.listen.port, 1234);
    assert!(matches!(
        parsed.upstream,
        UpstreamConfig::Gateway {
            protocol: ProxyProtocol::Socks5,
            port: 1080,
            poll_interval_secs: 30,
            connect_timeout_ms: 250,
        }
    ));
    assert!(matches!(
        &parsed.fallback,
        FallbackConfig::Static {
            protocol: ProxyProtocol::Http,
            host,
            port: 8080,
            connect_timeout_ms: 400,
        } if host == "proxy.example.com"
    ));
    assert_eq!(parsed.proxies.len(), 1);
    assert_eq!(parsed.proxies[0].name, "corp");
    assert_eq!(parsed.proxies[0].connect_timeout_ms, 1_500);
    assert!(!parsed.notifications.enabled);
    assert_eq!(
        parsed.notifications.icon.as_deref(),
        Some("/tmp/custom-icon.png")
    );
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
    assert!(config.notifications.enabled);
}

#[test]
fn notifications_can_be_disabled_from_the_config_file() {
    let raw = r#"
        [notifications]
        enabled = false
    "#;

    let config: AppConfig = toml::from_str(raw).unwrap();

    assert!(!config.notifications.enabled);
    assert!(config.notifications.icon.is_none());
}

#[test]
fn the_notification_icon_is_omitted_when_unset() {
    let serialized = toml::to_string_pretty(&AppConfig::default()).unwrap();

    assert!(!serialized.contains("icon"));
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
    assert_eq!(config.listen.port, 1234);
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

    assert!(matches!(&loaded.upstream, UpstreamConfig::Saved { name } if name == "corp"));
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
        "listen=127.0.0.1:1234 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1"
    );
    assert!(summarize(&config, None).ends_with("gateway=unknown"));
}
