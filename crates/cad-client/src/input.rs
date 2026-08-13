use serde::de::DeserializeOwned;

use crate::cli::args::LocalInputPath;
use crate::error::CliError;
use crate::support::terminal::Terminal;

/// Reads and deserializes JSON from a file or from stdin when `path` is `-`.
///
/// # Errors
///
/// Returns [`CliError`] for I/O failures and malformed or schema-invalid JSON.
pub(crate) fn read_json<T: DeserializeOwned>(
    path: &LocalInputPath,
    terminal: &dyn Terminal,
) -> Result<T, CliError> {
    let bytes = if path.is_stdin() {
        terminal.read_stdin()?
    } else {
        std::fs::read(path.as_path())?
    };

    serde_json::from_slice(&bytes).map_err(|error| {
        CliError::Input(format!(
            "could not parse camelCase JSON from {}: {error}",
            path.as_path().display()
        ))
    })
}
