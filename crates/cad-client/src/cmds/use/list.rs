//! `cad-cli use`.

use super::{ProfileRow, Report};
use crate::cli::GlobalArgs;
use crate::config::{Environment, select_profile};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::profiles::{ProfileName, ProfileSet, ProfileStore};
use crate::support::terminal::StatusKind;

/// The rows of every profile, sorted by name; `selected` marks the one named.
pub(super) fn rows(set: &ProfileSet, selected: Option<&ProfileName>) -> Vec<ProfileRow> {
    set.iter()
        .map(|(name, profile)| {
            ProfileRow::new(
                name,
                profile,
                set.active() == Some(name),
                selected == Some(name),
            )
        })
        .collect()
}

/// Lists the profiles from the profiles file alone: no request, no keyring.
pub(super) fn run(
    global: &GlobalArgs,
    environment: &impl Environment,
    profiles: &ProfileStore,
) -> Result<Report, CliError> {
    let set = profiles.load()?;
    let selected = select_profile(global, environment, &set)?;
    let rows = rows(&set, selected.as_ref().map(|selected| &selected.name));

    let value = to_value(&rows)?;
    let message = match (&selected, set.is_empty()) {
        (Some(selected), _) => (
            StatusKind::Info,
            format!(
                "using profile {} ({})",
                selected.name,
                selected.source.describe()
            ),
        ),
        (None, true) => (
            StatusKind::Warning,
            "no profile yet; run `cad-cli use profile <name>`".to_owned(),
        ),
        (None, false) => (
            StatusKind::Warning,
            "no profile is selected; run `cad-cli use profile <name>`".to_owned(),
        ),
    };
    Ok(Report {
        value,
        messages: vec![message],
    })
}
