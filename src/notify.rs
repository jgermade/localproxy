//! Desktop notifications for daemon lifecycle events.
//!
//! Every notification is best effort: delivery failures are logged at debug level and
//! never interrupt the proxy. Notifications can be turned off with `[notifications]
//! enabled = false` in the config file.

use tracing::debug;

use crate::config::NotificationsConfig;

const APP_NAME: &str = "localproxy";

/// Posts a desktop notification unless notifications are disabled in the config.
///
/// When called from inside a Tokio runtime the delivery runs on the blocking pool, so
/// the caller never waits for the notification server.
pub fn notify(config: &NotificationsConfig, summary: &str, body: &str) {
    if !config.enabled {
        return;
    }

    let summary = summary.to_string();
    let body = body.to_string();

    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn_blocking(move || send(&summary, &body));
        }
        Err(_) => send(&summary, &body),
    }
}

fn send(summary: &str, body: &str) {
    if let Err(error) = notify_rust::Notification::new()
        .appname(APP_NAME)
        .summary(summary)
        .body(body)
        .show()
    {
        debug!(%error, "no se pudo enviar la notificación de escritorio");
    }
}
