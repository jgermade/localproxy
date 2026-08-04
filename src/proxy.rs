use std::{net::IpAddr, time::Duration};

use anyhow::{Context, Result, anyhow, bail};
use http::Uri;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time,
};
use tracing::{debug, info, warn};

use crate::{
    app::SharedState,
    config::{self, AppConfig, ProxyEndpoint, ProxyProtocol},
    stream::ProxyStream,
};

const MAX_HEADER_BYTES: usize = 64 * 1024;

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
    let routes = resolve_routes(&state).await;
    let target = request.target.clone();
    let mut last_error = None;

    for route in routes {
        match connect_tunnel(&route, &target).await {
            Ok(mut upstream) => {
                client
                    .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                    .await?;
                tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
                return Ok(());
            }
            Err(error) => {
                debug!(route = %describe_route(&route), %error, "falló intento CONNECT");
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
    let routes = resolve_routes(&state).await;
    let mut last_error = None;

    for route in routes {
        match connect_for_http(&route, &request, &destination).await {
            Ok((mut upstream, outbound_head)) => {
                upstream.write_all(&outbound_head).await?;
                if !buffered_body.is_empty() {
                    upstream.write_all(&buffered_body).await?;
                }
                tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
                return Ok(());
            }
            Err(error) => {
                debug!(route = %describe_route(&route), %error, "falló intento HTTP");
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

async fn resolve_routes(state: &SharedState) -> Vec<Route> {
    let config = state.config.read().await.clone();
    let gateway = *state.gateway_ip.read().await;
    resolve_routes_from_config(&config, gateway)
}

fn resolve_routes_from_config(config: &AppConfig, gateway: Option<IpAddr>) -> Vec<Route> {
    let mut routes = Vec::new();

    if let Some(upstream) = config::resolve_upstream_endpoint(&config.upstream, gateway) {
        routes.push(Route::Proxy(upstream));
    }

    if let Some(fallback) = config::resolve_fallback_endpoint(&config.fallback) {
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
