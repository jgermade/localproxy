//! Service integration: installation probe and log tailing.

use std::fs;

use zproxy::service;

#[test]
fn is_installed_answers_without_failing() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(dir.path());

    // It inspects the real HOME, so only the absence of errors can be asserted.
    assert!(service::is_installed(&paths).is_ok());
}

#[test]
fn tailing_a_missing_log_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("zproxy.log");

    let error = service::tail_file(&missing, 10, false).unwrap_err();

    assert!(error.to_string().contains("no existe el fichero de log"));
}

#[test]
fn tailing_an_existing_log_file_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("zproxy.log");
    fs::write(&log, "line one\nline two\n").unwrap();

    assert!(service::tail_file(&log, 1, false).is_ok());
}
