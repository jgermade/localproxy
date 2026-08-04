# Testing

## Run the suite

```bash
cargo test
```

The tests live next to the code they exercise, as `#[cfg(test)] mod tests` blocks inside each
module. There is no `tests/` directory: zproxy is a binary crate, so integration-style tests are
written inside the modules and drive the real listeners over loopback sockets.

Useful variants:

```bash
cargo test --all-targets           # what CI runs
cargo test proxy::                 # only the proxy module
cargo test -- --nocapture          # show stdout from the tests
cargo test -- --test-threads 1     # serialise, useful when debugging port issues
```

## What is covered

| Module | Focus |
|---|---|
| [src/config.rs](../src/config.rs) | Defaults, TOML round-trips, saved-proxy lookup, upstream/fallback resolution, `load_or_create` / `save` against a temp dir. |
| [src/proxy.rs](../src/proxy.rs) | HTTP head parsing, destination extraction, request rewriting, route ordering, plus end-to-end HTTP forwarding, CONNECT tunnelling, upstream chaining, failover and 502 handling. |
| [src/control.rs](../src/control.rs) | Command parsing and a real Unix socket exercising `status`, `reload` and `stop`. |
| [src/gateway.rs](../src/gateway.rs) | `route -n get default`, `/proc/net/route` and `ip route` parsing, plus the detector loop lifecycle. |
| [src/app.rs](../src/app.rs) | `PidGuard` locking, pid file lifecycle and single-instance enforcement. |
| [src/service.rs](../src/service.rs) | Plist escaping, log tailing and the command runner error paths. |
| [src/main.rs](../src/main.rs) | CLI argument parsing for every subcommand and flag. |
| [src/stream.rs](../src/stream.rs) | `ProxyStream` read/write/flush/shutdown over TCP. |

Deliberately **not** covered:

- `zproxy service install` / `uninstall` on macOS and Linux — they call `launchctl` / `systemctl`
  and would modify the machine running the tests.
- `app::start_detached` — it spawns the current executable, which under `cargo test` is the test
  harness itself.
- The `zproxy config` wizard prompts — they require an interactive terminal.
- The SOCKS5 branch of `ProxyStream`, which needs a real SOCKS5 server.

## Test conventions

- Every test uses `tempfile::tempdir()` for config and state; nothing touches `~/.config/zproxy` or
  `~/.local/state/zproxy`.
- Network tests bind to `127.0.0.1:0` and read back the assigned port, so they run in parallel
  without port collisions.
- Async tests use `#[tokio::test]` and wrap blocking reads in `tokio::time::timeout` so a regression
  fails instead of hanging.
- Daemon-style tasks are shut down through the `CancellationToken` in `SharedState` at the end of
  each test.

## Coverage

Coverage uses LLVM source-based instrumentation through [scripts/coverage.sh](../scripts/coverage.sh):

```bash
scripts/coverage.sh                     # per-file summary table
scripts/coverage.sh --html              # also writes target/coverage/html/index.html
scripts/coverage.sh --fail-under 70     # non-zero exit below the threshold
```

Requirements — the script looks for `llvm-profdata` / `llvm-cov` in this order:

1. `$LLVM_BIN_DIR`
2. The Rust toolchain (`rustup component add llvm-tools`)
3. `PATH`
4. Homebrew (`brew install llvm`)

Current numbers (`cargo test`, 105 tests):

```text
Filename        Regions   Missed    Cover    Functions  Missed  Executed   Lines  Missed   Cover
app.rs              267      114   57.30%           17       6    64.71%     142      59  58.45%
config.rs          1292      500   61.30%           95      23    75.79%     892     275  69.17%
control.rs          390       16   95.90%           29       1    96.55%     200       1  99.50%
gateway.rs          242       34   85.95%           27       4    85.19%     152      29  80.92%
main.rs             283      186   34.28%           19      10    47.37%     179     118  34.08%
proxy.rs           1467       81   94.48%          128      11    91.41%     861      31  96.40%
service.rs          432      256   40.74%           36      20    44.44%     242     144  40.50%
stream.rs            98       18   81.63%            7       0   100.00%      49       4  91.84%
TOTAL              4471     1205   73.05%          358      75    79.05%    2717     661  75.67%
```

The proxy engine and the control socket — the parts that carry traffic — sit above 94%. The gap is
concentrated in code that shells out to the OS (`service.rs`), in the interactive wizard
(`config.rs`) and in the CLI command dispatch (`main.rs`), all of which are excluded for the reasons
listed above.

Note that the report includes the test code itself, which is fully executed by definition; treat the
absolute number as a trend indicator rather than a precise measure of production-code coverage.

## CI

[build.yml](../.github/workflows/build.yml) runs on every push and pull request to `main`:

| Job | Steps |
|---|---|
| `check` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo check --all-targets`, `cargo test --all-targets` |
| `coverage` | `scripts/coverage.sh --fail-under 70` |
| `build` | Release binaries for the four supported targets |

Run the same checks locally before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
scripts/coverage.sh --fail-under 70
```
