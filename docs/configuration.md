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

---

# The `zproxy config` wizard

## How it works

`zproxy config` is a fully interactive terminal wizard:

1. It loads the current config (or creates a default one if none exists).
2. Every prompt is **pre-filled with the current value**, so pressing <kbd>Enter</kbd> keeps it.
3. Once finished, it writes `~/.config/zproxy/config.toml`.
4. It then sends a `reload` command over the control socket, so a running daemon picks up the change immediately.

Controls:

| Key | Action |
|---|---|
| <kbd>Enter</kbd> | Accept the shown value / confirm the selection |
| <kbd>↑</kbd> <kbd>↓</kbd> | Move between options in a list |
| Type text | Replace the default value in an input field |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> | Abort without saving |

## Prompt reference

The wizard asks these questions, in order. Some prompts only appear depending on previous answers.

| # | Prompt | Type | Shown when | Default |
|---|---|---|---|---|
| 1 | `Listen host` | text | always | current value (`127.0.0.1`) |
| 2 | `Listen port` | text | always | current value (`8888`) |
| 3 | `Upstream type` | list: `none` / `gateway` / `static` | always | current type |
| 4 | `Proxy protocol` | list: `http` / `socks5` | upstream is `gateway` or `static` | current protocol |
| 5 | `Gateway upstream port` | text | upstream is `gateway` | current or `8080` |
| 6 | `Gateway poll interval (seconds)` | text | upstream is `gateway` | current or `5` |
| 7 | `Static upstream host` | text | upstream is `static` | current or `127.0.0.1` |
| 8 | `Static upstream port` | text | upstream is `static` | current or `8080` |
| 9 | `Fallback type` | list: `none` / `direct` / `static` | always | current type |
| 10 | `Proxy protocol` | list: `http` / `socks5` | fallback is `static` | current protocol |
| 11 | `Fallback host` | text | fallback is `static` | current or `127.0.0.1` |
| 12 | `Fallback port` | text | fallback is `static` | current or `8080` |

> `connect_timeout_ms` is **not** asked by the wizard. It is always written as `3000`. Edit the TOML file manually to change it.

## What a list prompt looks like

While a list prompt is active, the highlighted entry is the current selection:

```text
? Upstream type ›
  none
❯ gateway
  static
```

After you press <kbd>Enter</kbd>, the line collapses into a confirmation:

```text
✔ Upstream type · gateway
```

Text inputs show the default in parentheses. Pressing <kbd>Enter</kbd> accepts it:

```text
? Listen port (8888) ›
```

```text
✔ Listen port · 8888
```

## Full session examples

### Example 1 — Direct mode (no upstream)

Simplest setup: zproxy listens locally and connects straight to the destination.

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

Resulting `~/.config/zproxy/config.toml`:

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "none"

[fallback]
type = "direct"
```

### Example 2 — Gateway upstream with direct fallback

Routes traffic through an HTTP proxy running on the current default gateway, and falls back to a direct connection when that proxy is unreachable (for example, off the corporate network).

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · gateway
✔ Proxy protocol · http
✔ Gateway upstream port · 8080
✔ Gateway poll interval (seconds) · 5
✔ Fallback type · direct
reloaded: listen=127.0.0.1:8888 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1
```

Note that the last line already shows the detected gateway (`192.168.1.1`), so the effective upstream is `192.168.1.1:8080`.

Resulting config:

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

### Example 3 — Static SOCKS5 upstream, no fallback

Forces all traffic through a local SOCKS5 proxy (for example an SSH tunnel opened with `ssh -D 1080`). With `fallback = none`, connections fail if the tunnel is down instead of leaking traffic directly.

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · static
✔ Proxy protocol · socks5
✔ Static upstream host · 127.0.0.1
✔ Static upstream port · 1080
✔ Fallback type · none
reloaded: listen=127.0.0.1:8888 upstream=static:socks5:127.0.0.1:1080 fallback=none gateway=unknown
```

Resulting config:

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
type = "none"
```

### Example 4 — Static upstream with a static fallback proxy

Two proxies: a primary one and a backup one.

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · static
✔ Proxy protocol · http
✔ Static upstream host · proxy-a.internal
✔ Static upstream port · 8080
✔ Fallback type · static
✔ Proxy protocol · http
✔ Fallback host · proxy-b.internal
✔ Fallback port · 8080
reloaded: listen=127.0.0.1:8888 upstream=static:http:proxy-a.internal:8080 fallback=static:http:proxy-b.internal:8080 gateway=unknown
```

### Example 5 — Running the wizard when the daemon is stopped

The config is still written to disk; only the live reload is skipped.

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · none
✔ Fallback type · direct
config saved; daemon not notified: No such file or directory (os error 2)
```

Start the daemon afterwards to apply it:

```bash
zproxy start
```

### Example 6 — Changing only the listen port

Every prompt is pre-filled, so you only touch what you need. Press <kbd>Enter</kbd> on everything else.

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 9999
✔ Upstream type · gateway
✔ Proxy protocol · http
✔ Gateway upstream port · 8080
✔ Gateway poll interval (seconds) · 5
✔ Fallback type · direct
reloaded: listen=127.0.0.1:9999 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1
```

> The listen address is bound at startup. A `reload` does **not** move the listener to the new port; restart the daemon:
>
> ```bash
> zproxy service restart   # or: zproxy stop && zproxy start
> ```

## Aborting the wizard

Pressing <kbd>Ctrl</kbd>+<kbd>C</kbd> at any prompt exits without writing anything. The existing config file is left untouched.

## Editing the file by hand

The wizard is optional; `config.toml` can be edited directly. Apply the changes with:

```bash
zproxy reload
```

---

# Configuration file reference

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
