pub(crate) mod context;
pub(crate) mod platform;

#[cfg(test)]
mod tests;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::process::ExitCode;

use cad_api::Client;
use clap::{ArgMatches, FromArgMatches};

use self::context::CommandContext;
use self::platform::Platform;
use crate::cli::args::LocalInputPath;
use crate::cli::{Cli, OutputFormat, command_with_help_header};
use crate::cmds::Invocation;
use crate::config::{AppConfig, Environment, ProcessEnvironment};
use crate::error::CliError;
use crate::exec::update::notice::{self, BackgroundCheck};
use crate::support::terminal::{ProcessTerminal, StatusKind, Terminal};

/// Parses the supplied process arguments and executes the application.
pub async fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let parsed = command_with_help_header()
        .try_get_matches_from(args)
        .and_then(|matches| Cli::from_arg_matches(&matches).map(|cli| (cli, matches)));
    let (cli, matches) = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from);
        }
    };

    let terminal = ProcessTerminal;
    let environment = ProcessEnvironment;
    // Started before the command so it runs alongside it; read afterwards only
    // if it has already finished.
    let update_check = (cli.command.checks_for_updates()
        && notice::enabled(
            &environment,
            cli.global.output,
            std::io::stderr().is_terminal(),
        ))
    .then(|| BackgroundCheck::start(&environment))
    .flatten();

    let code = match execute(cli, &matches, &environment, &terminal).await {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            let _ = terminal.status(StatusKind::Error, &error.to_string());
            error.exit_code()
        }
    };

    if let Some(check) = update_check
        && let Some(message) = check.finish().await
    {
        let _ = terminal.status(StatusKind::Info, &message);
    }
    code
}

/// Runs the parsed command and returns the process exit status.
async fn execute(
    cli: Cli,
    matches: &ArgMatches,
    environment: &impl Environment,
    terminal: &dyn Terminal,
) -> Result<u8, CliError> {
    let output = cli.global.output;
    let command_context = CommandContext::new(terminal, cli.global.yes);
    // Artefacts generated from the command definition need no server and no
    // credentials, so they are dispatched before configuration is even resolved.
    let online = match cli.command.into_invocation() {
        Invocation::Offline(command) => return command.run(terminal).map(|()| 0),
        Invocation::Standalone(command) => {
            let report = command
                .run(&command_context, environment, cli.global.request_timeout)
                .await?;
            terminal.write_output(&report.value, output)?;
            terminal.status(report.status, &report.summary)?;
            // Reading matter is for the table view; a caller asking for JSON
            // is a program and gets the data alone.
            if output == OutputFormat::Table
                && let Some(notes) = &report.notes
            {
                terminal.notes(&notes.heading, &notes.body)?;
            }
            return Ok(report.exit_status);
        }
        Invocation::Online(online) => online,
    };

    let (server, credentials, company_id) =
        AppConfig::resolve(&cli.global, environment)?.into_parts();
    // Local preconditions are checked before the identity request so a typo
    // in a path does not cost a network round-trip to fail on.
    check_local_inputs(matches)?;
    let connected = Client::new(server)?.connect(credentials).await?;
    let platform = Platform::new(connected, company_id);

    let result = online.run(&command_context, &platform).await?;

    // Data first, confirmation second: stdout stays pure, and the reader sees
    // that the command finished rather than guessing from an empty result.
    terminal.write_output(&result, output)?;
    let (kind, summary) = completion_summary(&result);
    terminal.status(kind, &summary)?;
    Ok(0)
}

/// Short confirmation written to stderr once the data has been emitted.
///
/// An empty collection is reported as a warning rather than a success: nothing
/// went wrong, but an empty answer is far more often a filter that missed than
/// a result the caller expected.
fn completion_summary(value: &serde_json::Value) -> (StatusKind, String) {
    match value {
        serde_json::Value::Array(items) if items.is_empty() => {
            (StatusKind::Warning, "no results".to_owned())
        }
        serde_json::Value::Array(items) if items.len() == 1 => {
            (StatusKind::Success, "1 result".to_owned())
        }
        serde_json::Value::Array(items) => {
            (StatusKind::Success, format!("{} results", items.len()))
        }
        serde_json::Value::Null => (StatusKind::Success, "done".to_owned()),
        _ => (StatusKind::Success, "1 result".to_owned()),
    }
}

/// Verifies that every local path the invocation names can actually be opened.
///
/// The selection is by *type*: every argument whose value is a
/// [`LocalInputPath`] is checked, whatever it is called and whichever command
/// declares it. A mismatched argument simply fails the downcast, so this walk
/// never depends on a list of argument names being kept in sync.
fn check_local_inputs(matches: &ArgMatches) -> Result<(), CliError> {
    let mut current = matches;
    loop {
        for id in current.ids() {
            if let Ok(Some(path)) = current.try_get_one::<LocalInputPath>(id.as_str()) {
                check_readable(path)?;
            }
        }
        match current.subcommand() {
            Some((_, sub)) => current = sub,
            None => return Ok(()),
        }
    }
}

fn check_readable(path: &LocalInputPath) -> Result<(), CliError> {
    if path.is_stdin() {
        return Ok(());
    }
    std::fs::File::open(path.as_path())
        .map(drop)
        .map_err(|error| {
            CliError::Input(format!("cannot read {}: {error}", path.as_path().display()))
        })
}
