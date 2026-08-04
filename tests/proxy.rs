//! End-to-end proxying: direct routing, CONNECT tunnels, upstream chaining and failover.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use localproxy::{
    app::SharedState,
    config::{AppConfig, FallbackConfig, ListenConfig, ProxyProtocol, UpstreamConfig},
    proxy,
};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

async fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    listener.local_addr().unwrap().port()
}

/// Minimal origin server: answers every request with its own request line.
async fn spawn_origin() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = vec![0_u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let body = text.lines().next().unwrap_or("").to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });

    addr
}

/// Fake HTTP upstream proxy: records request lines and answers 200 (or 403 CONNECT).
async fn spawn_fake_http_proxy(accept_connect: bool) -> (SocketAddr, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = seen.clone();

    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let recorder = recorder.clone();
            tokio::spawn(async move {
                let mut buffer = vec![0_u8; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&buffer[..read]).to_string();
                let request_line = text.lines().next().unwrap_or("").to_string();
                recorder.lock().unwrap().push(request_line.clone());

                let response = if request_line.starts_with("CONNECT") {
                    if accept_connect {
                        "HTTP/1.1 200 Connection Established\r\n\r\n".to_string()
                    } else {
                        "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_string()
                    }
                } else {
                    let body = format!("via-proxy {request_line}");
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    )
                };
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });

    (addr, seen)
}

async fn spawn_proxy(config: AppConfig) -> (SharedState, SocketAddr, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let paths = localproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();
    let listen = config.listen.socket_addr();
    let state = localproxy::testing::state(paths, config);

    let server_state = state.clone();
    tokio::spawn(async move { proxy::serve(server_state).await });

    for _ in 0..200 {
        if TcpStream::connect(listen).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    (state, listen, dir)
}

fn config_listening_on(port: u16) -> AppConfig {
    AppConfig {
        listen: ListenConfig {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port,
        },
        ..AppConfig::default()
    }
}

async fn send_through_proxy(proxy: SocketAddr, payload: &str) -> String {
    let mut client = TcpStream::connect(proxy).await.unwrap();
    client.write_all(payload.as_bytes()).await.unwrap();

    let mut response = Vec::new();
    timeout(TEST_TIMEOUT, client.read_to_end(&mut response))
        .await
        .expect("timeout leyendo la respuesta del proxy")
        .unwrap();
    String::from_utf8_lossy(&response).to_string()
}

#[tokio::test]
async fn plain_http_requests_are_forwarded_directly() {
    let origin = spawn_origin().await;
    let (state, proxy_addr, _dir) = spawn_proxy(config_listening_on(free_port().await)).await;

    let response = send_through_proxy(
        proxy_addr,
        &format!(
            "GET http://{origin}/hello HTTP/1.1\r\nHost: {origin}\r\nProxy-Connection: Keep-Alive\r\n\r\n"
        ),
    )
    .await;

    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.contains("GET /hello HTTP/1.1"));
    state.shutdown.cancel();
}

#[tokio::test]
async fn https_requests_are_tunnelled_with_connect() {
    let origin = spawn_origin().await;
    let (state, proxy_addr, _dir) = spawn_proxy(config_listening_on(free_port().await)).await;

    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
    client
        .write_all(format!("CONNECT {origin} HTTP/1.1\r\nHost: {origin}\r\n\r\n").as_bytes())
        .await
        .unwrap();

    let mut established = [0_u8; 39];
    timeout(TEST_TIMEOUT, client.read_exact(&mut established))
        .await
        .expect("timeout esperando la respuesta CONNECT")
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&established),
        "HTTP/1.1 200 Connection Established\r\n\r\n"
    );

    client
        .write_all(b"GET /tunnelled HTTP/1.1\r\nHost: origin\r\n\r\n")
        .await
        .unwrap();

    let mut tunnelled = Vec::new();
    timeout(TEST_TIMEOUT, client.read_to_end(&mut tunnelled))
        .await
        .expect("timeout leyendo por el túnel")
        .unwrap();

    assert!(String::from_utf8_lossy(&tunnelled).contains("GET /tunnelled HTTP/1.1"));
    state.shutdown.cancel();
}

