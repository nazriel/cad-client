//! Artefacts generated from the command definition itself.
//!
//! Both outputs are derived from [`crate::cli::Cli`], so a new flag or command
//! shows up in the completions and the man page without a second definition to
//! keep in sync.

use clap::{Args, CommandFactory};
use clap_complete::Shell;

use crate::cli::Cli;
use crate::error::CliError;
use crate::support::terminal::Terminal;

/// Binary name written into the generated artefacts.
const BINARY: &str = "cad-cli";

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub(crate) shell: Shell,
}

impl CompletionsArgs {
    pub(crate) fn run(self, terminal: &dyn Terminal) -> Result<(), CliError> {
        let mut buffer = Vec::new();
        clap_complete::generate(self.shell, &mut Cli::command(), BINARY, &mut buffer);
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
