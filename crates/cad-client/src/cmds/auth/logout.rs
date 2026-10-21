//! `cad-cli auth logout`.

use serde::Serialize;

use super::{Report, api_keys_page, environment_decides};
use crate::cli::GlobalArgs;
use crate::config::{Environment, resolve_timed_server};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{CredentialStore, Place, origin_of};
use crate::support::terminal::StatusKind;

/// The command's stdout value.
#[derive(Debug, Serialize)]
struct LogoutResult {
    origin: String,
    /// The places an entry was removed from.
    removed: Vec<Place>,
}

/// Removes the selected server's entry from the keyring and the file. It never
/// connects and asks nothing.
pub(super) fn run(
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
) -> Result<Report, CliError> {
    let server = resolve_timed_server(global, environment)?;
    let origin = origin_of(&server);
    let removed = store.remove(&origin)?;

    let places = removed.places();
    let mut messages = Vec::new();
    if places.is_empty() {
        messages.push((
            StatusKind::Warning,
            format!("no stored credentials for {origin}; nothing to remove"),
        ));
    } else {
        messages.push((
            StatusKind::Success,
            format!(
                "logged out of {origin}: removed from {}. The key itself stays valid until it \
                 is revoked at {}",
                places
                    .iter()
                    .map(Place::to_string)
                    .collect::<Vec<_>>()
                    .join(" and "),
                api_keys_page(&origin)
            ),
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
        value: to_value(&LogoutResult {
            origin,
            removed: places,
        })?,
        messages,
    })
}
