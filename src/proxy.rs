use std::{
    net::IpAddr,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use http::Uri;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time,
};
use tracing::{debug, info, warn};

use crate::{
    app::{SharedState, UpstreamFailureTracker},
    config::{self, AppConfig, ProxyEndpoint, ProxyProtocol},
    notify,
    stream::ProxyStream,
};

const MAX_HEADER_BYTES: usize = 64 * 1024;
const UPSTREAM_FAILURE_NOTIFY_THRESHOLD: u32 = 3;
const UPSTREAM_FAILURE_WINDOW: Duration = Duration::from_secs(20);
const UPSTREAM_FAILURE_NOTIFY_COOLDOWN: Duration = Duration::from_secs(60);

pub async fn serve(state: SharedState) -> Result<()> {
    let listen_addr = state.config.read().await.listen.socket_addr();
    let listener = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("no se pudo escuchar en {listen_addr}"))?;
    info!(listen = %listen_addr, "proxy listo");

    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => return Ok(()),
            accepted = listener.accept() => {
                let (socket, remote_addr) = accepted?;
                let request_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_client(socket, request_state).await {
                        warn!(client = %remote_addr, %error, "petición proxy fallida");
                    }
                });
            }
        }
    }
}

async fn handle_client(mut client: TcpStream, state: SharedState) -> Result<()> {
    let (request, buffered_body) = read_request_head(&mut client).await?;
    debug!(method = %request.method, target = %request.target, "petición recibida");

    if request.method.eq_ignore_ascii_case("CONNECT") {
        handle_connect(client, request, state).await
    } else {
        handle_http(client, request, buffered_body, state).await
    }
}

async fn handle_connect(
    mut client: TcpStream,
    request: HttpRequestHead,
    state: SharedState,
) -> Result<()> {
    let (routes, upstream_present) = resolve_routes(&state).await;
    let target = request.target.clone();
    let mut last_error = None;

    for (index, route) in routes.iter().enumerate() {
        match connect_tunnel(&route, &target).await {
            Ok(mut upstream) => {
                if upstream_present && index == 0 {
                    reset_upstream_failure_streak(&state).await;
                }
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await?;
                tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
                return Ok(());
            }
            Err(error) => {
                debug!(route = %describe_route(&route), %error, "falló intento CONNECT");
                if upstream_present && index == 0 {
                    maybe_notify_upstream_failure(&state, route, &error).await;
                }
                last_error = Some(error);
            }
        }
    }

    client
        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
        .await?;
    Err(last_error.unwrap_or_else(|| anyhow!("no se pudo abrir túnel CONNECT")))
}

async fn handle_http(
    mut client: TcpStream,
    request: HttpRequestHead,
    buffered_body: Vec<u8>,
    state: SharedState,
) -> Result<()> {
    let destination = extract_destination(&request)?;
    let (routes, upstream_present) = resolve_routes(&state).await;
    let mut last_error = None;

    for (index, route) in routes.iter().enumerate() {
        match connect_for_http(&route, &request, &destination).await {
            Ok((mut upstream, outbound_head)) => {
                if upstream_present && index == 0 {
                    reset_upstream_failure_streak(&state).await;
                }
                upstream.write_all(&outbound_head).await?;
                if !buffered_body.is_empty() {
                    upstream.write_all(&buffered_body).await?;
                }
                tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
                return Ok(());
            }
            Err(error) => {
                debug!(route = %describe_route(&route), %error, "falló intento HTTP");
                if upstream_present && index == 0 {
                    maybe_notify_upstream_failure(&state, route, &error).await;
                }
                last_error = Some(error);
            }
        }
    }

    client
        .write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
        .await?;
    Err(last_error.unwrap_or_else(|| anyhow!("no se pudo procesar la petición HTTP")))
}

async fn connect_tunnel(route: &Route, target: &str) -> Result<ProxyStream> {
    match route {
        Route::Direct => {
            let stream = connect_tcp(target, Duration::from_millis(3_000)).await?;
            Ok(ProxyStream::Tcp { inner: stream })
        }
        Route::Proxy(endpoint) => match endpoint.protocol {
            ProxyProtocol::Http => connect_http_proxy_tunnel(endpoint, target).await,
            ProxyProtocol::Socks5 => connect_socks5_proxy(endpoint, target).await,
        },
    }
}

