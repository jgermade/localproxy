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

## Install

One line, like nvm or oh-my-zsh — detects your platform, installs into `~/.local/bin` and sets up
your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

Options go after `-s --` (`--version`, `--dir`, `--profile`, `--no-modify-profile`):

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh \
  | bash -s -- --dir /usr/local/bin --no-modify-profile
```

Re-run the same command to upgrade. Prefer doing it by hand? Download the binary for your platform
from the [latest release](https://github.com/jgermade/zproxy/releases/latest):

```bash
# macOS (Apple Silicon)
curl -fsSL -o ~/.local/bin/zproxy \
  https://github.com/jgermade/zproxy/releases/latest/download/zproxy-macos-aarch64
chmod +x ~/.local/bin/zproxy
```

Available assets: `zproxy-macos-aarch64`, `zproxy-macos-x86_64`, `zproxy-linux-x86_64`,
`zproxy-linux-aarch64`. See [docs/installation.md](docs/installation.md) for platform detection,
Gatekeeper notes, upgrades and uninstall.

## Shell setup (zsh / bash)

zproxy does not change the system network settings: tools pick it up through the `http_proxy` /
`https_proxy` environment variables. The installer adds the block below for you; add it by hand to
`~/.zshrc` (zsh) or `~/.bashrc` / `~/.bash_profile` (bash) if you used `--no-modify-profile`. It
starts the daemon when it is not running and exports the variables:

```bash
# --- zproxy ---------------------------------------------------------------
export PATH="$HOME/.local/bin:$PATH"

ZPROXY_URL="http://127.0.0.1:8888"
ZPROXY_NO_PROXY="localhost,127.0.0.1,::1"

if command -v zproxy > /dev/null 2>&1; then
  # `zproxy status` talks to the control socket, so it only succeeds when the
  # daemon is really listening. Start it in the background otherwise.
  zproxy status > /dev/null 2>&1 || zproxy start --detached > /dev/null 2>&1

  export http_proxy="$ZPROXY_URL"
  export https_proxy="$ZPROXY_URL"
  export all_proxy="$ZPROXY_URL"
  export HTTP_PROXY="$ZPROXY_URL"
  export HTTPS_PROXY="$ZPROXY_URL"
  export ALL_PROXY="$ZPROXY_URL"
  export no_proxy="$ZPROXY_NO_PROXY"
  export NO_PROXY="$ZPROXY_NO_PROXY"
fi
# --------------------------------------------------------------------------
```

Reload with `source ~/.zshrc` (or `~/.bashrc`). Notes:

- Both cases are exported on purpose: `curl` and most Unix tools read the lowercase names, while
  many language runtimes read the uppercase ones.
- If you registered the service (`zproxy service install`), replace `zproxy start --detached` with
  `zproxy service start`.
- Extend `ZPROXY_NO_PROXY` with the hosts that must bypass the proxy, e.g.
  `"localhost,127.0.0.1,::1,.internal.example.com,192.168.0.0/16"`.
- The `zproxy status` probe adds a few milliseconds to every shell start; use the `proxy-on` /
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

`release.yml` bumps the version in `Cargo.toml`, commits, tags, builds cross-platform binaries (Linux x86_64/aarch64, macOS x86_64/aarch64) and publishes a GitHub Release with those binaries attached.

## License

MIT — see [LICENSE](LICENSE).