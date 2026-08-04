# Operations

## Daemon modes

zproxy can run in three modes:

| Mode | Command | Description |
|---|---|---|
| Foreground | `zproxy daemon` | Runs in the terminal; exits when the terminal closes. |
| Detached | `zproxy start --detached` | Spawns the daemon in the background; stdout/stderr go to `zproxy.log`. |
| Service | `zproxy service install && zproxy start` | Registers a user-level service managed by launchd (macOS) or systemd (Linux). |

## Service management

### Install and start

```bash
zproxy service install
zproxy start            # starts the service (or asks to run detached if not installed)
```

### Status

```bash
zproxy service status
```

Example output:

```text
installed=true running=true state=running
```

### Restart

```bash
zproxy service restart
```

### Stop

```bash
zproxy service stop
```

### Uninstall

```bash
zproxy service uninstall
```

## `zproxy start` behaviour

1. If a service is installed → runs `service start`.
2. Otherwise → prompts before doing anything:

```console
$ zproxy start
No service installed. Do you want to run start --detached? [Y/n] y
daemon started in background with pid 41822
```

Declining leaves the system untouched:

```console
$ zproxy start
No service installed. Do you want to run start --detached? [Y/n] n
cancelled
```

Use `zproxy start --detached` to bypass the prompt and always start detached.

## Runtime state on disk

All runtime files live under `~/.local/state/zproxy`:

| File | Purpose |
|---|---|
| `zproxy.pid` | PID of the running instance. |
| `zproxy.lock` | Exclusive lock; prevents multiple instances. |
| `zproxy.sock` | Unix control socket. |
| `zproxy.log` | stdout/stderr from detached mode. |

## Control socket

The Unix socket accepts one command per line:

| Command | Description |
|---|---|
| `status` | Returns a summary of the running config. |
| `reload` | Re-reads `config.toml` from disk and applies it in memory. |
| `stop` | Shuts down the daemon. |

The CLI wraps these for you:

```bash
zproxy status
zproxy reload
zproxy stop
```

## `zproxy status` output

```text
listen=127.0.0.1:8888 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1
```

Fields: `listen`, `upstream`, `fallback`, `gateway` (current detected IP, or `unknown`).

## Configuration and hot-reload

`zproxy config` opens the wizard, saves the file, and immediately sends `reload` to the daemon if it is running:

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

If the daemon is not running, the config is still written and the last line becomes:

```text
config saved; daemon not notified: No such file or directory (os error 2)
```

To reload without the wizard:

```bash
zproxy reload
```

Reload replaces the in-memory config; in-flight connections continue with the old config, new connections use the new one.

See [configuration.md](configuration.md) for the complete `zproxy config` prompt reference and session examples.

### Common `zproxy config` operations

**Switch from direct to gateway upstream while the daemon is running:**

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

**Disable the upstream temporarily (go direct):**

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 8888
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

**Point to a static SOCKS5 proxy:**

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

**Change the listen port** — the config is reloaded but the listener stays on the old port until the daemon restarts:

```console
$ zproxy config
✔ Listen host · 127.0.0.1
✔ Listen port · 9999
✔ Upstream type · none
✔ Fallback type · direct
reloaded: listen=127.0.0.1:9999 upstream=none fallback=direct gateway=unknown

$ zproxy service restart      # or: zproxy stop && zproxy start
service restarted
```

## Gateway detection

When `upstream.type = "gateway"`, the daemon keeps a shared state with the current default gateway IP:

- macOS: `route -n get default`
- Linux: `/proc/net/route` or `ip route show default`

Detection runs in a background task and only affects new connections.

Verify manually:

```bash
# macOS
route -n get default

# Linux
ip route show default
```

## Logging

The binary uses `tracing`. Default level: `info,zproxy=debug`. Override with `RUST_LOG`:

```bash
RUST_LOG=debug zproxy daemon
RUST_LOG=trace zproxy daemon
RUST_LOG=warn  zproxy daemon
```

### View logs

Service logs (uses the OS service manager):

```bash
zproxy service logs
zproxy service logs --lines 200
zproxy service logs --follow
```

Universal logs (service if installed, detached log file otherwise):

```bash
zproxy logs
zproxy logs --follow
zproxy logs --lines 50
zproxy logs --detached      # force the zproxy.log file even if a service is installed
```

Platform details:

- **macOS**: tails `launchd.out.log` and `launchd.err.log` in `~/.local/state/zproxy`.
- **Linux**: uses `journalctl --user -u zproxy.service`.
- **Detached mode**: `~/.local/state/zproxy/zproxy.log` (stdout + stderr of the spawned process).

## Troubleshooting

### Daemon won't start — another instance is already running

The lock file is exclusive. Check whether the process is still alive:

```bash
cat ~/.local/state/zproxy/zproxy.pid
ps aux | grep zproxy
```

If the process is gone (stale lock after a crash), remove runtime files:

```bash
rm ~/.local/state/zproxy/zproxy.{pid,lock,sock}
zproxy start
```

### `status` or `reload` can't connect to the socket

Likely causes:

- The daemon is not running.
- The socket file was deleted.
- `HOME` mismatch between the daemon and the CLI process.

Check expected paths:

```bash
zproxy paths
```

### Gateway upstream never resolves

Test the underlying system command directly:

```bash
# macOS
route -n get default

# Linux
ip route show default
```

## Current limitations

- No log rotation.
- No persistent upstream health check outside of connection attempts.
- No HTTP or SOCKS5 upstream authentication.
- No metrics endpoint.
- Control socket commands are limited to `status`, `reload` and `stop`.
