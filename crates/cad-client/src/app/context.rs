use std::future::Future;

use serde::de::DeserializeOwned;

use crate::cli::args::LocalInputPath;
use crate::error::CliError;
use crate::support::terminal::Terminal;

pub(crate) struct CommandContext<'a> {
    terminal: &'a dyn Terminal,
    assume_yes: bool,
}

impl<'a> CommandContext<'a> {
    pub(crate) const fn new(terminal: &'a dyn Terminal, assume_yes: bool) -> Self {
        Self {
            terminal,
            assume_yes,
        }
    }

    /// The process I/O, for commands that prompt or open a browser.
    pub(crate) const fn terminal(&self) -> &'a dyn Terminal {
        self.terminal
    }

    pub(crate) fn confirm(&self, prompt: &str) -> Result<(), CliError> {
        self.terminal.confirm(prompt, self.assume_yes)
    }

    /// Runs `operation` while an indicator reports that something is happening.
    ///
    /// The indicator is bound to the scope, so it is cleared whether the
    /// operation finishes, fails, or loses a `select!` race against Ctrl-C.
    pub(crate) async fn during<F: Future>(&self, message: &str, operation: F) -> F::Output {
        let _progress = self.terminal.progress(message);
        operation.await
    }

    pub(crate) fn read_json<T: DeserializeOwned>(
        &self,
        path: &LocalInputPath,
    ) -> Result<T, CliError> {
        crate::input::read_json(path, self.terminal)
    }
}
