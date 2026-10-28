//! Root command-line parser and arguments shared by multiple commands.

pub(crate) mod args;
// The build script's version logic, compiled here only for its unit tests.
#[cfg(test)]
#[path = "../../build/version.rs"]
mod build_version;
#[cfg(test)]
mod docs;
pub(crate) mod help;

use std::io::IsTerminal;

use clap::{Command, CommandFactory, Parser};

pub use args::{CompanyArg, GlobalArgs, OutputFormat};

use help::{Example, Variable};

const ENVIRONMENT: &[Variable] = &[
    Variable::new(
        "CAD_PROFILE",
        "Profile to use instead of the active one. --profile has precedence over it",
    ),
    Variable::new(
        "CAD_API_KEY",
        "Machine API key ID (a UUID). When it or CAD_API_SECRET is set, the environment alone \
         decides which key commands use; otherwise they use the login of the profile",
    ),
    Variable::new(
        "CAD_API_SECRET",
        "Secret of the machine API key. Used with CAD_API_KEY, and instead of a stored login",
    ),
    Variable::new(
        "CAD_SERVER_URL",
        "Exact application origin, instead of the profile's. --server-url has precedence over \
         it. A stored login is used only with the profile whose server this is",
    ),
    Variable::new(
        "CAD_COMPANY_ID",
        "Accessible company to act on when --company-id is not given. It has precedence over \
         the company of the profile",
    ),
    Variable::new(
        "CAD_CLI_KEYRING",
        "Set to 0 (or false, no, off) to never use the system keyring: logins are then kept in \
         the credentials file only. For machines without a keyring",
    ),
    Variable::new(
        "CAD_CLI_COMPLETE_OFFLINE",
        "Set to 1 to make shell completion answer from its cache only, without platform requests",
    ),
    Variable::new(
        "RUST_LOG",
        "Diagnostic logging on stderr; cad_api=debug logs one line per platform request",
    ),
    Variable::new("NO_COLOR", "Disable colour in tables and status messages"),
    Variable::new(
        "CAD_CLI_NO_UPDATE_CHECK",
        "Set to 1 (or true, yes, on) to stop the once-a-day check for a newer release; empty, \
         0, false, no and off leave it on. The check is also off when CI is set to a true value, \
         with --output json or jsonl, and when stderr is not a terminal",
    ),
];

const EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli use",
        "List the profiles and show which one commands use",
    ),
    Example::new(
        "cad-cli use --check",
        "Check with the server who the CLI acts as and on which company",
    ),
    Example::new(
        "cad-cli use company \"Acme GmbH\"",
        "Choose the company the profile in use acts on",
    ),
    Example::new(
        "cad-cli company list --output json",
        "Print the accessible companies as JSON",
    ),
    Example::new(
        "cad-cli --company-id 00000000-0000-0000-0000-000000000001 reporting-period list",
        "Run a command for another accessible company",
    ),
    Example::new(
        "cad-cli --company-id - reporting-period list",
        "Pick the company from a list at a terminal for this one command; nothing is saved",
    ),
];

const PINK: &str = "38;2;255;85;120";
const YELLOW: &str = "38;2;255;200;50";
const PURPLE: &str = "38;2;150;130;240";
const TEAL: &str = "38;2;10;190;160";
const TEXT_TEAL: &str = "38;2;0;105;92";

/// Returns the root Clap command with the branded help-only header.
pub(crate) fn command_with_help_header() -> Command {
    Cli::command().before_help(help_header())
}

fn help_header() -> String {
    render_help_logo(std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none())
}

