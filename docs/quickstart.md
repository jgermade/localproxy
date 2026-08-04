# Quickstart

## Install

One line installs the binary for your platform into `~/.local/bin` and wires up your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/zproxy/main/install.sh | bash
```

Options go after `-s --`, e.g. `bash -s -- --dir /usr/local/bin --no-modify-profile`.

Manual alternative — grab the prebuilt binary attached to the latest release:

```bash
# macOS (Apple Silicon)
curl -fsSL -o ~/.local/bin/zproxy \
  https://github.com/jgermade/zproxy/releases/latest/download/zproxy-macos-aarch64
chmod +x ~/.local/bin/zproxy
export PATH="$HOME/.local/bin:$PATH"
```

Other assets: `zproxy-macos-x86_64`, `zproxy-linux-x86_64`, `zproxy-linux-aarch64`. Full details
(installer options, platform detection, Gatekeeper, upgrades, uninstall) in
[installation.md](installation.md).

## Build from source

Only needed for development.

### Requirements

- A recent Rust toolchain with `cargo`.
- macOS or Linux.
- Internet access to download crates on the first build.

```bash
cargo build
```

Optimised release binary:

```bash
cargo build --release
```

## Check default paths

```bash
zproxy paths
# or during development:
cargo run -- paths
```

Expected output:

```text
config: /Users/you/.config/zproxy/config.toml
state:  /Users/you/.local/state/zproxy
socket: /Users/you/.local/state/zproxy/zproxy.sock
pid:    /Users/you/.local/state/zproxy/zproxy.pid
```

## Create the initial configuration

```bash
zproxy config
```

The interactive wizard walks you through listen address, upstream and fallback. Every prompt is pre-filled with the current value, so pressing <kbd>Enter</kbd> keeps it.

If the daemon is already running, the wizard automatically sends a `reload` command through the control socket so the new config takes effect immediately.

### Wizard session examples

**Direct mode** (no upstream, direct fallback):

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

**Gateway upstream** (route through the current default gateway):

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

**Static SOCKS5 upstream** (fixed upstream proxy, no fallback):

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

See [configuration.md](configuration.md) for the full prompt reference and more session examples.

## Start the daemon

Recommended for daily use (registers a user-level service):

```bash
zproxy service install
zproxy start
```

Without registering a service:

```bash
zproxy start --detached
```

For development / foreground:

```bash
zproxy daemon
```

## Test the proxy

Plain HTTP:

```bash
curl -x http://127.0.0.1:8888 http://example.com
```

HTTPS via CONNECT tunnel:

```bash
curl -x http://127.0.0.1:8888 https://example.com
```

## Use it from your shell

Export the proxy variables so every tool picks zproxy up automatically:

```bash
export http_proxy="http://127.0.0.1:8888"
export https_proxy="$http_proxy"
export no_proxy="localhost,127.0.0.1,::1"
```

### Make it permanent (zsh / bash)

The one-line installer already adds this block. Add it by hand to `~/.zshrc` (zsh), `~/.bashrc`
(bash on Linux) or `~/.bash_profile` (bash login shell on macOS) if you installed manually or used
`--no-modify-profile`. It starts the daemon if it is not already running and exports every variable
the usual tooling looks at:

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

Apply it to the current shell:

```bash
source ~/.zshrc     # or ~/.bashrc
```

Check the result:

```bash
env | grep -i proxy
zproxy status
curl -s https://example.com > /dev/null && echo ok
```

Notes:

- Lowercase and uppercase variables are both exported because tools disagree: `curl` and most Unix
  tools read the lowercase names, many language runtimes read the uppercase ones.
- If you registered the service with `zproxy service install`, replace `zproxy start --detached`
  with `zproxy service start` so the service manager owns the process.
- `zproxy start` without `--detached` asks for confirmation when no service is installed, so it is
  not safe inside a startup file. Always use `start --detached` or `service start` there.
- Extend `ZPROXY_NO_PROXY` with the hosts that must bypass the proxy, e.g.
  `"localhost,127.0.0.1,::1,.internal.example.com,192.168.0.0/16"`.
- The `zproxy status` probe adds a few milliseconds to every shell start. If that matters, drop the
  auto-start line and use the `proxy-on` / `proxy-off` functions from
  [shell-integration.md](shell-integration.md).

See [shell-integration.md](shell-integration.md) for toggle functions, per-command wrappers, aliases
and tools that need their own proxy configuration.

## Query status

```bash
zproxy status
```

Example output:

```text
listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

## Stop the daemon

```bash
zproxy stop
```

## Notes

- The first build requires internet access to fetch crates from crates.io.
- There is no automatic integration with macOS Network Preferences or Linux desktop environments;
  use the environment variables described in [shell-integration.md](shell-integration.md).
