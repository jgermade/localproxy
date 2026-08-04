//! Helpers shared by the in-module unit tests and the integration suite in `tests/`.
//!
//! They build [`config::AppPaths`] and [`app::SharedState`] values rooted at a temporary
//! directory, so no test ever touches the real user configuration.

use std::{path::Path, sync::Arc};

use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::{app::SharedState, config};

/// Builds the application paths under `dir` (`dir/config` and `dir/state`).
pub fn paths(dir: &Path) -> config::AppPaths {
    let config_dir = dir.join("config");
    config::AppPaths {
        config_file: config_dir.join("config.toml"),
        config_dir,
        state_dir: dir.join("state"),
    }
}

/// Builds a shared state with no detected gateway and a fresh shutdown token.
///
/// Desktop notifications are disabled so the suite never posts to the user session.
pub fn state(paths: config::AppPaths, mut config: config::AppConfig) -> SharedState {
    config.notifications.enabled = false;

    SharedState {
        paths,
        config: Arc::new(RwLock::new(config)),
        gateway_ip: Arc::new(RwLock::new(None)),
        shutdown: CancellationToken::new(),
    }
}