async fn connect_for_http(
    route: &Route,
    request: &HttpRequestHead,
    destination: &Destination,
) -> Result<(ProxyStream, Vec<u8>)> {
    match route {
        Route::Direct => {
            let stream = connect_tcp(&destination.authority, Duration::from_millis(3_000)).await?;
            Ok((
                ProxyStream::Tcp { inner: stream },
                build_direct_request(request, destination),
            ))
        }
        Route::Proxy(endpoint) => match endpoint.protocol {
            ProxyProtocol::Http => {
                let stream = connect_tcp(&endpoint.address(), timeout_for(endpoint)).await?;
                Ok((
                    ProxyStream::Tcp { inner: stream },
                    build_http_proxy_request(request, destination),
                ))
            }
            ProxyProtocol::Socks5 => {
                let stream = connect_socks5_proxy(endpoint, &destination.authority).await?;
                Ok((stream, build_direct_request(request, destination)))
            }
        },
    }
}

async fn connect_http_proxy_tunnel(endpoint: &ProxyEndpoint, target: &str) -> Result<ProxyStream> {
    let mut stream = connect_tcp(&endpoint.address(), timeout_for(endpoint)).await?;
    let payload = format!(
        "CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Connection: Keep-Alive\r\n\r\n"
    );
    stream.write_all(payload.as_bytes()).await?;

    let response = read_http_response(&mut stream).await?;
    if !response.version.starts_with("HTTP/") {
        bail!(
            "respuesta CONNECT inválida del upstream proxy: {}",
            response.version
        );
    }
    if !(200..300).contains(&response.status) {
        bail!(
            "el upstream proxy rechazó CONNECT: {} {}",
            response.status,
            response.reason
        );
    }

    Ok(ProxyStream::Tcp { inner: stream })
}

async fn connect_socks5_proxy(endpoint: &ProxyEndpoint, target: &str) -> Result<ProxyStream> {
    let proxy_address = endpoint.address();
    let stream = time::timeout(
        timeout_for(endpoint),
        tokio_socks::tcp::Socks5Stream::connect(proxy_address.as_str(), target),
    )
    .await
    .context("timeout conectando a upstream socks5")??;
    Ok(ProxyStream::Socks5 { inner: stream })
}

async fn connect_tcp(address: &str, timeout: Duration) -> Result<TcpStream> {
    time::timeout(timeout, TcpStream::connect(address))
        .await
        .with_context(|| format!("timeout conectando a {address}"))?
        .with_context(|| format!("falló conexión a {address}"))
}

fn build_direct_request(request: &HttpRequestHead, destination: &Destination) -> Vec<u8> {
    let mut head = format!(
        "{} {} {}\r\n",
        request.method, destination.path, request.version
    );
    for (name, value) in &request.headers {
        if name.eq_ignore_ascii_case("proxy-connection") {
            continue;
        }
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    head.into_bytes()
}

fn build_http_proxy_request(request: &HttpRequestHead, destination: &Destination) -> Vec<u8> {
    let target = if request.target.contains("://") {
        request.target.clone()
    } else {
        format!("http://{}{}", destination.authority, destination.path)
    };

    let mut head = format!("{} {} {}\r\n", request.method, target, request.version);
    for (name, value) in &request.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    head.into_bytes()
}

fn extract_destination(request: &HttpRequestHead) -> Result<Destination> {
    if request.target.contains("://") {
        let uri: Uri = request.target.parse().context("URI inválida")?;
        let host = uri.host().ok_or_else(|| anyhow!("URI sin host"))?;
        let port = uri.port_u16().unwrap_or_else(|| {
            if uri.scheme_str() == Some("https") {
                443
            } else {
                80
            }
        });
        let path = uri
            .path_and_query()
            .map(|value| value.as_str().to_string())
            .unwrap_or_else(|| "/".to_string());
        return Ok(Destination {
            authority: format!("{host}:{port}"),
            path,
        });
    }

    let host_header = request
        .headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.clone())
        .ok_or_else(|| anyhow!("petición HTTP sin header Host"))?;

    let authority = if host_header.contains(':') {
        host_header
    } else {
        format!("{host_header}:80")
    };

    Ok(Destination {
        authority,
        path: request.target.clone(),
    })
}

