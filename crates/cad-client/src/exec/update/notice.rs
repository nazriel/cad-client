//! The one-line notice, printed after a command, that a newer release exists.
//!
//! The check runs on a background task started before the command and is
//! read only if it has already finished when the command ends: it never
//! delays a command. An answer from the network is kept in a small state file
//! in the user's cache directory and reused for [`CHECK_INTERVAL`]; a check
//! that got no answer (it failed, or the command ended first) is retried after
//! [`RETRY_INTERVAL`]. Every failure is silent.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use semver::Version;
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use tokio::task::JoinHandle;

use super::{ReleaseSource, available_message, current_version};
use crate::cli::OutputFormat;
use crate::config::Environment;

/// Turns the notice off when set to a true value (see [`is_true`]).
pub(crate) const DISABLE_VARIABLE: &str = "CAD_CLI_NO_UPDATE_CHECK";
/// How long an answer from the network is reused.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_hours(24);
/// How long after an unanswered check the network is asked again.
pub(crate) const RETRY_INTERVAL: Duration = Duration::from_hours(1);
/// Budget for the background request; a slower answer is not worth waiting for.
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);
/// Directory under the user cache directory, and the state file inside it.
const CACHE_DIR: &str = "cad-cli";
const STATE_FILE: &str = "update-check.json";

/// Whether the environment and invocation allow a background check at all.
///
/// The notice is for a person at a terminal. It is off when stderr is not one,
/// when the output is machine-readable, and when `CI` or [`DISABLE_VARIABLE`]
/// is true. Which commands take part is decided by the command definition.
pub(crate) fn enabled(
    environment: &impl Environment,
    output: OutputFormat,
    stderr_is_terminal: bool,
) -> bool {
    let set = |key| environment.get(key).is_some_and(|value| is_true(&value));
    stderr_is_terminal && output == OutputFormat::Table && !set("CI") && !set(DISABLE_VARIABLE)
}

/// Reads a flag variable. Empty, `0`, `false`, `no` and `off`, in any case and
/// with surrounding spaces, are false. Every other value is true, typos
/// included: whoever sets a variable named "no update check" most likely
/// wants no check.
pub(crate) fn is_true(value: &str) -> bool {
    let value = value.trim();
    !(value.is_empty()
        || ["0", "false", "no", "off"]
            .iter()
            .any(|off| value.eq_ignore_ascii_case(off)))
}

/// A check running alongside the command.
pub(crate) struct BackgroundCheck {
    task: JoinHandle<Option<Version>>,
}

impl BackgroundCheck {
    /// Starts the check, or returns `None` when there is no cache directory to
    /// throttle it with or the release source cannot be configured.
    pub(crate) fn start(environment: &impl Environment) -> Option<Self> {
        let state = state_path(environment, std::env::consts::OS)?;
        let source = ReleaseSource::from_environment(environment, CHECK_TIMEOUT).ok()?;
        let task =
            tokio::spawn(async move { latest_known(&state, &source, SystemTime::now()).await });
        Some(Self { task })
    }

    /// The notice to print, if the check has already finished and found a
    /// newer release. It never waits: an unfinished check is abandoned.
    pub(crate) async fn finish(self) -> Option<String> {
        if !self.task.is_finished() {
            self.task.abort();
            return None;
        }
        let latest = self.task.await.ok()??;
        available_message(&latest, &current_version())
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
}

/// What the state file remembers between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct State {
    /// When the network last answered, in seconds since the Unix epoch.
    #[serde(default)]
    pub(crate) checked_at: Option<u64>,
    /// When the network was last asked, answered or not.
    #[serde(default)]
    pub(crate) attempted_at: Option<u64>,
    /// The latest release version it reported, if any check succeeded.
    pub(crate) latest: Option<String>,
}

/// `<cache dir>/cad-cli/update-check.json`, where the cache directory is
/// `%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS and
/// `$XDG_CACHE_HOME` (default `~/.cache`) elsewhere.
pub(crate) fn state_path(environment: &impl Environment, os: &str) -> Option<PathBuf> {
    let absolute = |key| {
        environment
            .get(key)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let cache = match os {
        "windows" => absolute("LOCALAPPDATA"),
        "macos" => absolute("HOME").map(|home| home.join("Library").join("Caches")),
        _ => {
            absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
        }
    }?;
    Some(cache.join(CACHE_DIR).join(STATE_FILE))
}

/// Whether a network check is due at `now`: not while the last answer is
/// younger than [`CHECK_INTERVAL`], nor while the last attempt is younger than
/// [`RETRY_INTERVAL`].
///
/// A timestamp in the future means the clock moved; it does not count, so the
/// check is not suppressed until the clock catches up.
pub(crate) fn due(state: Option<&State>, now: SystemTime) -> bool {
    let Some(state) = state else {
        return true;
    };
    let within = |at: Option<u64>, interval: Duration| {
        at.is_some_and(|seconds| {
            now.duration_since(UNIX_EPOCH + Duration::from_secs(seconds))
                .is_ok_and(|elapsed| elapsed < interval)
        })
    };
    !within(state.checked_at, CHECK_INTERVAL) && !within(state.attempted_at, RETRY_INTERVAL)
}

/// The latest release version known at `now`: from the state file while it is
/// fresh, otherwise from `source`, recording the answer for the next run.
pub(crate) async fn latest_known(
    path: &Path,
    source: &ReleaseSource,
    now: SystemTime,
) -> Option<Version> {
    let previous = read_state(path);
    let cached = previous
        .as_ref()
        .and_then(|state| state.latest.as_deref())
        .and_then(|latest| Version::parse(latest).ok());
    if !due(previous.as_ref(), now) {
        return cached;
    }

    // The attempt is recorded before the request, so a check that fails or is
    // abandoned when the command ends waits RETRY_INTERVAL before the next
    // one, however many commands run meanwhile. Without a writable state file
    // there is no throttle, so there is no request either.
    let attempted_at = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    let claim = State {
        checked_at: previous.as_ref().and_then(|state| state.checked_at),
        attempted_at: Some(attempted_at),
        latest: cached.as_ref().map(ToString::to_string),
    };
    write_state(path, &claim).ok()?;

    match source.latest().await {
        Ok(release) => {
            let _ = write_state(
                path,
                &State {
                    checked_at: Some(attempted_at),
                    attempted_at: Some(attempted_at),
                    latest: Some(release.version.to_string()),
                },
            );
            Some(release.version)
        }
        Err(_) => cached,
    }
}

pub(crate) fn read_state(path: &Path) -> Option<State> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Replaces the state file atomically, so a concurrent run never reads half a
/// file.
pub(crate) fn write_state(path: &Path, state: &State) -> std::io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| std::io::Error::other("the state file has no directory"))?;
    std::fs::create_dir_all(directory)?;
    let mut file = NamedTempFile::new_in(directory)?;
    serde_json::to_writer(&mut file, state)?;
    file.flush()?;
    file.persist(path).map_err(|error| error.error)?;
    Ok(())
}
