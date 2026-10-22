//! `cad-cli use profile <name> --forget`.

use serde::Serialize;

use super::{Report, api_keys_page, environment_decides};
use crate::config::Environment;
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{CredentialStore, Place};
use crate::support::profiles::{ProfileName, ProfileStore};
use crate::support::terminal::StatusKind;

/// The command's stdout value.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Forgotten {
    profile: ProfileName,
    /// The places a stored login was removed from.
    removed: Vec<Place>,
    was_active: bool,
}

/// Removes the profile's login from the keyring and the file, then the
/// profile. It never connects and asks nothing.
pub(super) fn run(
    name: ProfileName,
    environment: &impl Environment,
    store: &CredentialStore,
    profiles: &ProfileStore,
) -> Result<Report, CliError> {
    let mut set = profiles.load()?;
    let url = set
        .get(&name)
        .map(|profile| profile.url.clone())
        .ok_or_else(|| CliError::Input(format!("there is no profile {name} to forget")))?;

    // The login goes first: a failure here leaves the profile as it was.
    let removed = store.remove(&name)?;
    let was_active = set.remove(&name).is_some_and(|(_, was_active)| was_active);
    profiles.save(&set)?;

    let places = removed.places();
    let mut messages = Vec::new();
    if places.is_empty() {
        messages.push((
            StatusKind::Success,
            format!("forgot profile {name} ({url}); it had no stored login"),
        ));
    } else {
        messages.push((
            StatusKind::Success,
            format!(
                "forgot profile {name} ({url}): its login was removed from {}. The key itself \
                 stays valid until it is revoked at {}",
                places
                    .iter()
                    .map(Place::to_string)
                    .collect::<Vec<_>>()
                    .join(" and "),
                api_keys_page(&url)
            ),
        ));
    }
    if was_active {
        messages.push((
            StatusKind::Info,
            "no profile is active now; run `cad-cli use profile <name>`".to_owned(),
        ));
    }
    if let Some(reason) = &removed.keyring_failure {
        messages.push((
            StatusKind::Warning,
            format!(
                "the system keyring could not be reached, so an entry in it may remain: {reason}"
            ),
        ));
    }
    if environment_decides(environment) {
        messages.push((
            StatusKind::Warning,
            "CAD_API_KEY or CAD_API_SECRET is still set, and commands keep using it".to_owned(),
        ));
    }
    Ok(Report {
        value: to_value(Forgotten {
            profile: name,
            removed: places,
            was_active,
        })?,
        messages,
    })
}
