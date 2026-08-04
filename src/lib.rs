//! localproxy: local proxy daemon with dynamic upstream resolution.
//!
//! The binary in `src/main.rs` is a thin wrapper around [`cli::run`]; every piece of
//! behaviour lives in this library so it can be driven from the integration tests
//! under `tests/`.

pub mod app;
pub mod cli;
pub mod config;
pub mod control;
pub mod gateway;
pub mod notify;
pub mod proxy;
pub mod service;
pub mod stream;
pub mod testing;

/// Installs the `tracing` subscriber used by the CLI.
///
/// The level can be overridden with `RUST_LOG`; the default is `info,localproxy=debug`.
pub fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,localproxy=debug".into()),
        )
        .with_target(false)
        .init();
}
