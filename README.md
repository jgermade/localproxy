# localproxy

<p align="center">
  <img src="./localproxy-logo.svg" alt="localproxy" width="640" />
</p>

localproxy is a local proxy daemon written in Rust. It listens on localhost and forwards HTTP and HTTPS traffic through a configurable resolution chain:

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

## Install

One line, like nvm or oh-my-zsh — detects your platform, installs into `~/.local/bin` and sets up
your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

Prefer doing it by hand? Download the binary for your platform
from the [latest release](https://github.com/jgermade/localproxy/releases/latest):

```bash
# macOS (Apple Silicon)
curl -fsSL -o ~/.local/bin/localproxy \
  https://github.com/jgermade/localproxy/releases/latest/download/localproxy-macos-aarch64
chmod +x ~/.local/bin/localproxy
```

Available assets: `localproxy-macos-aarch64`, `localproxy-macos-x86_64`, `localproxy-linux-x86_64`,
`localproxy-linux-aarch64`. See [docs/installation.md](docs/installation.md) for platform detection,
Gatekeeper notes, upgrades and uninstall.

## Shell setup (zsh / bash)

localproxy does not change the system network settings: tools pick it up through the `http_proxy` /
`https_proxy` environment variables. The installer adds the block below for you; add it by hand to
`~/.zshrc` (zsh) or `~/.bashrc` / `~/.bash_profile` (bash) if you used `--no-modify-profile`. It
starts the daemon when it is not running and exports the variables:

```bash
# --- localproxy -----------------------------------------------------------
export PATH="$HOME/.local/bin:$PATH"

LOCALPROXY_URL="http://127.0.0.1:1234"
LOCALPROXY_NO_PROXY="localhost,127.0.0.1,::1"

if command -v localproxy > /dev/null 2>&1; then
  # `localproxy status` talks to the control socket, so it only succeeds when the
  # daemon is really listening. Start it in the background otherwise.
  localproxy status > /dev/null 2>&1 || localproxy start --detached > /dev/null 2>&1

  export http_proxy="$LOCALPROXY_URL"
  export https_proxy="$LOCALPROXY_URL"
  export all_proxy="$LOCALPROXY_URL"
  export HTTP_PROXY="$LOCALPROXY_URL"
  export HTTPS_PROXY="$LOCALPROXY_URL"
  export ALL_PROXY="$LOCALPROXY_URL"
  export no_proxy="$LOCALPROXY_NO_PROXY"
  export NO_PROXY="$LOCALPROXY_NO_PROXY"
fi
# --------------------------------------------------------------------------
```

Reload with `source ~/.zshrc` (or `~/.bashrc`). Notes:

- Both cases are exported on purpose: `curl` and most Unix tools read the lowercase names, while
  many language runtimes read the uppercase ones.
- If you registered the service (`localproxy service install`), replace `localproxy start --detached` with
  `localproxy service start`.
- Extend `LOCALPROXY_NO_PROXY` with the hosts that must bypass the proxy, e.g.
  `"localhost,127.0.0.1,::1,.internal.example.com,192.168.0.0/16"`.
- The `localproxy status` probe adds a few milliseconds to every shell start; use the `proxy-on` /
  `proxy-off` toggle functions from [docs/shell-integration.md](docs/shell-integration.md) if you
  prefer to opt in per session.

## Documentation

- [docs/installation.md](docs/installation.md) — install from the release binaries, upgrade and uninstall.
- [docs/quickstart.md](docs/quickstart.md) — build, first run and basic usage.
- [docs/shell-integration.md](docs/shell-integration.md) — zsh/bash proxy variables, toggles and aliases.
- [docs/configuration.md](docs/configuration.md) — config format, wizard walkthrough and examples.
- [docs/operations.md](docs/operations.md) — daemon modes, service management, logs and daily operations.
- [docs/testing.md](docs/testing.md) — test suite and coverage.
- [macos-proxy-rust-design.md](macos-proxy-rust-design.md) — original design and evolution backlog.

## Commands

| Command | Description |
|---|---|
| `localproxy daemon` | Start the daemon and the proxy listener in the foreground. |
| `localproxy config` | Open the interactive wizard, save config and hot-reload the daemon. |
| `localproxy status` | Query the running daemon via the Unix control socket. |
| `localproxy reload` | Reload config from disk without restarting the process. |
| `localproxy stop` | Stop the daemon. |
| `localproxy start` | Start the service if installed; otherwise ask to run detached. |
| `localproxy start --detached` | Start `localproxy daemon` in the background without registering a service. |
| `localproxy logs [--lines N] [--follow] [--detached]` | Show logs from the service or the detached log file. |
| `localproxy paths` | Print config, state, socket and pid file paths. |
| `localproxy service install` | Register as a user-level service (LaunchAgent / systemd --user). |
| `localproxy service start` | Start the registered service. |
| `localproxy service restart` | Restart the registered service. |
| `localproxy service status` | Query the service manager (installed / running state). |
| `localproxy service stop` | Stop the registered service. |
| `localproxy service logs [--lines N] [--follow]` | Show service logs (tail/journalctl). |
| `localproxy service uninstall` | Unregister the user-level service. |

## Default paths

| Purpose | Path |
|---|---|
| Configuration | `~/.config/localproxy/config.toml` |
| State directory | `~/.local/state/localproxy` |
| Control socket | `~/.local/state/localproxy/localproxy.sock` |
| PID file | `~/.local/state/localproxy/localproxy.pid` |
| Lock file | `~/.local/state/localproxy/localproxy.lock` |
| Log file (detached) | `~/.local/state/localproxy/localproxy.log` |

## Development

```bash
cargo fmt --check
cargo check
cargo test
cargo run -- paths
cargo run -- config
cargo run -- daemon
```

See [docs/testing.md](docs/testing.md) for the test layout and how to measure coverage.

If the global Cargo cache is locked by another process, isolate it:

```bash
CARGO_HOME=$PWD/.cargo-home cargo check
```

## CI / Release

| Workflow | Trigger |
|---|---|
| [build.yml](.github/workflows/build.yml) | push / PR to `main` |
| [release.yml](.github/workflows/release.yml) | manual dispatch with `patch` / `minor` / `major` input |

`build.yml` runs fmt, clippy, `cargo test` and a coverage gate (`scripts/coverage.sh --fail-under 60`)
before building the binaries.

`release.yml` bumps the version in `Cargo.toml`, commits, tags, builds cross-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64) and publishes a GitHub Release with those binaries atta[...]

## License

MIT — see [LICENSE](LICENSE).
