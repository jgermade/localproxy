# Configuration

localproxy stores its configuration as TOML at `~/.config/localproxy/config.toml`.

Run the interactive wizard at any time:

```bash
localproxy config
```

The wizard saves the file and, if the daemon is running, hot-reloads the configuration without restarting the process. To reload manually:

```bash
localproxy reload
```

---

# The `localproxy config` wizard

## How it works

`localproxy config` is a fully interactive terminal wizard:

1. It loads the current config (or creates a default one if none exists).
2. Every prompt is **pre-filled with the current value**, so pressing <kbd>Enter</kbd> keeps it.
3. Once finished, it writes `~/.config/localproxy/config.toml`.
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
| 2 | `Listen port` | text | always | current value (`1234`) |
| 3 | `Proxies guardados` | menu: saved proxies + `+ añadir proxy` / `- eliminar proxy` / `continuar` | always | `continuar` |
| 4 | `Nombre del proxy`, `Proxy protocol`, `Host`, `Puerto`, `Connect timeout (ms)` | text + list | adding or editing a saved proxy | current values |
| 5 | `Upstream type` | list: `none` / `gateway` / `saved` / `static` | always (`saved` only when the list is not empty) | current type |
| 6 | `Proxy protocol` | list: `http` / `socks5` | upstream is `gateway` or `static` | current protocol |
| 7 | `Gateway upstream port` | text | upstream is `gateway` | current or `8080` |
| 8 | `Gateway poll interval (seconds)` | text | upstream is `gateway` | current or `5` |
| 9 | `Upstream proxy` | list of saved proxies | upstream is `saved` | current selection |
| 10 | `Static upstream host` | text | upstream is `static` | current or `127.0.0.1` |
| 11 | `Static upstream port` | text | upstream is `static` | current or `8080` |
| 12 | `Fallback type` | list: `none` / `direct` / `saved` / `static` | always (`saved` only when the list is not empty) | current type |
| 13 | `Proxy protocol` | list: `http` / `socks5` | fallback is `static` | current protocol |
| 14 | `Fallback proxy` | list of saved proxies | fallback is `saved` | current selection |
| 15 | `Fallback host` | text | fallback is `static` | current or `127.0.0.1` |
| 16 | `Fallback port` | text | fallback is `static` | current or `8080` |
| 17 | `Desktop notifications` | confirm: `y` / `n` | always | current value (`y`) |

> `connect_timeout_ms` is only asked for saved proxies. For `gateway` and `static` entries it is always written as `3000`. Edit the TOML file manually to change it.

## Managing the saved proxy list

The `Proxies guardados` step is a menu that loops until you pick `continuar`:

```text
? Proxies guardados (edita, añade o continúa) ›
  corp (http://proxy-a.internal:8080)
  tunnel (socks5://127.0.0.1:1080)
  + añadir proxy
  - eliminar proxy
❯ continuar
```

- Selecting an existing entry re-opens its prompts so you can edit it.
- `+ añadir proxy` appends a new entry; reusing an existing name overwrites that entry.
- `- eliminar proxy` asks which entry to delete.

Once the list has at least one entry, `saved` becomes available as an upstream and fallback type, so you can switch between proxies without retyping host and port.

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
? Listen port (1234) ›
```

```text
✔ Listen port · 1234
```

## Full session examples

### Example 1 — Direct mode (no upstream)

Simplest setup: localproxy listens locally and connects straight to the destination.

```console
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:1234 upstream=none fallback=direct gateway=unknown
```

Resulting `~/.config/localproxy/config.toml`:

```toml
[listen]
host = "127.0.0.1"
port = 1234

[upstream]
type = "none"

