//! CLI surface: argument parsing and subcommand defaults.

use std::fs;

use clap::Parser;
use localproxy::cli::{Cli, Command, ServiceCommand};
use localproxy::config::{AppConfig, ProxyProtocol, UpstreamConfig};

#[test]
fn no_subcommand_prints_help() {
    let cli = Cli::try_parse_from(["localproxy"]).unwrap();

    assert!(cli.command.is_none());
}

#[test]
fn every_top_level_subcommand_is_accepted() {
    for (args, expected) in [
        (vec!["localproxy", "run"], "Run"),
        (vec!["localproxy", "config"], "Config"),
        (vec!["localproxy", "config-extend"], "ConfigExtend"),
        (vec!["localproxy", "status"], "Status"),
        (vec!["localproxy", "stop"], "Stop"),
        (vec!["localproxy", "reload"], "Reload"),
        (vec!["localproxy", "paths"], "Paths"),
        (vec!["localproxy", "url"], "Url"),
        (vec!["localproxy", "purge"], "Purge"),
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

#[tokio::test]
async fn config_extend_adds_missing_fields_without_overwriting_existing_values() {
    let dir = tempfile::tempdir().unwrap();
    let paths = localproxy::testing::paths(dir.path());
    paths.ensure_dirs().unwrap();

    let old_config = r#"
        [listen]
        port = 4321

        [upstream]
        type = "gateway"
        port = 1234

        [[proxy]]
        name = "corp"
        host = "10.0.0.1"
        port = 3128
    "#;
    fs::write(&paths.config_file, old_config).unwrap();

    localproxy::cli::dispatch(Command::ConfigExtend, paths.clone())
        .await
        .unwrap();

    let raw = fs::read_to_string(&paths.config_file).unwrap();
    assert!(raw.contains("[notifications]"));

    let config: AppConfig = toml::from_str(&raw).unwrap();

    // Existing values are preserved.
    assert_eq!(config.listen.port, 4321);
    assert_eq!(config.proxies[0].name, "corp");

    // Missing values are added from defaults.
    assert!(config.notifications.enabled);
    assert!(matches!(config.proxies[0].protocol, ProxyProtocol::Http));
    assert_eq!(config.proxies[0].connect_timeout_ms, 3_000);
    assert!(matches!(
        config.upstream,
        UpstreamConfig::Gateway {
            protocol: ProxyProtocol::Http,
            port: 1234,
            poll_interval_secs: 5,
            connect_timeout_ms: 3_000,
        }
    ));
}
