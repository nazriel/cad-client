//! Which credentials a command uses.
//!
//! 1. If `CAD_API_KEY` or `CAD_API_SECRET` is set, the environment decides
//!    alone, and a missing half is an error.
//! 2. Otherwise the keyring entry for the server's origin. A keyring that
//!    cannot answer is not fatal: the lookup goes on and remembers why.
//! 3. Then the credentials file's entry for the origin.
//! 4. Otherwise there are none, and the error says how to get some.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use cad_api::ApiKeyV2Credentials;
use chrono::{DateTime, Utc};
use secrecy::SecretString;
use uuid::Uuid;

use super::{CredentialStore, Location, StoredCredential};
use crate::config::Environment;
use crate::error::CliError;

/// Where the credentials in use came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Source {
    Environment,
    Keyring,
    File(PathBuf),
}

impl Source {
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Keyring => "keyring",
            Self::File(_) => "file",
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Environment | Self::Keyring => None,
        }
    }
}

impl From<Location> for Source {
    fn from(location: Location) -> Self {
        match location {
            Location::Keyring => Self::Keyring,
            Location::File(path) => Self::File(path),
        }
    }
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
    stored_at: Option<DateTime<Utc>>,
}

impl Resolved {
    pub(crate) const fn key_id(&self) -> Uuid {
        self.key_id
    }

    pub(crate) const fn source(&self) -> &Source {
        &self.source
    }

    pub(crate) const fn stored_at(&self) -> Option<DateTime<Utc>> {
        self.stored_at
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

/// Finds the credentials for `origin`.
///
/// # Errors
///
/// [`CliError::AuthConfig`] when there are none, when the environment holds
/// only half of a login, or when the stored entry or file is unusable.
pub(crate) fn resolve(
    environment: &impl Environment,
    store: &CredentialStore,
    origin: &str,
) -> Result<Resolved, CliError> {
    if environment.get("CAD_API_KEY").is_some() || environment.get("CAD_API_SECRET").is_some() {
        return from_environment(environment);
    }
    let loaded = store.load(origin)?;
    if let Some((credential, location)) = loaded.credential {
        let stored_at = credential.stored_at();
        let key_id = credential.key_id();
        let StoredCredential::ApiKey { secret, .. } = credential;
        return Ok(Resolved {
            key_id,
            secret: Some(secret),
            source: location.into(),
            stored_at: Some(stored_at),
        });
    }
    let mut message = format!(
        "no credentials for {origin}; run `cad-cli auth login` or set CAD_API_KEY and \
         CAD_API_SECRET"
    );
    if let Some(reason) = loaded.keyring_failure {
        let _ = write!(message, " (the system keyring could not be read: {reason})");
    }
    Err(CliError::auth_config(message))
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
        stored_at: None,
    })
}
