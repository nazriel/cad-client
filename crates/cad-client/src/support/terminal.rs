use std::io::{IsTerminal, Read, Write};
use std::time::Duration;

use secrecy::SecretString;
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

    /// Writes a block of reading matter, such as release notes, to stderr
    /// under a heading. It is for a person at a terminal: when stderr is not
    /// one, nothing is written.
    fn notes(&self, heading: &str, body: &str) -> Result<(), CliError>;

    /// Confirms a consequential operation, unless confirmation was explicitly skipped.
    fn confirm(&self, prompt: &str, assume_yes: bool) -> Result<(), CliError>;

    /// Reads all bytes from the process standard input.
    fn read_stdin(&self) -> Result<Vec<u8>, CliError>;

    /// Whether a person can answer a prompt: standard input and standard
    /// error are both terminals.
    fn is_interactive(&self) -> bool;

    /// Asks for one line of text on stderr, with the typed text shown.
    fn prompt(&self, label: &str) -> Result<String, CliError>;

    /// Asks for one line of text on stderr, with the typed text hidden.
    fn prompt_secret(&self, label: &str) -> Result<SecretString, CliError>;

    /// Asks a yes/no question on stderr; only an explicit yes is a yes. Unlike
    /// [`Terminal::confirm`], `--yes` does not answer it.
    fn ask(&self, question: &str) -> Result<bool, CliError>;

    /// Opens `url` in the user's browser. Returns whether a browser was
    /// started: on a machine with no display there is none to start.
    fn open_url(&self, url: &str) -> bool;

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

    fn notes(&self, heading: &str, body: &str) -> Result<(), CliError> {
        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        if !stderr.is_terminal() {
            return Ok(());
        }
        let heading = console::style(heading).bold().for_stderr();
        writeln!(stderr, "\n{heading}")?;
        if !body.is_empty() {
            writeln!(stderr, "{body}")?;
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
                "operations that require confirmation outside a TTY require --yes".to_owned(),
            ));
        }

        let stderr = std::io::stderr();
        let mut stderr = stderr.lock();
        write!(stderr, "{prompt} (y/N) ")?;
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

    fn is_interactive(&self) -> bool {
        std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
    }

    fn prompt(&self, label: &str) -> Result<String, CliError> {
        let terminal = console::Term::stderr();
        terminal.write_str(&format!("{label} "))?;
        Ok(terminal.read_line()?)
    }

    fn prompt_secret(&self, label: &str) -> Result<SecretString, CliError> {
        let terminal = console::Term::stderr();
        terminal.write_str(&format!("{label} "))?;
        Ok(SecretString::from(terminal.read_secure_line()?))
    }

    fn ask(&self, question: &str) -> Result<bool, CliError> {
        let answer = self.prompt(&format!("{question} (y/N)"))?;
        Ok(matches!(
            answer.trim().to_ascii_lowercase().as_str(),
            "y" | "yes"
        ))
    }

    fn open_url(&self, url: &str) -> bool {
        let Some(mut command) = browser_command(url, |name| {
            std::env::var_os(name).is_some_and(|value| !value.is_empty())
        }) else {
            return false;
        };
        let Ok(mut child) = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            return false;
        };
        // Some openers stay in the foreground for as long as the browser runs,
        // so nothing waits for it here; a thread reaps it whenever it ends.
        std::thread::spawn(move || child.wait());
        true
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

/// The command that opens `url` in a browser on this platform, or `None`
/// where there is none to open: Linux without a display. `is_set` says
/// whether an environment variable is set and not empty.
fn browser_command(url: &str, is_set: impl Fn(&str) -> bool) -> Option<std::process::Command> {
    let mut command = match std::env::consts::OS {
        "macos" => std::process::Command::new("open"),
        "windows" => {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "start", ""]);
            command
        }
        _ if is_set("DISPLAY") || is_set("WAYLAND_DISPLAY") => {
            std::process::Command::new("xdg-open")
        }
        _ => return None,
    };
    command.arg(url);
    Some(command)
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

    fn notes(&self, _heading: &str, _body: &str) -> Result<(), CliError> {
        Ok(())
    }

    fn confirm(&self, _prompt: &str, _assume_yes: bool) -> Result<(), CliError> {
        Ok(())
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(Vec::new())
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn prompt(&self, _label: &str) -> Result<String, CliError> {
        Err(CliError::Input("no terminal to ask on".to_owned()))
    }

    fn prompt_secret(&self, _label: &str) -> Result<SecretString, CliError> {
        Err(CliError::Input("no terminal to ask on".to_owned()))
    }

    fn ask(&self, _question: &str) -> Result<bool, CliError> {
        Ok(false)
    }

    fn open_url(&self, _url: &str) -> bool {
        false
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

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::browser_command;

    fn arguments(command: &Command) -> Vec<String> {
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_opens_a_browser_only_with_a_display() {
        let url = "https://example.test/app";
        assert!(browser_command(url, |_| false).is_none());
        for variable in ["DISPLAY", "WAYLAND_DISPLAY"] {
            let command = browser_command(url, |name| name == variable).expect("a display");
            assert_eq!(command.get_program(), "xdg-open");
            assert_eq!(arguments(&command), [url]);
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_always_has_a_browser() {
        let command = browser_command("https://example.test/", |_| false).expect("open");
        assert_eq!(command.get_program(), "open");
        assert_eq!(arguments(&command), ["https://example.test/"]);
    }

    #[cfg(windows)]
    #[test]
    fn windows_starts_the_default_browser() {
        let command = browser_command("https://example.test/", |_| false).expect("start");
        assert_eq!(command.get_program(), "cmd");
        assert_eq!(
            arguments(&command),
            ["/C", "start", "", "https://example.test/"]
        );
    }
}