async fn read_request_head(stream: &mut TcpStream) -> Result<(HttpRequestHead, Vec<u8>)> {
    read_http_head(stream).await
}

async fn read_http_head<S>(stream: &mut S) -> Result<(HttpRequestHead, Vec<u8>)>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            bail!("la conexión se cerró antes de completar los headers HTTP");
        }

        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            bail!("headers HTTP demasiado grandes");
        }

        if let Some(index) = find_header_end(&buffer) {
            let body = buffer[index..].to_vec();
            let head = parse_request_head(&buffer[..index])?;
            return Ok((head, body));
        }
    }
}

async fn read_http_response<S>(stream: &mut S) -> Result<HttpResponseHead>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            bail!("la conexión se cerró antes de completar la respuesta HTTP");
        }

        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_HEADER_BYTES {
            bail!("respuesta HTTP demasiado grande");
        }

        if let Some(index) = find_header_end(&buffer) {
            return parse_response_head(&buffer[..index]);
        }
    }
}

fn parse_request_head(bytes: &[u8]) -> Result<HttpRequestHead> {
    let text = std::str::from_utf8(bytes).context("headers HTTP no son UTF-8 válido")?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or_else(|| anyhow!("request line vacía"))?;
    let mut parts = request_line.split_whitespace();
    let first = parts.next().ok_or_else(|| anyhow!("request incompleta"))?;
    let second = parts.next().ok_or_else(|| anyhow!("request incompleta"))?;
    let third = parts.next().ok_or_else(|| anyhow!("request incompleta"))?;

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| anyhow!("header inválido: {line}"))?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }

    Ok(HttpRequestHead {
        method: first.to_string(),
        target: second.to_string(),
        version: third.to_string(),
        headers,
    })
}

