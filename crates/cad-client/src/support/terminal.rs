use std::io::{IsTerminal, Read, Write};
use std::time::Duration;

use serde_json::Value;

use crate::cli::OutputFormat;
use crate::error::CliError;
use crate::output::TableStyle;

/// What a status message means, independent of how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusKind {
    Success,
    Info,
    Warning,
    Error,
}

impl StatusKind {
    /// Symbol shown in front of the message on a terminal.
    const fn symbol(self) -> &'static str {
        match self {
            Self::Success => "✔",
            Self::Info => "ℹ",
            Self::Warning => "⚠",
            Self::Error => "✖",
        }
    }

    fn styled_symbol(self) -> console::StyledObject<&'static str> {
        let styled = console::style(self.symbol()).for_stderr();
        match self {
            Self::Success => styled.green(),
            Self::Info => styled.cyan(),
            Self::Warning => styled.yellow(),
            Self::Error => styled.red(),
        }
    }
}

/// A running indicator for a long operation.
///
/// It deliberately has no methods: the whole interface is the value's lifetime.
/// Dropping it stops and clears the indicator, so an early return — including
/// the Ctrl-C arm of a `tokio::select!` — cannot leave a spinner on the screen.
pub(crate) trait Progress: Send {}

/// Process I/O used by command handlers.
pub(crate) trait Terminal: Send + Sync {
    /// Writes a user-facing status message to stderr.
    fn status(&self, kind: StatusKind, message: &str) -> Result<(), CliError>;

    /// Confirms a destructive operation, unless confirmation was explicitly skipped.
    fn confirm(&self, prompt: &str, assume_yes: bool) -> Result<(), CliError>;

    /// Reads all bytes from the process standard input.
    fn read_stdin(&self) -> Result<Vec<u8>, CliError>;

    /// Writes command data to stdout in the selected presentation format.
    fn write_output(&self, value: &Value, format: OutputFormat) -> Result<(), CliError>;

    /// Writes bytes to stdout that are already in their final form, such as a
    /// generated completion script or manual page.
    fn write_bytes(&self, bytes: &[u8]) -> Result<(), CliError>;

    /// Starts an indicator that runs until the returned value is dropped.
    fn progress(&self, message: &str) -> Box<dyn Progress>;
}

/// Terminal backed by the current process standard streams.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ProcessTerminal;

impl Terminal for ProcessTerminal {
    fn status(&self, kind: StatusKind, message: &str) -> Result<(), CliError> {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        // Outside a terminal the message is written exactly as it always was:
        // captured stderr belongs to logs and scripts, not to decoration.
        if stderr.is_terminal() {
            writeln!(stderr, "{} {message}", kind.styled_symbol())?;
        } else {
            writeln!(stderr, "{message}")?;
        }
        Ok(())
    }

    fn confirm(&self, prompt: &str, assume_yes: bool) -> Result<(), CliError> {
        if assume_yes {
            return Ok(());
        }

        let stdin = std::io::stdin();
        if !stdin.is_terminal() {
            return Err(CliError::Input(
                "destructive operations outside a TTY require --yes".to_owned(),
            ));
        }

        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        write!(stderr, "{prompt} [y/N] ")?;
        stderr.flush()?;

        let mut answer = String::new();
        stdin.read_line(&mut answer)?;
        if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            Ok(())
        } else {
            Err(CliError::Cancelled)
        }
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        let mut bytes = Vec::new();
        stdin.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn write_output(&self, value: &Value, format: OutputFormat) -> Result<(), CliError> {
        let stdout = std::io::stdout();
        crate::output::write_styled(value, format, &mut stdout.lock(), TableStyle::detect())
    }

    fn write_bytes(&self, bytes: &[u8]) -> Result<(), CliError> {
        let stdout = std::io::stdout();
        stdout.lock().write_all(bytes)?;
        Ok(())
    }

    fn progress(&self, message: &str) -> Box<dyn Progress> {
        if std::io::stderr().is_terminal() {
            return Box::new(Spinner::start(message));
        }
        // A redirected stream must not collect spinner frames, but it should
        // still learn that a long operation started — one line, exactly what the
        // plain status call printed before there were spinners.
        let _ = self.status(StatusKind::Info, message);
        Box::new(SilentProgress)
    }
}

/// Indicator for streams that cannot be redrawn.
struct SilentProgress;

impl Progress for SilentProgress {}

struct Spinner(indicatif::ProgressBar);

impl Spinner {
    /// Frame interval fast enough to read as motion, slow enough not to churn.
    const TICK: Duration = Duration::from_millis(80);

    fn start(message: &str) -> Self {
        let bar =
            indicatif::ProgressBar::with_draw_target(None, indicatif::ProgressDrawTarget::stderr());
        if let Ok(style) = indicatif::ProgressStyle::with_template("{spinner} {msg}") {
            bar.set_style(style);
        }
        bar.set_message(message.to_owned());
        bar.enable_steady_tick(Self::TICK);
        Self(bar)
    }
}

impl Progress for Spinner {}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.0.finish_and_clear();
    }
}

/// Resolves when the process receives Ctrl-C.
pub(crate) async fn cancelled() {
    let _ = tokio::signal::ctrl_c().await;
}

/// A terminal that swallows everything, for tests whose subject is a workflow
/// rather than what the user sees.
#[cfg(test)]
pub(crate) struct SilentTerminal;

#[cfg(test)]
impl Progress for () {}

#[cfg(test)]
impl Terminal for SilentTerminal {
    fn status(&self, _kind: StatusKind, _message: &str) -> Result<(), CliError> {
        Ok(())
    }

    fn confirm(&self, _prompt: &str, _assume_yes: bool) -> Result<(), CliError> {
        Ok(())
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(Vec::new())
    }

    fn write_output(&self, _value: &Value, _format: OutputFormat) -> Result<(), CliError> {
        Ok(())
    }

    fn write_bytes(&self, _bytes: &[u8]) -> Result<(), CliError> {
        Ok(())
    }

    fn progress(&self, _message: &str) -> Box<dyn Progress> {
        Box::new(())
    }
}
