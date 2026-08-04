# Testing

## Layout

zproxy is split into a library and a very thin binary:

| Path | Role |
|---|---|
| [src/lib.rs](../src/lib.rs) | Library crate (`zproxy`) exposing every module. |
| [src/cli.rs](../src/cli.rs) | Argument definitions (`Cli`, `Command`, `ServiceCommand`) and command dispatch. |
| [src/main.rs](../src/main.rs) | Wrapper: installs tracing and calls `zproxy::cli::run()`. |
| [src/testing.rs](../src/testing.rs) | Helpers to build `AppPaths` / `SharedState` rooted at a temp dir. |
| [tests/](../tests) | Integration tests, one file per area, driving the public API. |

Because of that split, tests come in two flavours:

- **Unit tests** stay inside each module as `#[cfg(test)] mod tests` blocks, and cover private
  functions: HTTP head parsing, route resolution, gateway output parsing, plist escaping, the
  config description helpers and the wizard accessors.
- **Integration tests** live in `tests/` as separate crates that link against `zproxy` and can only
  touch the public API. They spin up real listeners over loopback sockets.

## Run the suite

```bash
cargo test
```

Useful variants:

```bash
cargo test --all-targets           # what CI runs
cargo test --lib                   # only the in-module unit tests
cargo test --test proxy            # only tests/proxy.rs
cargo test -- --nocapture          # show stdout from the tests
cargo test -- --test-threads 1     # serialise, useful when debugging port issues
```

## What is covered

| File | Focus |
|---|---|
| [tests/proxy.rs](../tests/proxy.rs) | End-to-end HTTP forwarding, CONNECT tunnelling, upstream chaining, failover, 502 handling, bind failure and shutdown. |
| [tests/control.rs](../tests/control.rs) | Command wire format plus a real Unix socket exercising `status`, `reload` and `stop`. |
| [tests/config.rs](../tests/config.rs) | Defaults, TOML round-trips, saved-proxy lookup, upstream/fallback resolution, `load_or_create` / `save` against a temp dir. |
| [tests/cli.rs](../tests/cli.rs) | Argument parsing for every subcommand and flag, plus dispatch of `paths`, the control commands and detached `logs`. |
| [tests/daemon.rs](../tests/daemon.rs) | `PidGuard` locking, pid file lifecycle and single-instance enforcement. |
| [tests/gateway.rs](../tests/gateway.rs) | Detector loop lifecycle and cancellation. |
| [tests/service.rs](../tests/service.rs) | Installation probe and log tailing. |
| [tests/stream.rs](../tests/stream.rs) | `ProxyStream` read/write/flush/shutdown over TCP. |
| [src/proxy.rs](../src/proxy.rs) | HTTP head parsing, destination extraction, request rewriting, route ordering, connect timeouts. |
| [src/gateway.rs](../src/gateway.rs) | `route -n get default`, `/proc/net/route` and `ip route` parsing. |
| [src/config.rs](../src/config.rs) | Upstream/fallback descriptions, kind labels and the wizard value accessors. |
| [src/service.rs](../src/service.rs) | Plist escaping, service label and the command runner error paths. |

Deliberately **not** covered:

- `zproxy service install` / `uninstall` on macOS and Linux — they call `launchctl` / `systemctl`
  and would modify the machine running the tests.
- `app::start_detached` and `zproxy start` — they spawn the current executable, which under
  `cargo test` is the test harness itself.
- The `zproxy config` wizard prompts — they require an interactive terminal.
- The SOCKS5 branch of `ProxyStream`, which needs a real SOCKS5 server.

## Test conventions

- Every test uses `tempfile::tempdir()` for config and state; nothing touches `~/.config/zproxy` or
  `~/.local/state/zproxy`. Use `zproxy::testing::paths()` / `zproxy::testing::state()` instead of
  hand-rolling paths.
- Network tests bind to `127.0.0.1:0` and read back the assigned port, so they run in parallel
  without port collisions.
- Async tests use `#[tokio::test]` and wrap blocking reads in `tokio::time::timeout` so a regression
  fails instead of hanging.
- Daemon-style tasks are shut down through the `CancellationToken` in `SharedState` at the end of
  each test.
- Platform-specific pure functions are gated with `#[cfg(any(target_os = "...", test))]` so they
  compile — and stay testable — on every platform.

## Coverage

Coverage uses LLVM source-based instrumentation through [scripts/coverage.sh](../scripts/coverage.sh):

```bash
scripts/coverage.sh                     # per-file summary table
scripts/coverage.sh --html              # also writes target/coverage/html/index.html
scripts/coverage.sh --fail-under 60     # non-zero exit below the threshold
```

Requirements — the script looks for `llvm-profdata` / `llvm-cov` in this order:

1. `$LLVM_BIN_DIR`
2. The Rust toolchain (`rustup component add llvm-tools`)
3. `PATH`
4. Homebrew (`brew install llvm`)

The report ignores `tests/` and `src/testing.rs`, so the numbers describe production code only.

Current numbers (`cargo test`, 107 tests):

```text
Filename       Regions   Missed    Cover    Functions  Missed  Executed   Lines  Missed   Cover
app.rs             150      114   24.00%           10       6    40.00%      87      59  32.18%
cli.rs             179      127   29.05%           12       6    50.00%      92      64  30.43%
config.rs          900      494   45.11%           71      23    67.61%     617     276  55.27%
control.rs         153       13   91.50%           11       1    90.91%      74       1  98.65%
gateway.rs         184       34   81.52%           22       4    81.82%     126      29  76.98%
proxy.rs          1002       78   92.22%           94      11    88.30%     597      29  95.14%
service.rs         393      256   34.86%           33      20    39.39%     225     144  36.00%
stream.rs           48       18   62.50%            4       0   100.00%      28       4  85.71%
TOTAL             3025     1150   61.98%          261      75    71.26%    1861     621  66.63%
```

The proxy engine and the control socket — the parts that carry traffic — sit above 95%. The gap is
concentrated in code that shells out to the OS (`service.rs`), in the interactive wizard
(`config.rs`), in the daemon supervisor (`app.rs`) and in the CLI paths that spawn processes
(`cli.rs`), all of which are excluded for the reasons listed above.

## CI

[build.yml](../.github/workflows/build.yml) runs on every push and pull request to `main`:

| Job | Steps |
|---|---|
| `check` | `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo check --all-targets`, `cargo test --all-targets` |
| `coverage` | `scripts/coverage.sh --fail-under 60` |
| `build` | Release binaries for the four supported targets |

Run the same checks locally before pushing:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
scripts/coverage.sh --fail-under 60
```