/// Renders the terminal-safe hash-mark rendition of CAD's four-part logo.
fn render_help_logo(colour: bool) -> String {
    let paint = |text: &str, ansi: &str| {
        if colour {
            format!("\x1b[{ansi}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    };
    let name = paint("Carbon+Alt+Delete", TEXT_TEAL);
    let description = paint("Command-Line Interface", TEXT_TEAL);

    [
        format!("       {}       ", paint("##", PINK)),
        format!("      {}      ", paint("####", PINK)),
        format!("     {}     ", paint("######", PINK)),
        format!("    {}    ", paint("########", PINK)),
        format!("  {}        {}  ", paint("##", YELLOW), paint("##", PURPLE)),
        format!(" {}        {} ", paint("###", YELLOW), paint("###", PURPLE)),
        format!(
            "{}        {}  {name}",
            paint("####", YELLOW),
            paint("####", PURPLE)
        ),
        format!(
            "{}        {}  {description}",
            paint("####", YELLOW),
            paint("####", PURPLE)
        ),
        format!(" {}        {} ", paint("###", YELLOW), paint("###", PURPLE)),
        format!("  {}        {}  ", paint("##", YELLOW), paint("##", PURPLE)),
        format!("    {}    ", paint("########", TEAL)),
        format!("     {}     ", paint("######", TEAL)),
        format!("      {}      ", paint("####", TEAL)),
        format!("       {}       ", paint("##", TEAL)),
    ]
    .join("\n")
}

#[derive(Debug, Parser)]
#[command(
    name = "cad-cli",
    // Set by build.rs: the package version, with a `-dev+<commit>` suffix
    // when the build names a commit through CAD_CLI_BUILD_COMMIT.
    version = env!("CAD_CLI_BUILD_VERSION"),
    about = None,
    // `-h` shows the examples; `--help` adds the environment variables.
    after_help = help::examples(EXAMPLES),
    after_long_help = help::environment_and_examples(ENVIRONMENT, EXAMPLES)
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

    use super::{Cli, render_help_logo};
    use crate::cmds;

    #[test]
    fn clap_definition_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn root_help_logo_is_sixteen_columns_without_empty_rows_and_names_cad() {
        let header = render_help_logo(false);
        let lines: Vec<_> = header.lines().collect();

        assert_eq!(lines.len(), 14);
        assert!(lines.iter().all(|line| line.chars().take(16).count() == 16));
        assert!(lines.iter().all(|line| !line[..16].trim().is_empty()));
        assert_eq!(lines[6], "####        ####  Carbon+Alt+Delete");
        assert_eq!(lines[7], "####        ####  Command-Line Interface");
    }

    #[test]
    fn interactive_help_logo_uses_all_brand_colours() {
        let header = render_help_logo(true);

        for colour in [
            super::PINK,
            super::YELLOW,
            super::PURPLE,
            super::TEAL,
            super::TEXT_TEAL,
        ] {
            assert!(header.contains(&format!("\x1b[{colour}m")));
        }
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
        assert_eq!(
            options.filters,
            ["country_inList=PL,,DE"
                .parse::<cad_api::domains::inventory_lines::InventoryFilter>()
                .expect("a valid filter")]
        );
    }

    #[test]
    fn malformed_inventory_queries_are_usage_errors() {
        for args in [
            ["--filter", "country"].as_slice(),
            &["--filter", "country_bogus=PL"],
            &["--filter", "country_inList="],
            &["--sort", "date"],
            &["--sort", "date_sideways"],
            &["--page", "0"],
            &["--limit", "251"],
        ] {
            let error = Cli::try_parse_from(
                ["cad-cli", "inventory-line", "list"]
                    .into_iter()
                    .chain(args.iter().copied()),
            )
            .expect_err("a malformed inventory query must not parse");
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ValueValidation,
                "{args:?}: {error}"
            );
        }
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
    fn profile_names_are_checked_by_the_parser() {
        for name in [
            "production",
            "dev",
            "a",
            "my_profile-2",
            "0abc",
            &"a".repeat(32),
        ] {
            let cli = Cli::try_parse_from(["cad-cli", "--profile", name, "company", "list"])
                .unwrap_or_else(|error| panic!("{name} must parse: {error}"));
            assert_eq!(cli.global.profile.unwrap().as_str(), name);
        }
        for name in ["", "Prod", "-a", "_a", "a b", "a.b", "é", &"a".repeat(33)] {
            let error =
                Cli::try_parse_from(["cad-cli", &format!("--profile={name}"), "company", "list"])
                    .expect_err("an invalid name must not parse");
            assert_eq!(
                error.kind(),
                clap::error::ErrorKind::ValueValidation,
                "{name:?}"
            );
            assert!(error.to_string().contains("1 to 32 characters"), "{name:?}");
        }
    }

    #[test]
    fn the_environment_flag_is_gone() {
        Cli::try_parse_from(["cad-cli", "--environment", "staging", "company", "list"])
            .expect_err("--environment was removed in favour of profiles");
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
