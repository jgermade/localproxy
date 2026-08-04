# Configuration

zproxy stores its configuration as TOML at `~/.config/zproxy/config.toml`.

Run the interactive wizard at any time:

```bash
zproxy config
```

The wizard saves the file and, if the daemon is running, hot-reloads the configuration without restarting the process. To reload manually:

```bash
zproxy reload
```

## Schema overview

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "none"

[fallback]
type = "direct"
```

## [listen]

| Field | Type | Default | Description |
|---|---|---|---|
| `host` | IP string | `"127.0.0.1"` | Local bind address. |
| `port` | integer | `8888` | Local bind port. |

## [upstream]

Three types are supported: `none`, `gateway`, `static`.

### upstream = none

No upstream proxy. Traffic is forwarded only if the fallback allows it.

```toml
[upstream]
type = "none"
```

### upstream = gateway

Builds the upstream address as `<default_gateway_ip>:<port>`. The daemon re-detects the gateway at `poll_interval_secs` intervals.

| Field | Type | Default | Description |
|---|---|---|---|
| `protocol` | `"http"` \| `"socks5"` | — | Upstream proxy protocol. |
| `port` | integer | — | Port on the gateway host. |
| `poll_interval_secs` | integer | — | Gateway re-detection interval. |
| `connect_timeout_ms` | integer | `3000` | Connection timeout in milliseconds. |

```toml
[upstream]
type = "gateway"
protocol = "http"
port = 8080
poll_interval_secs = 5
connect_timeout_ms = 3000
```

### upstream = static

Fixed upstream proxy address.

| Field | Type | Description |
|---|---|---|
| `protocol` | `"http"` \| `"socks5"` | Upstream proxy protocol. |
| `host` | string | Upstream hostname or IP. |
| `port` | integer | Upstream port. |
| `connect_timeout_ms` | integer | Connection timeout in milliseconds. |

```toml
[upstream]
type = "static"
protocol = "socks5"
host = "127.0.0.1"
port = 1080
connect_timeout_ms = 3000
```

## [fallback]

Three types are supported: `none`, `direct`, `static`.

### fallback = none

If the upstream fails, the connection is terminated with an error.

```toml
[fallback]
type = "none"
```

### fallback = direct

If the upstream fails, zproxy attempts a direct connection to the destination.

```toml
[fallback]
type = "direct"
```

### fallback = static

If the upstream fails, zproxy tries a second fixed proxy.

| Field | Type | Description |
|---|---|---|
| `protocol` | `"http"` \| `"socks5"` | Fallback proxy protocol. |
| `host` | string | Fallback hostname or IP. |
| `port` | integer | Fallback port. |
| `connect_timeout_ms` | integer | Connection timeout in milliseconds. |

```toml
[fallback]
type = "static"
protocol = "http"
host = "10.0.0.20"
port = 8080
connect_timeout_ms = 3000
```

## Resolution order

1. Try the primary upstream (if resolvable).
2. Try the static fallback (if configured as `static`).
3. Attempt a direct connection if `fallback = "direct"` or no upstream is configured.
4. Return `502 Bad Gateway` to the client if nothing succeeds.

## Complete examples

### Direct mode

No upstream, direct fallback. Useful for testing connectivity without a proxy.

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "none"

[fallback]
type = "direct"
```

### Gateway upstream with direct fallback

Route through whatever proxy the corporate network gateway exposes, fall back to direct when off-network.

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "gateway"
protocol = "http"
port = 8080
poll_interval_secs = 5
connect_timeout_ms = 3000

[fallback]
type = "direct"
```

### Static SOCKS5 upstream with static HTTP fallback

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "static"
protocol = "socks5"
host = "127.0.0.1"
port = 1080
connect_timeout_ms = 3000

[fallback]
type = "static"
protocol = "http"
host = "10.10.10.10"
port = 8080
connect_timeout_ms = 3000
```

## Notes

- The wizard always suggests `connect_timeout_ms = 3000`. Edit the TOML file directly to use a different value.
- There are no reachability checks or upstream authentication.
- Per-domain, per-CIDR and per-suffix bypass rules are not implemented.
