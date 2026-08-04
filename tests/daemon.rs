//! Daemon lifecycle: single-instance lock and shared state.

use std::fs;

use zproxy::{app::PidGuard, config};

#[test]
fn pid_guard_writes_the_pid_file_and_removes_it_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    let guard = PidGuard::acquire(&paths).unwrap();

    let pid = fs::read_to_string(paths.pid_file()).unwrap();
    assert_eq!(pid.trim(), std::process::id().to_string());
    assert!(paths.lock_file().exists());

    drop(guard);
    assert!(!paths.pid_file().exists());
}

#[test]
fn pid_guard_rejects_a_second_instance() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    let _guard = PidGuard::acquire(&paths).unwrap();
    let error = PidGuard::acquire(&paths).unwrap_err();

    assert!(error.to_string().contains("ya está corriendo"));
}

#[test]
fn pid_guard_can_be_reacquired_after_release() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    drop(PidGuard::acquire(&paths).unwrap());

    assert!(PidGuard::acquire(&paths).is_ok());
}

#[test]
fn pid_guard_fails_when_the_state_dir_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(&dir.path().join("missing"));

    let error = PidGuard::acquire(&paths).unwrap_err();

    assert!(error.to_string().contains("no se pudo abrir"));
}

#[tokio::test]
async fn a_fresh_state_has_no_gateway_and_is_not_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let state = zproxy::testing::state(
        zproxy::testing::paths(dir.path()),
        config::AppConfig::default(),
    );

    assert!(state.gateway_ip.read().await.is_none());
    assert!(!state.shutdown.is_cancelled());

    state.shutdown.cancel();
    assert!(state.shutdown.is_cancelled());
}
