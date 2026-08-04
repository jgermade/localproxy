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

The interactive wizard asks for:

1. **Listen address** — host and port for the local proxy (default `127.0.0.1:8888`).
2. **Upstream type** — `none`, `gateway` or `static`.
3. **Upstream protocol** — `http` or `socks5` (only for `gateway` and `static`).
4. **Fallback type** — `none`, `direct` or `static`.

If the daemon is already running, the wizard automatically sends a `reload` command through the control socket so the new config takes effect immediately.

### Wizard walkthrough examples

**Direct mode** (no upstream, direct fallback):

```
? Listen host › 127.0.0.1
? Listen port › 8888
? Upstream type › none
? Fallback type › direct
Config saved. Daemon reloaded.
```

**Gateway upstream** (route through current default gateway):

```
? Listen host › 127.0.0.1
? Listen port › 8888
? Upstream type › gateway
? Upstream protocol › http
? Upstream port › 8080
? Poll interval (secs) › 5
? Connect timeout (ms) › 3000
? Fallback type › direct
Config saved. Daemon reloaded.
```

**Static SOCKS5 upstream** (fixed upstream proxy):

```
? Listen host › 127.0.0.1
? Listen port › 8888
? Upstream type › static
? Upstream protocol › socks5
? Upstream host › 127.0.0.1
? Upstream port › 1080
? Connect timeout (ms) › 3000
? Fallback type › none
Config saved. Daemon reloaded.
```

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
