# Quickstart

## Install

One line installs the binary for your platform into `~/.local/bin` and wires up your shell profile:

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

```bash
wget -qO- https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh | bash
```

Options go after `-s --` (`--version`, `--dir`, `--profile`, `--no-modify-profile`):

```bash
curl -fsSL https://raw.githubusercontent.com/jgermade/localproxy/main/install.sh \
  | bash -s -- --dir /usr/local/bin --no-modify-profile
```

Re-run the same command to upgrade.

Manual alternative — grab the prebuilt binary attached to the latest release:

```bash
# macOS (Apple Silicon)
curl -fsSL -o ~/.local/bin/localproxy \
  https://github.com/jgermade/localproxy/releases/latest/download/localproxy-macos-aarch64
chmod +x ~/.local/bin/localproxy
export PATH="$HOME/.local/bin:$PATH"
```

Other assets: `localproxy-macos-x86_64`, `localproxy-linux-x86_64`, `localproxy-linux-aarch64`. Full details
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
localproxy paths
# or during development:
cargo run -- paths
```

Expected output:

```text
config: /Users/you/.config/localproxy/config.toml
state:  /Users/you/.local/state/localproxy
socket: /Users/you/.local/state/localproxy/localproxy.sock
pid:    /Users/you/.local/state/localproxy/localproxy.pid
```

## Create the initial configuration

```bash
localproxy config
```

The interactive wizard walks you through listen address, upstream and fallback. Every prompt is pre-filled with the current value, so pressing <kbd>Enter</kbd> keeps it.

If the daemon is already running, the wizard automatically sends a `reload` command through the control socket so the new config takes effect immediately.

### Wizard session examples

**Direct mode** (no upstream, direct fallback):

```console
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:1234 upstream=none fallback=direct gateway=unknown
```

**Gateway upstream** (route through the current default gateway):

```console
$ localproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 1234
✔ Upstream type · gateway
✔ Proxy protocol · http
✔ Gateway upstream port · 1234
✔ Gateway poll interval (seconds) · 5
✔ Fallback type · direct
reloaded: listen=127.0.0.1:1234 upstream=gateway:http:1234 fallback=direct gateway=192.168.1.1
```

**Static SOCKS5 upstream** (fixed upstream proxy, no fallback):

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

See [configuration.md](configuration.md) for the full prompt reference and more session examples.

## Start the daemon

Recommended for daily use (registers a user-level service):

```bash
localproxy service install
localproxy service start
```

Without registering a service:

```bash
localproxy start --detached
```

For development / foreground:

```bash
localproxy run
```

## Test the proxy

Plain HTTP:

```bash
curl -x http://127.0.0.1:1234 http://example.com
```

HTTPS via CONNECT tunnel:

```bash
curl -x http://127.0.0.1:1234 https://example.com
```

## Use it from your shell

Export the proxy variables so every tool picks localproxy up automatically:

```bash
export http_proxy="http://127.0.0.1:1234"
export https_proxy="$http_proxy"
export no_proxy="localhost,127.0.0.1,::1"
```

### Make it permanent (zsh / bash)

The one-line installer already adds this block. Add it by hand to `~/.zshrc` (zsh), `~/.bashrc`
(bash on Linux) or `~/.bash_profile` (bash login shell on macOS) if you installed manually or used
`--no-modify-profile`. It starts the daemon if it is not already running and exports every variable
the usual tooling looks at:

```bash
# --- localproxy -----------------------------------------------------------
export PATH="$HOME/.local/bin:$PATH"

LOCALPROXY_URL="http://127.0.0.1:1234"
LOCALPROXY_NO_PROXY="localhost,127.0.0.1,::1"

if command -v localproxy > /dev/null 2>&1; then
  localproxy-env-off() {
    unset http_proxy https_proxy all_proxy HTTP_PROXY HTTPS_PROXY ALL_PROXY no_proxy NO_PROXY
  }

  localproxy() {
    if [ "$#" -eq 0 ]; then
      command localproxy
      return $?
    fi

    command localproxy "$@"
    rc=$?

    if [ "$rc" -eq 0 ]; then
      if [ "$1" = "stop" ] || [ "$1" = "purge" ]; then
        localproxy-env-off
      elif [ "$1" = "service" ] && [ "${2:-}" = "stop" ]; then
        localproxy-env-off
      fi
    fi

    return $rc
  }

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

Apply it to the current shell:

```bash
source ~/.zshrc     # or ~/.bashrc
```

Check the result:

```bash
env | grep -i proxy
localproxy status
curl -s https://example.com > /dev/null && echo ok
```

Notes:

- Lowercase and uppercase variables are both exported because tools disagree: `curl` and most Unix
  tools read the lowercase names, many language runtimes read the uppercase ones.
- The wrapper function above makes `localproxy stop`, `localproxy service stop` and
  `localproxy purge` unset proxy env vars in the current shell session.
- If you registered the service with `localproxy service install`, replace `localproxy start --detached`
  with `localproxy service start` so the service manager owns the process.
- `localproxy start` without `--detached` asks for confirmation when no service is installed, so it is
  not safe inside a startup file. Always use `start --detached` or `service start` there.
- Extend `LOCALPROXY_NO_PROXY` with the hosts that must bypass the proxy, e.g.
  `"localhost,127.0.0.1,::1,.internal.example.com,192.168.0.0/16"`.
- The `localproxy status` probe adds a few milliseconds to every shell start. If that matters, drop the
  auto-start line and use the `proxy-on` / `proxy-off` functions from
  [shell-integration.md](shell-integration.md).

See [shell-integration.md](shell-integration.md) for toggle functions, per-command wrappers, aliases
and tools that need their own proxy configuration.

## Query status

```bash
localproxy status
```

Example output:

```text
listen=127.0.0.1:1234 upstream=none fallback=direct gateway=unknown
```

## Stop the daemon

```bash
localproxy stop
```

With the installer snippet loaded, this also unsets `http_proxy` / `https_proxy`
(`all_proxy`, uppercase variants and `no_proxy`) in the current shell.

## Full cleanup

```bash
localproxy purge
```

Use `localproxy purge --confirm` in scripts to skip the interactive confirmation.

## Notes

- The first build requires internet access to fetch crates from crates.io.
- There is no automatic integration with macOS Network Preferences or Linux desktop environments;
  use the environment variables described in [shell-integration.md](shell-integration.md).
