//! Where `cad-cli` keeps the credentials `cad-cli use profile` saved, and how
//! every other command finds them again.
//!
//! A login is stored per *profile*, under the profile's name. The entry
//! records the server origin (`scheme://host[:port]`) it was created for, and
//! it is used for that origin only, so a key saved for one server is never
//! sent to another, not even to another port on the same host. The value is a
//! small JSON document tagged by `kind`; today the only kind is `apiKey`, and
//! the tag leaves room for OAuth.
//!
//! There are two places to keep it. The system keyring is preferred; a file in
//! the user's configuration directory, readable only by its owner, is the
//! fallback for machines without a usable keyring. One profile has one stored
//! entry: saving it in one place removes it from the other.

mod file;
mod resolve;
mod system_keyring;

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use cad_api::{ServerConfig, ids::UserId};
use chrono::{DateTime, SubsecRound, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;
use strum::{Display, EnumDiscriminants};
use uuid::Uuid;

pub(crate) use self::file::FileBackend;
pub(crate) use self::resolve::{Resolved, resolve};
pub(crate) use self::system_keyring::KeyringBackend;
use super::profiles::ProfileName;
use crate::config::Environment;
use crate::error::CliError;
use crate::support::config_dir::app_config_dir;
use crate::support::switch::is_off_word;

/// Set to `0`, `false`, `no` or `off` to keep `cad-cli` away from the system
/// keyring altogether.
pub(crate) const KEYRING_VARIABLE: &str = "CAD_CLI_KEYRING";

/// The file the credentials fallback is written to, inside the configuration
/// directory.
const FILE_NAME: &str = "credentials.json";

/// The origin a server is identified by: `scheme://host[:port]`, lowercased,
/// without the default port, any path or a trailing slash.
pub(crate) fn origin_of(server: &ServerConfig) -> String {
    server.base_url().origin().ascii_serialization()
}

/// A login as it is stored: a JSON object tagged by `kind`.
///
/// [`Kind`], derived from it, names the tags this version knows, so an entry
/// of another kind is told apart from a damaged one.
#[derive(Debug, Clone, Serialize, Deserialize, EnumDiscriminants)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[strum_discriminants(name(Kind), derive(Deserialize), serde(rename_all = "camelCase"))]
pub(crate) enum StoredCredential {
    ApiKey {
        /// The server origin the key was created for, and the only one it is
        /// sent to.
        origin: String,
        key_id: Uuid,
        #[serde(serialize_with = "expose_secret")]
        secret: SecretString,
        user_id: UserId,
        stored_at: DateTime<Utc>,
    },
}

/// `secrecy` does not serialize a secret on purpose; an entry is the one place
/// that has to.
fn expose_secret<S: Serializer>(secret: &SecretString, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(secret.expose_secret())
}

impl StoredCredential {
    /// A new entry. `stored_at` is cut to whole seconds, which is how it is
    /// stored.
    pub(crate) fn api_key(
        origin: String,
        key_id: Uuid,
        secret: SecretString,
        user_id: UserId,
        stored_at: DateTime<Utc>,
    ) -> Self {
        Self::ApiKey {
            origin,
            key_id,
            secret,
            user_id,
            stored_at: stored_at.trunc_subsecs(0),
        }
    }

    pub(crate) const fn key_id(&self) -> Uuid {
        let Self::ApiKey { key_id, .. } = self;
        *key_id
    }

    pub(crate) const fn stored_at(&self) -> DateTime<Utc> {
        let Self::ApiKey { stored_at, .. } = self;
        *stored_at
    }

    pub(crate) fn origin(&self) -> &str {
        let Self::ApiKey { origin, .. } = self;
        origin
    }

    /// The entry as the JSON value both places store.
    pub(crate) fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("an entry is plain data and always serializes")
    }

    /// Reads an entry stored for `profile`.
    ///
    /// # Errors
    ///
    /// An entry of a `kind` this version does not know, such as one written by
    /// a newer `cad-cli`, is an error that names the kind and the profile; it
    /// is never skipped. So is an entry that is damaged.
    pub(crate) fn from_value(profile: &ProfileName, value: Value) -> Result<Self, CliError> {
        let kind = value.get("kind").and_then(Value::as_str).ok_or_else(|| {
            CliError::auth_config(format!(
                "the stored credentials for profile {profile} have no kind; log in again with \
                 `cad-cli use profile {profile} --login`"
            ))
        })?;
        if serde_json::from_value::<Kind>(Value::from(kind)).is_err() {
            return Err(CliError::auth_config(format!(
                "the stored credentials for profile {profile} are of kind \"{kind}\", which \
                 this version of cad-cli does not know; update cad-cli or log in again with \
                 `cad-cli use profile {profile} --login`"
            )));
        }
        serde_json::from_value(value).map_err(|error| {
            CliError::auth_config(format!(
                "the stored credentials for profile {profile} are damaged ({error}); log in \
                 again with `cad-cli use profile {profile} --login`"
            ))
        })
    }
}

