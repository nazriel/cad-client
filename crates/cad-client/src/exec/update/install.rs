//! Installing a downloaded executable: run it once to prove it works on this
//! machine, then swap it in for the running one.

use std::io;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use semver::Version;

use crate::error::CliError;

/// How long the new binary gets to answer `--version`.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// How much of an unexpected `--version` answer an error message quotes.
const QUOTED_OUTPUT_CHARS: usize = 80;

/// Checks that `binary` runs here and is the release it claims to be, then
/// atomically replaces the running executable with it.
///
/// # Errors
///
/// Fails, leaving the running executable untouched, when the probe fails or
/// the executable cannot be replaced.
pub(crate) async fn install(binary: &Path, version: &Version) -> Result<(), CliError> {
    probe(binary, version).await?;
    replace_running_executable(binary)
}

/// Runs `binary --version` with no input and requires it to print exactly
/// `cad-cli <version>`.
///
/// A release built for another architecture or C library, a truncated file,
/// or a binary blocked by the system fails here, before it can replace a
/// working executable.
///
/// # Errors
///
/// Fails when the binary cannot be started, does not finish within
/// [`PROBE_TIMEOUT`], exits unsuccessfully, or prints anything else.
pub(crate) async fn probe(binary: &Path, version: &Version) -> Result<(), CliError> {
    let does_not_run = |detail: String| {
        CliError::Update(format!(
            "the downloaded binary does not run on this machine: {detail}; nothing was installed"
        ))
    };
    let child = tokio::process::Command::new(binary)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // A probe that times out is dropped, and with it the process.
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| does_not_run(error.to_string()))?;
    let output = tokio::time::timeout(PROBE_TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| {
            does_not_run(format!(
                "`--version` did not finish within {}s",
                PROBE_TIMEOUT.as_secs()
            ))
        })?
        .map_err(|error| does_not_run(error.to_string()))?;
    if !output.status.success() {
        return Err(does_not_run(format!(
            "`--version` failed ({})",
            output.status
        )));
    }

    let expected = format!("cad-cli {version}");
    let printed = String::from_utf8_lossy(&output.stdout);
    let printed = printed.trim_end();
    if printed == expected {
        return Ok(());
    }
    let quoted: String = printed
        .chars()
        .filter(|character| !character.is_control())
        .take(QUOTED_OUTPUT_CHARS)
        .collect();
    Err(CliError::Update(format!(
        "the downloaded binary reports `{quoted}` instead of `{expected}`; nothing was installed"
    )))
}

/// Atomically replaces the running executable with `binary`.
///
/// On Unix the new file is written beside the executable and renamed over it;
/// on Windows the running file is moved aside first, as it cannot be
/// overwritten while it runs. `self_replace` handles both.
///
/// # Errors
///
/// Fails when the executable's directory is not writable, typically because
/// it was installed into a system directory.
pub(crate) fn replace_running_executable(binary: &Path) -> Result<(), CliError> {
    self_replace::self_replace(binary).map_err(|error| {
        let location = std::env::current_exe().map_or_else(
            |_| "the running executable".to_owned(),
            |path| path.display().to_string(),
        );
        let hint = if error.kind() == io::ErrorKind::PermissionDenied {
            "; re-run with permission to write there, or reinstall with the install script"
        } else {
            ""
        };
        CliError::Io(io::Error::new(
            error.kind(),
            format!("cannot replace {location}: {error}{hint}"),
        ))
    })
}
