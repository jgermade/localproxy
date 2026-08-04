//! CLI surface: argument parsing and subcommand defaults.

use clap::Parser;
use localproxy::cli::{Cli, Command, ServiceCommand};

#[test]
fn no_subcommand_means_daemon_mode() {
    let cli = Cli::try_parse_from(["localproxy"]).unwrap();

    assert!(cli.command.is_none());
}

#[test]
fn every_top_level_subcommand_is_accepted() {
    for (args, expected) in [
        (vec!["localproxy", "daemon"], "Daemon"),
        (vec!["localproxy", "config"], "Config"),
        (vec!["localproxy", "status"], "Status"),
        (vec!["localproxy", "stop"], "Stop"),
        (vec!["localproxy", "reload"], "Reload"),
        (vec!["localproxy", "paths"], "Paths"),
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
    let attached = Cli::try_parse_from(["localproxy", "start"]).unwrap();
    assert!(matches!(
        attached.command,
        Some(Command::Start { detached: false })
    ));

    let detached = Cli::try_parse_from(["localproxy", "start", "--detached"]).unwrap();
    assert!(matches!(
        detached.command,
        Some(Command::Start { detached: true })
    ));
}

#[test]
fn logs_defaults_to_a_hundred_lines_without_following() {
    let cli = Cli::try_parse_from(["localproxy", "logs"]).unwrap();

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
    let cli = Cli::try_parse_from([
        "localproxy",
        "logs",
        "--lines",
        "5",
        "--follow",
        "--detached",
    ])
    .unwrap();

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
    let install = Cli::try_parse_from(["localproxy", "service", "install"]).unwrap();
    assert!(matches!(
        install.command,
        Some(Command::Service {
            command: ServiceCommand::Install
        })
    ));

    let logs = Cli::try_parse_from(["localproxy", "service", "logs", "--lines", "20"]).unwrap();
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
    assert!(Cli::try_parse_from(["localproxy", "restart"]).is_err());
    assert!(Cli::try_parse_from(["localproxy", "logs", "--lines", "many"]).is_err());
    assert!(Cli::try_parse_from(["localproxy", "service"]).is_err());
}

#[tokio::test]
async fn the_paths_command_runs_against_a_temporary_home() {
    let dir = tempfile::tempdir().unwrap();

    localproxy::cli::dispatch(Command::Paths, localproxy::testing::paths(dir.path()))
        .await
        .unwrap();
}

#[tokio::test]
async fn control_commands_fail_when_no_daemon_is_listening() {
    for command in [Command::Status, Command::Stop, Command::Reload] {
        let dir = tempfile::tempdir().unwrap();

        let error = localproxy::cli::dispatch(command, localproxy::testing::paths(dir.path()))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no se pudo conectar"));
    }
}

#[tokio::test]
async fn detached_logs_read_the_state_log_file() {
    let dir = tempfile::tempdir().unwrap();
    let paths = localproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    let missing = localproxy::cli::dispatch(
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
    localproxy::cli::dispatch(
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
