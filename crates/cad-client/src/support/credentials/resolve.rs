//! Which credentials a command uses.
//!
//! 1. If `CAD_API_KEY` or `CAD_API_SECRET` is set, the environment decides
//!    alone, and a missing half is an error.
//! 2. Otherwise the stored login of the profile in use: the keyring entry
//!    first. A keyring that cannot answer is not fatal: the lookup goes on and
//!    remembers why.
//! 3. Then the credentials file's entry for the profile.
//! 4. Otherwise there are none, and the error says how to get some.
//!
//! A stored login goes only to the origin it was created for. An entry made
//! for another origin than the one selected is never used, whatever the
//! profile's name says.

use cad_api::ApiKeyV2Credentials;
use secrecy::SecretString;
use serde::Serialize;
use std::fmt::Write as _;
use uuid::Uuid;

use super::{CredentialStore, Location, StoredCredential};
use crate::config::Environment;
use crate::error::CliError;
use crate::support::profiles::ProfileName;

/// Where the credentials a command uses came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Source {
    /// `CAD_API_KEY` and `CAD_API_SECRET`.
    Environment,
    /// The login stored in the system keyring.
    Keyring,
    /// The login stored in the credentials file.
    File,
}

/// The credentials a command will use, found once.
///
/// The Key ID is known whenever any credentials are; the secret can still be
/// missing when only `CAD_API_KEY` is set. Shell completion keys its cache on
/// the Key ID alone and so can answer without a secret.
#[derive(Debug)]
pub(crate) struct Resolved {
    key_id: Uuid,
    secret: Option<SecretString>,
    source: Source,
}

impl Resolved {
    pub(crate) const fn source(&self) -> Source {
        self.source
    }

    pub(crate) const fn key_id(&self) -> Uuid {
        self.key_id
    }

    /// The credentials to authenticate with.
    ///
    /// # Errors
    ///
    /// Fails when `CAD_API_KEY` is set without a `CAD_API_SECRET`.
    pub(crate) fn into_credentials(self) -> Result<ApiKeyV2Credentials, CliError> {
        let secret = self
            .secret
            .ok_or_else(|| CliError::auth_config("CAD_API_SECRET is not set or empty"))?;
        Ok(ApiKeyV2Credentials::from_secret(self.key_id, secret))
    }
}

/// Whether the environment alone supplies the credentials: `CAD_API_KEY` or
/// `CAD_API_SECRET` is set, even when the other half is missing. A stored login
/// is then never consulted.
pub(crate) fn environment_decides(environment: &impl Environment) -> bool {
    environment.get("CAD_API_KEY").is_some() || environment.get("CAD_API_SECRET").is_some()
}

/// Finds the credentials for `origin`, the server selected, on behalf of
/// `profile`, the profile selected, if there is one.
///
/// # Errors
///
/// [`CliError::AuthConfig`] when there are none, when the environment holds
/// only half of a login, when the stored entry or file is unusable, or when
/// the stored entry was made for another origin.
pub(crate) fn resolve(
    environment: &impl Environment,
    store: &CredentialStore,
    profile: Option<&ProfileName>,
    origin: &str,
) -> Result<Resolved, CliError> {
    if environment_decides(environment) {
        return from_environment(environment);
    }
    let Some(profile) = profile else {
        return Err(CliError::auth_config_with_hint(
            format!("no credentials for {origin}"),
            format!(
                "set CAD_API_KEY and CAD_API_SECRET, or create a profile with `cad-cli use \
                 profile <name> --url {origin}`"
            ),
        ));
    };
    let loaded = store.load(profile)?;
    if let Some((credential, location)) = loaded.credential {
        if credential.origin() != origin {
            return Err(CliError::auth_config_with_hint(
                format!(
                    "the login stored for profile {profile} is for {}, not {origin}",
                    credential.origin()
                ),
                format!("log in again with `cad-cli use profile {profile} --login`"),
            ));
        }
        let key_id = credential.key_id();
        let StoredCredential::ApiKey { secret, .. } = credential;
        let source = match location {
            Location::Keyring => Source::Keyring,
            Location::File(_) => Source::File,
        };
        return Ok(Resolved {
            key_id,
            secret: Some(secret),
            source,
        });
    }
    let mut hint = format!(
        "run `cad-cli use profile {profile} --login`, or set CAD_API_KEY and CAD_API_SECRET"
    );
    if let Some(reason) = loaded.keyring_failure {
        let _ = write!(hint, " (the system keyring could not be read: {reason})");
    }
    Err(CliError::auth_config_with_hint(
        format!("profile {profile} has no login"),
        hint,
    ))
}

fn from_environment(environment: &impl Environment) -> Result<Resolved, CliError> {
    let key_id = environment
        .get("CAD_API_KEY")
        .ok_or_else(|| CliError::auth_config("CAD_API_KEY is not set"))?
        .parse::<Uuid>()
        .map_err(|error| CliError::auth_config(format!("CAD_API_KEY is not a UUID: {error}")))?;
    let secret = environment
        .get("CAD_API_SECRET")
        .filter(|secret| !secret.is_empty())
        .map(SecretString::from);
    Ok(Resolved {
        key_id,
        secret,
        source: Source::Environment,
    })
}