fn parse_response_head(bytes: &[u8]) -> Result<HttpResponseHead> {
    let text = std::str::from_utf8(bytes).context("respuesta HTTP no es UTF-8 válida")?;
    let status_line = text
        .split("\r\n")
        .next()
        .ok_or_else(|| anyhow!("status line vacía"))?;
    let mut parts = status_line.split_whitespace();
    let version = parts
        .next()
        .ok_or_else(|| anyhow!("respuesta HTTP incompleta"))?;
    let status = parts
        .next()
        .ok_or_else(|| anyhow!("respuesta HTTP incompleta"))?;
    let reason = parts.collect::<Vec<_>>().join(" ");

    Ok(HttpResponseHead {
        version: version.to_string(),
        status: status.parse().context("status HTTP inválido")?,
        reason,
    })
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

async fn resolve_routes(state: &SharedState) -> (Vec<Route>, bool) {
    let config = state.config.read().await.clone();
    let gateway = *state.gateway_ip.read().await;
    let upstream_present = config::resolve_upstream_endpoint(&config, gateway).is_some();
    (resolve_routes_from_config(&config, gateway), upstream_present)
}

async fn maybe_notify_upstream_failure(state: &SharedState, route: &Route, error: &anyhow::Error) {
    let now = Instant::now();
    let (should_notify, consecutive) = {
        let mut tracker = state.upstream_failures.lock().await;
        let should_notify = record_upstream_failure(
            &mut tracker,
            now,
            UPSTREAM_FAILURE_NOTIFY_THRESHOLD,
            UPSTREAM_FAILURE_WINDOW,
            UPSTREAM_FAILURE_NOTIFY_COOLDOWN,
        );
        (should_notify, tracker.consecutive_failures)
    };

    if !should_notify {
        return;
    }

    let route_desc = describe_route(route);
    let message = format!(
        "{route_desc} falló {consecutive} veces seguidas. Último error: {error}"
    );

    let config = state.config.read().await;
    notify::notify(
        &config.notifications,
        "upstream no disponible",
        &message,
    );
    warn!(route = %route_desc, consecutive, %error, "upstream no disponible repetidamente");
}

async fn reset_upstream_failure_streak(state: &SharedState) {
    let mut tracker = state.upstream_failures.lock().await;
    clear_upstream_failures(&mut tracker);
}

fn clear_upstream_failures(tracker: &mut UpstreamFailureTracker) {
    tracker.consecutive_failures = 0;
    tracker.first_failure_at = None;
    tracker.last_failure_at = None;
}

fn record_upstream_failure(
    tracker: &mut UpstreamFailureTracker,
    now: Instant,
    threshold: u32,
    window: Duration,
    cooldown: Duration,
) -> bool {
    if let Some(last_failure_at) = tracker.last_failure_at
        && now.duration_since(last_failure_at) > window
    {
        clear_upstream_failures(tracker);
    }

    tracker.consecutive_failures = tracker.consecutive_failures.saturating_add(1);
    if tracker.first_failure_at.is_none() {
        tracker.first_failure_at = Some(now);
    }
    tracker.last_failure_at = Some(now);

    if tracker.consecutive_failures < threshold {
        return false;
    }

    if let Some(last_notified_at) = tracker.last_notified_at
        && now.duration_since(last_notified_at) < cooldown
    {
        return false;
    }

    tracker.last_notified_at = Some(now);
    true
}

fn resolve_routes_from_config(config: &AppConfig, gateway: Option<IpAddr>) -> Vec<Route> {
    let mut routes = Vec::new();

    if let Some(upstream) = config::resolve_upstream_endpoint(config, gateway) {
        routes.push(Route::Proxy(upstream));
    }

    if let Some(fallback) = config::resolve_fallback_endpoint(config) {
        routes.push(Route::Proxy(fallback));
    }

    if config::fallback_allows_direct(&config.fallback)
        || matches!(config.upstream, config::UpstreamConfig::None)
    {
        routes.push(Route::Direct);
    }

    routes
}

fn describe_route(route: &Route) -> String {
    match route {
        Route::Direct => "direct".to_string(),
        Route::Proxy(endpoint) => format!(
            "{}://{}",
            protocol_name(endpoint.protocol),
            endpoint.address()
        ),
    }
}

fn protocol_name(protocol: ProxyProtocol) -> &'static str {
    match protocol {
        ProxyProtocol::Http => "http",
        ProxyProtocol::Socks5 => "socks5",
    }
}

fn timeout_for(endpoint: &ProxyEndpoint) -> Duration {
    Duration::from_millis(endpoint.connect_timeout_ms.max(1))
}

#[derive(Debug, Clone)]
struct HttpRequestHead {
    method: String,
    target: String,
    version: String,
    headers: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct HttpResponseHead {
    version: String,
    status: u16,
    reason: String,
}

#[derive(Debug, Clone)]
struct Destination {
    authority: String,
    path: String,
}

#[derive(Debug, Clone)]
enum Route {
    Direct,
    Proxy(ProxyEndpoint),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FallbackConfig, SavedProxy, UpstreamConfig};
    use std::net::Ipv4Addr;