/// Why a place could not be used.
#[derive(Debug)]
pub(crate) enum StoreError {
    /// The system keyring cannot be used here: no service answers, it is
    /// locked, or `CAD_CLI_KEYRING` turned it off. The text says why.
    Unavailable(String),
    /// Anything else: the entry or the file is not what it should be.
    Failed(CliError),
}

impl From<CliError> for StoreError {
    fn from(error: CliError) -> Self {
        Self::Failed(error)
    }
}

impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error.into())
    }
}

/// One place credentials can be kept, addressed by profile name. Values are
/// the JSON text of an entry and are held as secrets.
pub(crate) trait Backend: Send + Sync {
    fn get(&self, profile: &str) -> Result<Option<SecretString>, StoreError>;
    fn set(&self, profile: &str, value: &SecretString) -> Result<(), StoreError>;
    /// Removes the entry; `false` when there was none.
    fn remove(&self, profile: &str) -> Result<bool, StoreError>;
}

/// One of the two places a login is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub(crate) enum Place {
    Keyring,
    File,
}

/// Where a login is kept: the place, and the file's path for the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Location {
    Keyring,
    File(PathBuf),
}

impl Location {
    pub(crate) const fn place(&self) -> Place {
        match self {
            Self::Keyring => Place::Keyring,
            Self::File(_) => Place::File,
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::Keyring => None,
            Self::File(path) => Some(path),
        }
    }
}

/// What a lookup found, and why the keyring did not answer if it did not.
#[derive(Debug)]
pub(crate) struct Loaded {
    pub(crate) credential: Option<(StoredCredential, Location)>,
    pub(crate) keyring_failure: Option<String>,
}

/// What forgetting a profile removed.
#[derive(Debug, Default)]
pub(crate) struct Removed {
    pub(crate) keyring: bool,
    pub(crate) file: bool,
    pub(crate) keyring_failure: Option<String>,
}

impl Removed {
    /// The places an entry was removed from, keyring first.
    pub(crate) fn places(&self) -> Vec<Place> {
        let mut places = Vec::new();
        if self.keyring {
            places.push(Place::Keyring);
        }
        if self.file {
            places.push(Place::File);
        }
        places
    }
}

/// The keyring and the file behind one interface.
pub(crate) struct CredentialStore {
    /// `None` when `CAD_CLI_KEYRING` turned the keyring off.
    keyring: Option<Box<dyn Backend>>,
    /// `None` when the platform has no per-user configuration directory.
    file: Option<FileBackend>,
}

impl CredentialStore {
    pub(crate) fn new(keyring: Option<Box<dyn Backend>>, file: Option<FileBackend>) -> Self {
        Self { keyring, file }
    }

    /// The real store for this process, with nothing read yet.
    pub(crate) fn from_environment(environment: &impl Environment) -> Self {
        let keyring =
            keyring_enabled(environment).then(|| Box::new(KeyringBackend) as Box<dyn Backend>);
        let file = app_config_dir(environment, std::env::consts::OS)
            .map(|dir| FileBackend::new(dir.join(FILE_NAME)));
        Self::new(keyring, file)
    }

    /// Where the file fallback lives, if this platform has a place for it.
    pub(crate) fn file_path(&self) -> Option<&Path> {
        self.file.as_ref().map(FileBackend::path)
    }