#[tokio::test]
async fn requests_go_through_the_configured_http_upstream() {
    let (upstream, seen) = spawn_fake_http_proxy(true).await;
    let mut config = config_listening_on(free_port().await);
    config.upstream = UpstreamConfig::Static {
        protocol: ProxyProtocol::Http,
        host: upstream.ip().to_string(),
        port: upstream.port(),
        connect_timeout_ms: 3_000,
    };
    config.fallback = FallbackConfig::None;
    let (state, proxy_addr, _dir) = spawn_proxy(config).await;

    let response = send_through_proxy(
        proxy_addr,
        "GET http://example.com/a HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;

    assert!(response.contains("via-proxy GET http://example.com/a HTTP/1.1"));
    assert_eq!(
        seen.lock().unwrap().first().unwrap(),
        "GET http://example.com/a HTTP/1.1"
    );
    state.shutdown.cancel();
}

#[tokio::test]
async fn connect_is_tunnelled_through_the_configured_http_upstream() {
    let (upstream, seen) = spawn_fake_http_proxy(true).await;
    let mut config = config_listening_on(free_port().await);
    config.upstream = UpstreamConfig::Static {
        protocol: ProxyProtocol::Http,
        host: upstream.ip().to_string(),
        port: upstream.port(),
        connect_timeout_ms: 3_000,
    };
    config.fallback = FallbackConfig::None;
    let (state, proxy_addr, _dir) = spawn_proxy(config).await;

    let response = send_through_proxy(
        proxy_addr,
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
    )
    .await;

    assert!(response.starts_with("HTTP/1.1 200 Connection Established"));
    assert_eq!(
        seen.lock().unwrap().first().unwrap(),
        "CONNECT example.com:443 HTTP/1.1"
    );
    state.shutdown.cancel();
}

#[tokio::test]
async fn a_rejected_connect_upstream_falls_back_to_502() {
    let (upstream, _seen) = spawn_fake_http_proxy(false).await;
    let mut config = config_listening_on(free_port().await);
    config.upstream = UpstreamConfig::Static {
        protocol: ProxyProtocol::Http,
        host: upstream.ip().to_string(),
        port: upstream.port(),
        connect_timeout_ms: 3_000,
    };
    config.fallback = FallbackConfig::None;
    let (state, proxy_addr, _dir) = spawn_proxy(config).await;

    let response = send_through_proxy(
        proxy_addr,
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
    )
    .await;

    assert!(response.starts_with("HTTP/1.1 502 Bad Gateway"));
    state.shutdown.cancel();
}

#[tokio::test]
async fn an_unreachable_upstream_falls_back_to_the_direct_route() {
    let origin = spawn_origin().await;
    let dead_port = free_port().await;
    let mut config = config_listening_on(free_port().await);
    config.upstream = UpstreamConfig::Static {
        protocol: ProxyProtocol::Http,
        host: "127.0.0.1".to_string(),
        port: dead_port,
        connect_timeout_ms: 200,
    };
    config.fallback = FallbackConfig::Direct;
    let (state, proxy_addr, _dir) = spawn_proxy(config).await;

    let response = send_through_proxy(
        proxy_addr,
        &format!("GET http://{origin}/recovered HTTP/1.1\r\nHost: {origin}\r\n\r\n"),
    )
    .await;

    assert!(response.contains("GET /recovered HTTP/1.1"));
    state.shutdown.cancel();
}

#[tokio::test]
async fn requests_without_any_working_route_return_502() {
    let dead_port = free_port().await;
    let mut config = config_listening_on(free_port().await);
    config.upstream = UpstreamConfig::Static {
        protocol: ProxyProtocol::Http,
        host: "127.0.0.1".to_string(),
        port: dead_port,
        connect_timeout_ms: 200,
    };
    config.fallback = FallbackConfig::None;
    let (state, proxy_addr, _dir) = spawn_proxy(config).await;

    let response = send_through_proxy(
        proxy_addr,
        "GET http://example.com/a HTTP/1.1\r\nHost: example.com\r\n\r\n",
    )
    .await;

    assert!(response.starts_with("HTTP/1.1 502 Bad Gateway"));
    state.shutdown.cancel();
}

#[tokio::test]
async fn serve_fails_when_the_listen_address_is_already_taken() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let taken = listener.local_addr().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = localproxy::testing::state(
        localproxy::testing::paths(dir.path()),
        config_listening_on(taken.port()),
    );

    let error = proxy::serve(state).await.unwrap_err();

    assert!(error.to_string().contains("no se pudo escuchar"));
}

#[tokio::test]
async fn serve_stops_when_the_shutdown_token_is_cancelled() {
    let (state, _addr, _dir) = spawn_proxy(config_listening_on(free_port().await)).await;

    state.shutdown.cancel();

    let stopped = timeout(TEST_TIMEOUT, state.shutdown.cancelled()).await;
    assert!(stopped.is_ok());
}
