# Rust Intermediary Proxy for macOS — Design

## Goal

A macOS application written in Rust that acts as:

1. **Local proxy server** (HTTP/HTTPS via `CONNECT`, optionally SOCKS5).
2. **Optional upstream proxy client**, with two resolution modes:
   - **Dynamic gateway**: `default_gateway_ip:<port>`, automatically detecting IP changes when switching networks.
   - **Static**: fixed host:port.
3. Configurable **fallback** when the primary upstream does not respond (another proxy, or a direct connection to the destination).

## Architecture

```
Local client → [App: proxy listening on 127.0.0.1:PORT]
                     │
                     ├─ try upstream (dynamic gateway or static)
                     │      └─ on failure → try fallback
                     │             └─ on failure → direct to destination (per config)
                     └─ if no upstream configured → direct to destination
```

Components:

- **Listener/proxy handler**: accepts connections, handles `CONNECT` for HTTPS (tunneling, no MITM initially).
- **Gateway detector**: independent background task that updates a shared state (`Arc<RwLock<Option<IpAddr>>>`) with the current gateway IP.
- **Upstream resolver**: for each new connection, decides which destination to connect to based on config + gateway detector state.

## Proposed crates

| Crate | Purpose |
|---|---|
| `tokio` | async runtime, connection concurrency |
| `hyper` / `hyper-util` | HTTP proxy server, `CONNECT` support |
| `tokio-socks` | SOCKS5 client when the upstream/fallback is SOCKS5 |
| `rustls` + `tokio-rustls` | TLS if optional MITM is added in the future |
| `system-configuration` | reactive network reads on macOS (alternative to polling `route -n get default`) |
| `serde` + `toml` | configuration |
| `clap` | CLI flags |

## Gateway detection / IP changes

- **Simple option (to start with)**: run `route -n get default`, parse the `gateway:` field, and compare it against the cached value in a poller (`tokio::time::interval`, every 5–10s).
- **Reactive option (future optimisation)**: `SCDynamicStore`/`SCNetworkReachability` via the `system-configuration` crate, to react to network events without polling.
- The detector runs as a task separate from the connection handler, so active connections are not blocked when the network changes.

## Configuration (draft)

```toml
[listen]
host = "127.0.0.1"
port = 1234

[upstream]
type = "gateway"          # "gateway" | "static" | "none"
port = 8080
poll_interval_secs = 5    # only applies when type = "gateway"

[fallback]
type = "static"           # "static" | "direct" | "none"
host = "1.2.3.4"
port = 8080
```

Connection logic:

```
current_upstream = resolve(config.upstream, gateway_state)
try connecting to current_upstream
  on failure (timeout / connection refused):
    try fallback
      on failure:
        if config.fallback.type == "direct" → connect directly to destination
        otherwise → connection error
```

## macOS: additional considerations

- Registering the app as the system proxy: call `networksetup` from Rust, or leave manual configuration in Network Settings.
- To run in the background as a service: package it as a **LaunchAgent**.
- For a configuration UI: **Tauri** (TS/Vue frontend + Rust core) or native SwiftUI + Rust binary via FFI/XPC.

## Mode without administrator privileges (shell launcher)

For machines without admin privileges (where installing as `launchd`/`systemd` is not possible), an alternative mode is supported: a launcher embedded in `.zshrc`/`.bashrc` that checks whether the daemon is running and, if not, starts it in the background. On machines with privileges, it can be installed as a real service (see the table in the previous section); the daemon binary is agnostic to how it is launched.

### Launcher in `.zshrc` / `.bashrc`

- Check based on a **pidfile** (`~/.local/state/localproxy/localproxy.pid`) + `flock` on a lockfile, to avoid race conditions when several terminals open at the same time.
- On shell startup:
  1. Does the pidfile exist?
  2. Is the PID alive (`kill -0`) and does it actually correspond to `localproxy` (avoid recycled PIDs)?
  3. If not, clean up the pidfile and relaunch.
- Start with `nohup localproxy daemon >> ~/.local/state/localproxy/log 2>&1 & disown` so it survives closing the terminal.
- The check must be cheap (read pidfile + `kill -0`), without spawning heavy processes in every new shell.

### `localproxy config` — interactive wizard + hot reload

- The `localproxy config` command launches a terminal wizard (candidate crates: `dialoguer` or `inquire`) to configure listen/upstream/fallback.
- Control channel via a **Unix socket** (`~/.local/state/localproxy/localproxy.sock`) instead of signals (`SIGHUP`), being more flexible and portable across macOS/Linux:
  - When saving the config, the wizard writes the TOML and sends a `reload` command over the socket.
  - The same socket serves `localproxy status`, `localproxy stop`, etc.
  - If the daemon is not running, the wizard saves the TOML without notifying; the shell launcher will pick it up on the next start.

### Distribution as a real service (machines with privileges)

- The daemon binary runs in the foreground; `launchd`/`systemd` handles backgrounding, restarts, logs, etc.
- Optional unit file templates can be distributed (`.service` for systemd, `.plist` for launchd) without the binary depending on them.

## Cross-platform: macOS + Linux

The proxy core (`tokio`, `hyper`, `tokio-socks`, `rustls`, `serde`/`toml`, `clap`) is portable without changes. Only the system-specific parts need per-platform implementations, ideally behind a trait (e.g. `GatewayDetector`) with `#[cfg(target_os = "...")]`:

| Function | macOS | Linux |
|---|---|---|
| Gateway detection | `route -n get default` or `system-configuration` (SCDynamicStore) | read `/proc/net/route`, or `ip route show default` |
| Register as system proxy | `networksetup` | depends on the environment (GNOME: `gsettings`; KDE: `kwriteconfig`; or `http_proxy`/`https_proxy` variables) |
| Run as a service | LaunchAgent/LaunchDaemon (`launchd`) | `systemd` unit |

**Cross-compilation:**

- Build for Linux from macOS: add the target (`rustup target add x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`) and use `cross` (Docker-based) to avoid linking issues.
- Simpler alternative: build natively on each platform via CI, with a `macos-latest` / `ubuntu-latest` matrix in GitHub Actions.

## Next steps / open decisions

- [ ] Define the project crate structure (workspace vs single crate).
- [ ] Decide whether the fallback can be cascaded (another proxy with its own fallback) or only one level.
- [ ] Decide whether bypass rules by domain/IP are needed (skip the proxy for certain destinations).
- [ ] Choose the gateway detection strategy: simple polling vs `SCDynamicStore`.
- [ ] Decide whether there will be a UI (Tauri) or only a CLI/daemon with LaunchAgent.
- [ ] Minimal prototype: HTTP proxy server with `hyper` + `CONNECT` support, no upstream yet.
- [ ] Design the `GatewayDetector` trait (or similar) to separate per-platform implementations (macOS/Linux) from the start.
- [ ] Implement the shell launcher (pidfile + lock) and the `localproxy daemon` command.
- [ ] Implement the Unix control socket (reload/status/stop) and the `localproxy config` wizard.
