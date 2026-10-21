//! `cad-cli update`: replace the running binary with the latest release.
//!
//! The command needs neither platform credentials nor configuration. It
//! returns a small JSON object in the selected output format; progress and
//! the outcome are reported on stderr.

#[cfg(test)]
mod tests;

use std::path::Path;
use std::time::Duration;

use clap::Args;
use semver::Version;
use serde::Serialize;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::help::{self, Example};
use crate::config::Environment;
use crate::error::CliError;
use crate::exec::update::notes::{self, ReleaseNotes};
use crate::exec::update::target::Target;
use crate::exec::update::{self, ReleaseSource, install};
use crate::support::terminal::StatusKind;

const EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli update",
        "Install the latest release over the running binary",
    ),
    Example::new(
        "cad-cli update --check",
        "Report whether a newer release exists; exits 10 if one does",
    ),
    Example::new(
        "cad-cli update --force --output json",
        "Reinstall the latest release even if it is not newer",
    ),
];

/// Exit status of `update --check` when a newer release exists.
pub(crate) const UPDATE_AVAILABLE_STATUS: u8 = 10;

/// Arguments of `cad-cli update`. Its help summary is the doc comment on
/// [`crate::cmds::Command::Update`].
#[derive(Debug, Args)]
#[command(after_long_help = help::examples(EXAMPLES))]
pub struct UpdateArgs {
    /// Only report whether a newer release exists: exit 10 if one does, 0 if
    /// this binary is up to date.
    #[arg(long, conflicts_with = "force")]
    pub(crate) check: bool,

    /// Reinstall the latest release even when it is not newer than this binary.
    #[arg(long)]
    pub(crate) force: bool,
}

/// The command's stdout value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateResult {
    /// Version of the binary that ran the command.
    pub(crate) current: String,
    /// Version of the latest published release.
    pub(crate) latest: String,
    /// Whether `latest` is newer than `current`.
    pub(crate) update_available: bool,
    /// Whether the executable was replaced.
    pub(crate) updated: bool,
}

/// What the command produced: data for stdout, a closing status line for
/// stderr, release notes to show after an update, and the exit status.
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) value: Value,
    pub(crate) status: StatusKind,
    pub(crate) summary: String,
    /// Shown on stderr only with table output and only at a terminal.
    pub(crate) notes: Option<ReleaseNotes>,
    pub(crate) exit_status: u8,
}

impl UpdateArgs {
    /// Checks for, and unless `--check` is given installs, the latest release.
    ///
    /// # Errors
    ///
    /// Fails when no release is published for this platform, the release
    /// cannot be fetched or verified, the new binary does not report the
    /// release version when run, or the executable cannot be replaced.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        environment: &impl Environment,
        timeout: Duration,
    ) -> Result<Report, CliError> {
        let target = Target::current().ok_or_else(|| {
            CliError::Update(format!(
                "no release is published for {}/{}",
                std::env::consts::OS,
                std::env::consts::ARCH
            ))
        })?;
        let source = ReleaseSource::from_environment(environment, timeout)?;
        self.run_with(
            context,
            &source,
            target,
            &update::current_version(),
            install::install,
        )
        .await
    }

    /// [`Self::run`] with the running version and the install step (probe,
    /// then replace) supplied, so tests exercise the whole workflow without
    /// overwriting the test binary.
    pub(crate) async fn run_with(
        self,
        context: &CommandContext<'_>,
        source: &ReleaseSource,
        target: Target,
        current: &Version,
        install: impl AsyncFnOnce(&Path, &Version) -> Result<(), CliError>,
    ) -> Result<Report, CliError> {
        let release = context
            .during("checking for a newer cad-cli release", source.latest())
            .await?;
        let mut result = UpdateResult {
            current: current.to_string(),
            latest: release.version.to_string(),
            update_available: release.version > *current,
            updated: false,
        };

        if self.check && result.update_available {
            let summary = update::available_message(&release.version, current).unwrap_or_default();
            return Report::new(&result, StatusKind::Info, summary, UPDATE_AVAILABLE_STATUS);
        }
        // Without --force an older latest release is never installed.
        if self.check || !(result.update_available || self.force) {
            let summary = format!("cad-cli {current} is up to date");
            return Report::new(&result, StatusKind::Success, summary, 0);
        }

        let archive = target.archive_name(&release.version);
        let staging = update::install_directory();
        let binary = context
            .during(
                &format!("downloading {archive}"),
                source.fetch_binary(&release, target, staging.as_deref()),
            )
            .await?;
        install(&binary, &release.version).await?;
        result.updated = true;
        let summary = format!("updated cad-cli {current} to {}", release.version);
        let mut report = Report::new(&result, StatusKind::Success, summary, 0)?;
        report.notes = Some(notes::after_update(&release, current));
        Ok(report)
    }
}

impl Report {
    fn new(
        result: &UpdateResult,
        status: StatusKind,
        summary: String,
        exit_status: u8,
    ) -> Result<Self, CliError> {
        Ok(Self {
            value: crate::output::to_value(result)?,
            status,
            summary,
            notes: None,
            exit_status,
        })
    }
}
