//! Artefacts generated from the command definition itself.
//!
//! The man page is derived from [`crate::cli::Cli`], and shell completion asks
//! the binary for candidates at Tab time, so a new flag or command shows up in
//! both without a second definition to keep in sync.

use clap::{Args, CommandFactory};
use clap_complete::Shell;

use crate::cli::Cli;
use crate::error::CliError;
use crate::support::terminal::Terminal;

/// Binary name written into the generated artefacts.
const BINARY: &str = "cad-cli";

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to print the completion registration for.
    #[arg(value_enum)]
    pub(crate) shell: Shell,
}

impl CompletionsArgs {
    /// Prints the dynamic registration, identical to `COMPLETE=<shell> cad-cli`.
    ///
    /// Printing the registration rather than a static script means that every
    /// way of installing completions, sourced or written to a file, also gets
    /// the platform-backed candidates.
    pub(crate) fn run(self, terminal: &dyn Terminal) -> Result<(), CliError> {
        let mut buffer = Vec::new();
        crate::completion::write_registration(self.shell, &mut buffer).map_err(CliError::Io)?;
        terminal.write_bytes(&buffer)
    }
}

/// Renders the top-level manual page in roff format.
pub(crate) fn man(terminal: &dyn Terminal) -> Result<(), CliError> {
    let mut buffer = Vec::new();
    clap_mangen::Man::new(Cli::command().name(BINARY))
        .render(&mut buffer)
        .map_err(CliError::Io)?;
    terminal.write_bytes(&buffer)
}