    /// Looks `profile` up: the keyring first, then the file.
    ///
    /// A keyring that cannot answer is not an error here; the lookup goes on
    /// to the file and reports the reason.
    ///
    /// # Errors
    ///
    /// Fails when the file is unreadable, too open, or damaged, or when an
    /// entry is of an unknown kind.
    pub(crate) fn load(&self, profile: &ProfileName) -> Result<Loaded, CliError> {
        let mut keyring_failure = None;
        if let Some(keyring) = &self.keyring {
            match keyring.get(profile.as_str()) {
                Ok(Some(text)) => {
                    return Ok(Loaded {
                        credential: Some((parse_entry(profile, &text)?, Location::Keyring)),
                        keyring_failure: None,
                    });
                }
                Ok(None) => {}
                Err(StoreError::Unavailable(reason)) => keyring_failure = Some(reason),
                Err(StoreError::Failed(error)) => return Err(error),
            }
        }
        let credential = match &self.file {
            Some(file) => match file.get(profile.as_str()).map_err(into_cli_error)? {
                Some(text) => Some((
                    parse_entry(profile, &text)?,
                    Location::File(file.path().to_path_buf()),
                )),
                None => None,
            },
            None => None,
        };
        Ok(Loaded {
            credential,
            keyring_failure,
        })
    }

    /// Saves `credential` in the keyring and removes the profile's file entry.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] when the keyring cannot be used, in which
    /// case nothing has been stored or removed.
    pub(crate) fn save_in_keyring(
        &self,
        profile: &ProfileName,
        credential: &StoredCredential,
    ) -> Result<(), StoreError> {
        let keyring = self.keyring.as_ref().ok_or_else(|| {
            StoreError::Unavailable(format!("it is turned off by {KEYRING_VARIABLE}"))
        })?;
        keyring.set(profile.as_str(), &entry_text(credential))?;
        if let Some(file) = &self.file {
            file.remove(profile.as_str())?;
        }
        Ok(())
    }

    /// Saves `credential` in the credentials file and removes the profile's
    /// keyring entry, if there is one and the keyring answers.
    ///
    /// # Errors
    ///
    /// Fails when there is no configuration directory to put the file in, or
    /// the file cannot be read or written.
    pub(crate) fn save_in_file(
        &self,
        profile: &ProfileName,
        credential: &StoredCredential,
    ) -> Result<PathBuf, CliError> {
        let file = self.file.as_ref().ok_or_else(|| {
            CliError::auth_config(
                "this platform has no per-user configuration directory to keep credentials in",
            )
        })?;
        file.set(profile.as_str(), &entry_text(credential))
            .map_err(into_cli_error)?;
        if let Some(keyring) = &self.keyring {
            // Whatever the keyring says, the file now holds the login; an
            // entry that could not be removed stays behind the file's own.
            let _ = keyring.remove(profile.as_str());
        }
        Ok(file.path().to_path_buf())
    }

    /// Removes the profile's entry from both places.
    ///
    /// # Errors
    ///
    /// Fails when the credentials file cannot be read, is too open, or cannot
    /// be rewritten. A keyring that cannot answer is reported in the result.
    pub(crate) fn remove(&self, profile: &ProfileName) -> Result<Removed, CliError> {
        let mut removed = Removed::default();
        if let Some(keyring) = &self.keyring {
            match keyring.remove(profile.as_str()) {
                Ok(found) => removed.keyring = found,
                Err(StoreError::Unavailable(reason)) => removed.keyring_failure = Some(reason),
                Err(StoreError::Failed(error)) => return Err(error),
            }
        }
        if let Some(file) = &self.file {
            removed.file = file.remove(profile.as_str()).map_err(into_cli_error)?;
        }
        Ok(removed)
    }
}

fn entry_text(credential: &StoredCredential) -> SecretString {
    SecretString::from(credential.to_value().to_string())
}

fn parse_entry(profile: &ProfileName, text: &SecretString) -> Result<StoredCredential, CliError> {
    let value = serde_json::from_str(text.expose_secret()).map_err(|_| {
        CliError::auth_config(format!(
            "the stored credentials for profile {profile} are not valid JSON; log in again \
             with `cad-cli use profile {profile} --login`"
        ))
    })?;
    StoredCredential::from_value(profile, value)
}

/// The file backend only fails with [`StoreError::Failed`].
fn into_cli_error(error: StoreError) -> CliError {
    match error {
        StoreError::Failed(error) => error,
        StoreError::Unavailable(reason) => CliError::auth_config(reason),
    }
}

/// Whether the system keyring may be used. Only an explicit `0`, `false`,
/// `no` or `off` turns it off.
fn keyring_enabled(environment: &impl Environment) -> bool {
    !environment
        .get(KEYRING_VARIABLE)
        .is_some_and(|value| is_off_word(&value))
}
