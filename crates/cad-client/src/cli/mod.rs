//! Root command-line parser and arguments shared by multiple commands.

pub(crate) mod args;

use clap::Parser;

pub use args::{EnvironmentName, GlobalArgs, OutputFormat};

/// Parsed `cad-cli` invocation.
#[derive(Debug, Parser)]
#[command(
    name = "cad-cli",
    version,
    about = "Carbon+Alt+Delete command-line client"
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: crate::cmds::Command,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use clap::{CommandFactory, Parser};

    use super::Cli;
    use crate::cmds;

    #[test]
    fn clap_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_nested_inventory_list() {
        let cli = Cli::try_parse_from([
            "cad-cli",
            "inventory-line",
            "list",
            "--page",
            "3",
            "--filter",
            "country_inList=PL,,DE",
        ])
        .expect("nested command should parse");

        let cmds::Command::InventoryLine(group) = cli.command else {
            panic!("expected inventory-line command");
        };
        let cmds::inventory_line::Command::List(options) = group.command else {
            panic!("expected inventory-line list command");
        };
        assert_eq!(options.page, 3);
        assert_eq!(options.limit, 100);
        assert_eq!(options.filters, ["country_inList=PL,,DE"]);
    }

    #[test]
    fn report_create_uses_polling_defaults() {
        let cli = Cli::try_parse_from(["cad-cli", "report", "create", "--input", "-"])
            .expect("report create should parse");

        let cmds::Command::Report(group) = cli.command else {
            panic!("expected report command");
        };
        let cmds::report::Command::Create(options) = group.command else {
            panic!("expected report create command");
        };
        assert!(options.request.input.input.is_stdin());
        assert_eq!(
            options.request.wait.poll_interval,
            Duration::from_millis(1_500)
        );
        assert_eq!(options.request.wait.timeout, Duration::from_mins(30));
    }

    #[test]
    fn every_waiting_command_shares_the_same_polling_contract() {
        let report = Cli::try_parse_from(["cad-cli", "report", "create", "--input", "-"])
            .expect("report create should parse");
        let duplicate =
            Cli::try_parse_from(["cad-cli", "reporting-period", "duplicate", "--input", "-"])
                .expect("duplicate should parse");

        let cmds::Command::Report(report) = report.command else {
            panic!("expected report command");
        };
        let cmds::report::Command::Create(report) = report.command else {
            panic!("expected report create command");
        };
        let cmds::Command::ReportingPeriod(duplicate) = duplicate.command else {
            panic!("expected reporting-period command");
        };
        let cmds::reporting_period::Command::Duplicate(duplicate) = duplicate.command else {
            panic!("expected duplicate command");
        };

        assert_eq!(
            report.request.wait.poll_interval,
            duplicate.request.wait.poll_interval
        );
        assert_eq!(report.request.wait.timeout, duplicate.request.wait.timeout);
    }

    #[test]
    fn durations_accept_compound_units_and_reject_bare_numbers() {
        let cli = Cli::try_parse_from(["cad-cli", "--request-timeout", "1m30s", "company", "list"])
            .expect("a compound duration should parse");
        assert_eq!(cli.global.request_timeout, Duration::from_secs(90));

        Cli::try_parse_from(["cad-cli", "--request-timeout", "90", "company", "list"])
            .expect_err("a bare number no longer silently means seconds");
    }

    #[test]
    fn environment_flag_accepts_the_same_aliases_as_cad_env() {
        for (value, expected) in [
            ("production", super::EnvironmentName::Production),
            ("prod", super::EnvironmentName::Production),
            ("staging", super::EnvironmentName::Staging),
            ("stage", super::EnvironmentName::Staging),
        ] {
            let cli = Cli::try_parse_from(["cad-cli", "--environment", value, "company", "list"])
                .expect("known environment name");
            assert_eq!(cli.global.environment, Some(expected));
        }
    }

    #[test]
    fn poll_interval_below_the_floor_is_rejected() {
        let error = Cli::try_parse_from([
            "cad-cli",
            "report",
            "create",
            "--input",
            "-",
            "--poll-interval",
            "0",
        ])
        .expect_err("a zero poll interval must not parse");
        assert!(error.to_string().contains("minimum"));
    }
}
