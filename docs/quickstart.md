# Quickstart

## Requirements

- A recent Rust toolchain with `cargo`.
- macOS or Linux.
- Internet access to download crates on the first build.

## Build

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
- There is no automatic integration with macOS Network Preferences or Linux desktop environments.
