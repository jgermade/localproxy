# zproxy

zproxy is a local proxy daemon written in Rust. It listens on localhost and forwards HTTP and HTTPS traffic through a configurable resolution chain:

1. optional primary upstream
2. optional fallback
3. direct connection when allowed by the configuration

## Features

- HTTP proxy with support for plain HTTP requests.
- CONNECT tunneling for HTTPS traffic (no MITM).
- Static HTTP or SOCKS5 upstream.
- Dynamic upstream based on the system default gateway.
- Static or direct fallback.
- Daemon with pidfile, lockfile and Unix control socket.
- Interactive wizard to generate the TOML configuration.
- User-level service management (LaunchAgent on macOS, systemd --user on Linux).
- Detached background mode without a registered service.

Not yet implemented:

- Automatic system proxy registration.
- Per-domain or per-CIDR bypass rules.
- Multi-level fallback chains.
- Upstream proxy authentication.

## Documentation

- [docs/quickstart.md](docs/quickstart.md) — build, first run and basic usage.
- [docs/configuration.md](docs/configuration.md) — config format, wizard walkthrough and examples.
- [docs/operations.md](docs/operations.md) — daemon modes, service management, logs and daily operations.
- [macos-proxy-rust-design.md](macos-proxy-rust-design.md) — original design and evolution backlog.

## Commands

| Command | Description |
|---|---|
| `zproxy daemon` | Start the daemon and the proxy listener in the foreground. |
| `zproxy config` | Open the interactive wizard, save config and hot-reload the daemon. |
| `zproxy status` | Query the running daemon via the Unix control socket. |
| `zproxy reload` | Reload config from disk without restarting the process. |
| `zproxy stop` | Stop the daemon. |
| `zproxy start` | Start the service if installed; otherwise ask to run detached. |
| `zproxy start --detached` | Start `zproxy daemon` in the background without registering a service. |
| `zproxy logs [--lines N] [--follow] [--detached]` | Show logs from the service or the detached log file. |
| `zproxy paths` | Print config, state, socket and pid file paths. |
| `zproxy service install` | Register as a user-level service (LaunchAgent / systemd --user). |
| `zproxy service start` | Start the registered service. |
| `zproxy service restart` | Restart the registered service. |
| `zproxy service status` | Query the service manager (installed / running state). |
| `zproxy service stop` | Stop the registered service. |
| `zproxy service logs [--lines N] [--follow]` | Show service logs (tail/journalctl). |
| `zproxy service uninstall` | Unregister the user-level service. |

## Default paths

| Purpose | Path |
|---|---|
| Configuration | `~/.config/zproxy/config.toml` |
| State directory | `~/.local/state/zproxy` |
| Control socket | `~/.local/state/zproxy/zproxy.sock` |
| PID file | `~/.local/state/zproxy/zproxy.pid` |
| Lock file | `~/.local/state/zproxy/zproxy.lock` |
| Log file (detached) | `~/.local/state/zproxy/zproxy.log` |

## Development

```bash
cargo fmt --check
cargo check
cargo run -- paths
cargo run -- config
cargo run -- daemon
```

If the global Cargo cache is locked by another process, isolate it:

```bash
CARGO_HOME=$PWD/.cargo-home cargo check
```

## CI / Release

| Workflow | Trigger |
|---|---|
| [build.yml](.github/workflows/build.yml) | push / PR to `main` |
| [release.yml](.github/workflows/release.yml) | manual dispatch with `patch` / `minor` / `major` input |

`release.yml` bumps the version in `Cargo.toml`, commits, tags, builds cross-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64) and publishes a GitHub Release with those binaries attached.