    fn request(method: &str, target: &str, headers: &[(&str, &str)]) -> HttpRequestHead {
        HttpRequestHead {
            method: method.to_string(),
            target: target.to_string(),
            version: "HTTP/1.1".to_string(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        }
    }

    fn endpoint(protocol: ProxyProtocol, host: &str, port: u16) -> ProxyEndpoint {
        ProxyEndpoint {
            protocol,
            host: host.to_string(),
            port,
            connect_timeout_ms: 3_000,
        }
    }

    #[test]
    fn header_end_is_detected_after_the_blank_line() {
        assert_eq!(find_header_end(b"GET / HTTP/1.1\r\n\r\nbody"), Some(18));
        assert!(find_header_end(b"GET / HTTP/1.1\r\n").is_none());
        assert!(find_header_end(b"").is_none());
    }

    #[test]
    fn request_heads_are_parsed_into_method_target_version_and_headers() {
        let head =
            parse_request_head(b"GET /index.html HTTP/1.1\r\nHost: example.com\r\nX-A:  b \r\n")
                .unwrap();

        assert_eq!(head.method, "GET");
        assert_eq!(head.target, "/index.html");
        assert_eq!(head.version, "HTTP/1.1");
        assert_eq!(
            head.headers,
            vec![
                ("Host".to_string(), "example.com".to_string()),
                ("X-A".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn incomplete_or_invalid_request_heads_are_rejected() {
        assert!(parse_request_head(b"GET /\r\n").is_err());
        assert!(parse_request_head(b"\r\n").is_err());
        assert!(parse_request_head(b"GET / HTTP/1.1\r\nbroken-header\r\n").is_err());
        assert!(parse_request_head(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn response_heads_are_parsed_into_version_status_and_reason() {
        let head = parse_response_head(b"HTTP/1.1 200 Connection Established\r\n").unwrap();

        assert_eq!(head.version, "HTTP/1.1");
        assert_eq!(head.status, 200);
        assert_eq!(head.reason, "Connection Established");
    }

    #[test]
    fn invalid_response_heads_are_rejected() {
        assert!(parse_response_head(b"HTTP/1.1\r\n").is_err());
        assert!(parse_response_head(b"HTTP/1.1 abc\r\n").is_err());
        assert!(parse_response_head(&[0xff, 0xfe]).is_err());
    }

    #[tokio::test]
    async fn reading_a_head_splits_the_buffered_body() {
        let mut stream: &[u8] = b"POST /submit HTTP/1.1\r\nHost: example.com\r\n\r\nname=value";

        let (head, body) = read_http_head(&mut stream).await.unwrap();

        assert_eq!(head.method, "POST");
        assert_eq!(body, b"name=value");
    }

    #[tokio::test]
    async fn reading_a_head_fails_when_the_connection_closes_early() {
        let mut stream: &[u8] = b"GET / HTTP/1.1\r\nHost: example.com\r\n";

        let error = read_http_head(&mut stream).await.unwrap_err();

        assert!(error.to_string().contains("se cerró antes de completar"));
    }

    #[tokio::test]
    async fn oversized_heads_are_rejected() {
        let payload = format!(
            "GET / HTTP/1.1\r\nX-Big: {}\r\n",
            "a".repeat(MAX_HEADER_BYTES)
        );
        let mut stream: &[u8] = payload.as_bytes();

        let error = read_http_head(&mut stream).await.unwrap_err();

        assert!(error.to_string().contains("demasiado grandes"));
    }

    #[tokio::test]
    async fn reading_a_response_head_fails_when_the_connection_closes_early() {
        let mut stream: &[u8] = b"HTTP/1.1 200 OK\r\n";

        let error = read_http_response(&mut stream).await.unwrap_err();

        assert!(error.to_string().contains("se cerró antes de completar"));
    }

    #[tokio::test]
    async fn oversized_responses_are_rejected() {
        let payload = format!(
            "HTTP/1.1 200 OK\r\nX-Big: {}\r\n",
            "a".repeat(MAX_HEADER_BYTES)
        );
        let mut stream: &[u8] = payload.as_bytes();

        let error = read_http_response(&mut stream).await.unwrap_err();

        assert!(error.to_string().contains("demasiado grande"));
    }

    #[test]
    fn absolute_form_targets_yield_authority_and_path() {
        let destination =
            extract_destination(&request("GET", "http://example.com/a?b=c", &[])).unwrap();

        assert_eq!(destination.authority, "example.com:80");
        assert_eq!(destination.path, "/a?b=c");
    }

    #[test]
    fn absolute_form_targets_default_to_the_scheme_port() {
        assert_eq!(
            extract_destination(&request("GET", "https://example.com/", &[]))
                .unwrap()
                .authority,
            "example.com:443"
        );
        assert_eq!(
            extract_destination(&request("GET", "http://example.com:1234/", &[]))
                .unwrap()
                .authority,
            "example.com:1234"
        );
    }

    #[test]
    fn absolute_form_targets_without_a_path_default_to_root() {
        assert_eq!(
            extract_destination(&request("GET", "http://example.com", &[]))
                .unwrap()
                .path,
            "/"
        );
    }

    #[test]
    fn origin_form_targets_use_the_host_header() {
        let destination =
            extract_destination(&request("GET", "/a", &[("Host", "example.com")])).unwrap();
        assert_eq!(destination.authority, "example.com:80");
        assert_eq!(destination.path, "/a");

        let explicit =
            extract_destination(&request("GET", "/a", &[("host", "example.com:1234")])).unwrap();
        assert_eq!(explicit.authority, "example.com:1234");
    }

    #[test]
    fn origin_form_targets_without_a_host_header_are_rejected() {
        let error = extract_destination(&request("GET", "/a", &[])).unwrap_err();

        assert!(error.to_string().contains("sin header Host"));
    }

    #[test]
    fn absolute_form_targets_without_a_host_are_rejected() {
        assert!(extract_destination(&request("GET", "file:///tmp/x", &[])).is_err());
    }

    #[test]
    fn direct_requests_use_origin_form_and_drop_proxy_headers() {
        let head = request(
            "GET",
            "http://example.com/a",
            &[("Host", "example.com"), ("Proxy-Connection", "Keep-Alive")],
        );
        let destination = extract_destination(&head).unwrap();

        let bytes = build_direct_request(&head, &destination);
        let text = String::from_utf8(bytes).unwrap();

        assert!(text.starts_with("GET /a HTTP/1.1\r\n"));
        assert!(text.contains("Host: example.com\r\n"));
        assert!(!text.to_lowercase().contains("proxy-connection"));
        assert!(text.ends_with("\r\n\r\n"));
    }

    #[test]
    fn proxied_requests_keep_the_absolute_form_target() {
        let head = request("GET", "http://example.com/a", &[("Host", "example.com")]);
        let destination = extract_destination(&head).unwrap();

        let text = String::from_utf8(build_http_proxy_request(&head, &destination)).unwrap();

        assert!(text.starts_with("GET http://example.com/a HTTP/1.1\r\n"));
    }

    #[test]
    fn proxied_requests_rebuild_the_absolute_form_from_origin_form() {
        let head = request("GET", "/a", &[("Host", "example.com")]);
        let destination = extract_destination(&head).unwrap();

        let text = String::from_utf8(build_http_proxy_request(&head, &destination)).unwrap();

        assert!(text.starts_with("GET http://example.com:80/a HTTP/1.1\r\n"));
    }

    #[test]
    fn default_config_only_routes_direct() {
        let routes = resolve_routes_from_config(&AppConfig::default(), None);

        assert_eq!(routes.len(), 1);
        assert_eq!(describe_route(&routes[0]), "direct");
    }

    #[test]
    fn gateway_upstream_is_skipped_until_the_gateway_is_known() {
        let config = AppConfig {
            upstream: UpstreamConfig::Gateway {
                protocol: ProxyProtocol::Http,
                port: 1234,
                poll_interval_secs: 5,
                connect_timeout_ms: 3_000,
            },
            ..AppConfig::default()
        };

        let without_gateway = resolve_routes_from_config(&config, None);
        assert_eq!(
            without_gateway
                .iter()
                .map(describe_route)
                .collect::<Vec<_>>(),
            vec!["direct"]
        );

        let with_gateway =
            resolve_routes_from_config(&config, Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
        assert_eq!(
            with_gateway.iter().map(describe_route).collect::<Vec<_>>(),
            vec!["http://192.168.1.1:1234", "direct"]
        );
    }

    #[test]
    fn upstream_fallback_and_direct_are_tried_in_order() {
        let config = AppConfig {
            upstream: UpstreamConfig::Saved {
                name: "corp".to_string(),
            },
            fallback: FallbackConfig::Static {
                protocol: ProxyProtocol::Socks5,
                host: "127.0.0.1".to_string(),
                port: 1080,
                connect_timeout_ms: 3_000,
            },
            proxies: vec![SavedProxy {
                name: "corp".to_string(),
                protocol: ProxyProtocol::Http,
                host: "10.0.0.1".to_string(),
                port: 3128,
                connect_timeout_ms: 3_000,
            }],
            ..AppConfig::default()
        };

        let routes = resolve_routes_from_config(&config, None);

        assert_eq!(
            routes.iter().map(describe_route).collect::<Vec<_>>(),
            vec!["http://10.0.0.1:3128", "socks5://127.0.0.1:1080"]
        );
    }

    #[test]
    fn a_static_upstream_without_fallback_has_no_direct_route() {
        let config = AppConfig {
            upstream: UpstreamConfig::Static {
                protocol: ProxyProtocol::Http,
                host: "10.0.0.1".to_string(),
                port: 3128,
                connect_timeout_ms: 3_000,
            },
            fallback: FallbackConfig::None,
            ..AppConfig::default()
        };

        let routes = resolve_routes_from_config(&config, None);

        assert_eq!(
            routes.iter().map(describe_route).collect::<Vec<_>>(),
            vec!["http://10.0.0.1:3128"]
        );
    }

    #[tokio::test]
    async fn routes_are_resolved_from_the_shared_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = crate::testing::state(crate::testing::paths(dir.path()), AppConfig::default());

        let (routes, upstream_present) = resolve_routes(&state).await;

        assert_eq!(routes.len(), 1);
        assert!(!upstream_present);
        assert!(matches!(routes[0], Route::Direct));
    }

    #[test]
    fn upstream_failure_streak_notifies_after_threshold() {
        let mut tracker = UpstreamFailureTracker::default();
        let start = Instant::now();

        assert!(!record_upstream_failure(
            &mut tracker,
            start,
            3,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert!(!record_upstream_failure(
            &mut tracker,
            start + Duration::from_secs(1),
            3,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert!(record_upstream_failure(
            &mut tracker,
            start + Duration::from_secs(2),
            3,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
    }

    #[test]
    fn upstream_failure_notification_respects_cooldown() {
        let mut tracker = UpstreamFailureTracker::default();
        let start = Instant::now();

        assert!(record_upstream_failure(
            &mut tracker,
            start,
            1,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert!(!record_upstream_failure(
            &mut tracker,
            start + Duration::from_secs(5),
            1,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert!(record_upstream_failure(
            &mut tracker,
            start + Duration::from_secs(61),
            1,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
    }

    #[test]
    fn upstream_failure_streak_resets_after_window() {
        let mut tracker = UpstreamFailureTracker::default();
        let start = Instant::now();

        assert!(!record_upstream_failure(
            &mut tracker,
            start,
            3,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert!(!record_upstream_failure(
            &mut tracker,
            start + Duration::from_secs(30),
            3,
            Duration::from_secs(20),
            Duration::from_secs(60),
        ));
        assert_eq!(tracker.consecutive_failures, 1);
    }

    #[test]
    fn connect_timeouts_are_never_zero() {
        assert_eq!(
            timeout_for(&endpoint(ProxyProtocol::Http, "127.0.0.1", 1234)),
            Duration::from_millis(3_000)
        );

        let mut instant = endpoint(ProxyProtocol::Http, "127.0.0.1", 1234);
        instant.connect_timeout_ms = 0;
        assert_eq!(timeout_for(&instant), Duration::from_millis(1));
    }

    #[test]
    fn routes_are_rendered_for_logging() {
        assert_eq!(describe_route(&Route::Direct), "direct");
        assert_eq!(
            describe_route(&Route::Proxy(endpoint(
                ProxyProtocol::Socks5,
                "127.0.0.1",
                1080
            ))),
            "socks5://127.0.0.1:1080"
        );
        assert_eq!(protocol_name(ProxyProtocol::Http), "http");
        assert_eq!(protocol_name(ProxyProtocol::Socks5), "socks5");
    }
}