[fallback]
type = "direct"
```

### Example 2 — Gateway upstream with direct fallback

Routes traffic through an HTTP proxy running on the current default gateway, and falls back to a direct connection when that proxy is unreachable (for example, off the corporate network).

```console
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · gateway
✔ Proxy protocol · http
✔ Gateway upstream port · 8080
✔ Gateway poll interval (seconds) · 5
✔ Fallback type · direct
reloaded: listen=127.0.0.1:1234 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1
```

Note that the last line already shows the detected gateway (`192.168.1.1`), so the effective upstream is `192.168.1.1:8080`.

Resulting config:

```toml
[listen]
host = "127.0.0.1"
port = 1234

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
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · static
✔ Proxy protocol · socks5
✔ Static upstream host · 127.0.0.1
✔ Static upstream port · 1080
✔ Fallback type · none
reloaded: listen=127.0.0.1:1234 upstream=static:socks5:127.0.0.1:1080 fallback=none gateway=unknown
```

Resulting config:

```toml
[listen]
host = "127.0.0.1"
port = 1234

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
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · static
✔ Proxy protocol · http
✔ Static upstream host · proxy-a.internal
✔ Static upstream port · 8080
✔ Fallback type · static
✔ Proxy protocol · http
✔ Fallback host · proxy-b.internal
✔ Fallback port · 8080
reloaded: listen=127.0.0.1:1234 upstream=static:http:proxy-a.internal:8080 fallback=static:http:proxy-b.internal:8080 gateway=unknown
```

### Example 5 — Running the wizard when the daemon is stopped

The config is still written to disk; only the live reload is skipped.

```console
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · none
✔ Fallback type · direct
config saved; daemon not notified: No such file or directory (os error 2)
```

Start the daemon afterwards to apply it:

```bash
localproxy start
```

### Example 6 — Changing only the listen port

Every prompt is pre-filled, so you only touch what you need. Press <kbd>Enter</kbd> on everything else.

```console
$ localproxy config
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
> localproxy service restart   # or: localproxy stop && localproxy start
> ```

## Aborting the wizard

Pressing <kbd>Ctrl</kbd>+<kbd>C</kbd> at any prompt exits without writing anything. The existing config file is left untouched.

## Editing the file by hand

The wizard is optional; `config.toml` can be edited directly. Apply the changes with:

```bash
localproxy reload
```

---

# Configuration file reference

## Schema overview

```toml
[listen]
host = "127.0.0.1"
port = 1234

[upstream]
type = "none"

[fallback]
type = "direct"

[notifications]
enabled = true

[[proxy]]
name = "corp"
protocol = "http"
host = "proxy-a.internal"
port = 8080
connect_timeout_ms = 3000
```

## [listen]

| Field | Type | Default | Description |
|---|---|---|---|
| `host` | IP string | `"127.0.0.1"` | Local bind address. |
| `port` | integer | `1234` | Local bind port. |

## [notifications]

Desktop notifications for daemon events: startup, shutdown, config reload and gateway changes.

| Field | Type | Default | Description |
|---|---|---|---|
| `enabled` | boolean | `true` | Set to `false` to silence every notification. |

```toml
[notifications]
enabled = false
```

> Notifications are best effort: if the desktop notification service is unavailable the failure is only logged at debug level and the proxy keeps running. No administrator privileges are required, but macOS asks the user to allow notifications the first time one is posted.

## [[proxy]]

Optional list of saved proxies. Each entry is a named endpoint that `upstream` and `fallback` can reference by name with `type = "saved"`.

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | — | Unique identifier used by `type = "saved"`. |
| `protocol` | `"http"` \| `"socks5"` | `"http"` | Proxy protocol. |
| `host` | string | — | Hostname or IP. |
| `port` | integer | — | Proxy port. |
| `connect_timeout_ms` | integer | `3000` | Connection timeout in milliseconds. |

```toml
[[proxy]]
name = "corp"
protocol = "http"
host = "proxy-a.internal"
port = 8080
connect_timeout_ms = 3000

[[proxy]]
name = "tunnel"
protocol = "socks5"
host = "127.0.0.1"
port = 1080
connect_timeout_ms = 3000
```

> If `type = "saved"` points to a name that no longer exists in the list, that route is skipped as if it were unreachable.

## [upstream]

Four types are supported: `none`, `gateway`, `saved`, `static`.

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

### upstream = saved

Uses one of the entries from the `[[proxy]]` list.

| Field | Type | Description |
|---|---|---|
| `name` | string | `name` of a `[[proxy]]` entry. |

```toml
[upstream]
type = "saved"
name = "corp"
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

Four types are supported: `none`, `direct`, `saved`, `static`.

### fallback = none

If the upstream fails, the connection is terminated with an error.

```toml
[fallback]
type = "none"
```

### fallback = direct

If the upstream fails, localproxy attempts a direct connection to the destination.

```toml
[fallback]
type = "direct"
```

### fallback = saved

If the upstream fails, localproxy tries an entry from the `[[proxy]]` list.

| Field | Type | Description |
|---|---|---|
| `name` | string | `name` of a `[[proxy]]` entry. |

```toml
[fallback]
type = "saved"
name = "tunnel"
```

### fallback = static

If the upstream fails, localproxy tries a second fixed proxy.

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
2. Try the fallback proxy (if configured as `saved` or `static`).
3. Attempt a direct connection if `fallback = "direct"` or no upstream is configured.
4. Return `502 Bad Gateway` to the client if nothing succeeds.

## Complete examples

### Direct mode

No upstream, direct fallback. Useful for testing connectivity without a proxy.

```toml
[listen]
host = "127.0.0.1"
port = 1234

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
port = 1234

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
port = 1234

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
