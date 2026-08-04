//! CLI surface: argument parsing and subcommand defaults.

use clap::Parser;
use zproxy::cli::{Cli, Command, ServiceCommand};

#[test]
fn no_subcommand_means_daemon_mode() {
    let cli = Cli::try_parse_from(["zproxy"]).unwrap();

    assert!(cli.command.is_none());
}

#[test]
fn every_top_level_subcommand_is_accepted() {
    for (args, expected) in [
        (vec!["zproxy", "daemon"], "Daemon"),
        (vec!["zproxy", "config"], "Config"),
        (vec!["zproxy", "status"], "Status"),
        (vec!["zproxy", "stop"], "Stop"),
        (vec!["zproxy", "reload"], "Reload"),
        (vec!["zproxy", "paths"], "Paths"),
    ] {
        let cli = Cli::try_parse_from(&args).unwrap();
        let rendered = format!("{:?}", cli.command.unwrap());

        assert!(
            rendered.starts_with(expected),
            "{args:?} produjo {rendered}"
        );
    }
}

#[test]
fn start_is_attached_unless_detached_is_requested() {
    let attached = Cli::try_parse_from(["zproxy", "start"]).unwrap();
    assert!(matches!(
        attached.command,
        Some(Command::Start { detached: false })
    ));

    let detached = Cli::try_parse_from(["zproxy", "start", "--detached"]).unwrap();
    assert!(matches!(
        detached.command,
        Some(Command::Start { detached: true })
    ));
}

#[test]
fn logs_defaults_to_a_hundred_lines_without_following() {
    let cli = Cli::try_parse_from(["zproxy", "logs"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Logs {
            lines: 100,
            follow: false,
            detached: false,
        })
    ));
}

#[test]
fn logs_accepts_lines_follow_and_detached() {
    let cli =
        Cli::try_parse_from(["zproxy", "logs", "--lines", "5", "--follow", "--detached"]).unwrap();

    assert!(matches!(
        cli.command,
        Some(Command::Logs {
            lines: 5,
            follow: true,
            detached: true,
        })
    ));
}

#[test]
fn service_subcommands_are_parsed() {
    let install = Cli::try_parse_from(["zproxy", "service", "install"]).unwrap();
    assert!(matches!(
        install.command,
        Some(Command::Service {
            command: ServiceCommand::Install
        })
    ));

    let logs = Cli::try_parse_from(["zproxy", "service", "logs", "--lines", "20"]).unwrap();
    assert!(matches!(
        logs.command,
        Some(Command::Service {
            command: ServiceCommand::Logs {
                lines: 20,
                follow: false
            }
        })
    ));
}

#[test]
fn unknown_commands_and_flags_are_rejected() {
    assert!(Cli::try_parse_from(["zproxy", "restart"]).is_err());
    assert!(Cli::try_parse_from(["zproxy", "logs", "--lines", "many"]).is_err());
    assert!(Cli::try_parse_from(["zproxy", "service"]).is_err());
}

#[tokio::test]
async fn the_paths_command_runs_against_a_temporary_home() {
    let dir = tempfile::tempdir().unwrap();

    zproxy::cli::dispatch(Command::Paths, zproxy::testing::paths(dir.path()))
        .await
        .unwrap();
}

#[tokio::test]
async fn control_commands_fail_when_no_daemon_is_listening() {
    for command in [Command::Status, Command::Stop, Command::Reload] {
        let dir = tempfile::tempdir().unwrap();

        let error = zproxy::cli::dispatch(command, zproxy::testing::paths(dir.path()))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no se pudo conectar"));
    }
}

#[tokio::test]
async fn detached_logs_read_the_state_log_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = zproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    let missing = zproxy::cli::dispatch(
        Command::Logs {
            lines: 5,
            follow: false,
            detached: true,
        },
        paths.clone(),
    )
    .await;
    assert!(missing.is_err());

    std::fs::write(paths.log_file(), "one\ntwo\n").unwrap();
    zproxy::cli::dispatch(
        Command::Logs {
            lines: 1,
            follow: false,
            detached: true,
        },
        paths,
    )
    .await
    .unwrap();
}
