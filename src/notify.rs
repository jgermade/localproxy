//! Desktop notifications for daemon lifecycle events.
//!
//! Every notification is best effort: delivery failures are logged at debug level and
//! never interrupt the proxy. Notifications can be turned off with `[notifications]
//! enabled = false` in the config file.

use std::{fs, path::PathBuf, sync::OnceLock};

use anyhow::Result;
use tracing::debug;

use crate::config::{AppPaths, NotificationsConfig};

const APP_NAME: &str = "localproxy";

/// Logo cropped from `localproxy-logo.svg`, shipped inside the binary.
const BUNDLED_ICON: &[u8] = include_bytes!("../assets/localproxy-icon.png");

/// Posts a desktop notification unless notifications are disabled in the config.
///
/// When called from inside a Tokio runtime the delivery runs on the blocking pool, so
/// the caller never waits for the notification server.
pub fn notify(config: &NotificationsConfig, summary: &str, body: &str) {
    if !config.enabled {
        return;
    }

    let icon = icon_path(config);
    let summary = summary.to_string();
    let body = body.to_string();

    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn_blocking(move || send(icon, &summary, &body));
        }
        Err(_) => send(icon, &summary, &body),
    }
}

fn send(icon: Option<PathBuf>, summary: &str, body: &str) {
    let mut notification = notify_rust::Notification::new();
    notification.appname(APP_NAME).summary(summary).body(body);

    if let Some(icon) = icon.as_ref().map(|path| path.to_string_lossy().to_string()) {
        // macOS renders it beside the banner text; XDG servers take it as the image hint.
        notification.image_path(&icon);
        #[cfg(not(target_os = "macos"))]
        notification.icon(&icon);
    }

    if let Err(error) = notification.show() {
        debug!(%error, "no se pudo enviar la notificación de escritorio");
    }
}

fn icon_path(config: &NotificationsConfig) -> Option<PathBuf> {
    if let Some(custom) = &config.icon {
        return Some(PathBuf::from(custom));
    }

    static BUNDLED: OnceLock<Option<PathBuf>> = OnceLock::new();

    BUNDLED
        .get_or_init(|| match write_bundled_icon() {
            Ok(path) => Some(path),
            Err(error) => {
                debug!(%error, "no se pudo materializar el icono de las notificaciones");
                None
            }
        })
        .clone()
}

/// Notification servers read the icon from disk, so the bundled logo is written once
/// into the state directory.
fn write_bundled_icon() -> Result<PathBuf> {
    let paths = AppPaths::discover()?;
    paths.ensure_dirs()?;

    let path = paths.state_dir.join("localproxy-icon.png");
    fs::write(&path, BUNDLED_ICON)?;
    Ok(path)
}
