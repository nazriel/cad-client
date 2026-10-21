//! Where `cad-cli` keeps the credentials `auth login` saved, and how every
//! other command finds them again.
//!
//! A login is stored per server *origin* (`scheme://host[:port]`), so a key
//! saved for one server is never sent to another, not even to another port on
//! the same host. The value is a small JSON document tagged by `kind`; today
//! the only kind is `apiKey`, and the tag leaves room for OAuth.
//!
//! There are two places to keep it. The system keyring is preferred; a file in
//! the user's configuration directory, readable only by its owner, is the
//! fallback for machines without a usable keyring. One origin has one stored
//! entry: saving it in one place removes it from the other.

mod file;
mod resolve;
mod system_keyring;

#[cfg(test)]
pub(crate) mod memory;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use cad_api::ServerConfig;
use chrono::{DateTime, SecondsFormat, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(crate) use self::file::FileBackend;
pub(crate) use self::resolve::{Resolved, Source, resolve};
pub(crate) use self::system_keyring::KeyringBackend;
use crate::config::Environment;
use crate::error::CliError;
use crate::support::config_dir::app_config_dir;

/// Set to `0`, `false`, `no` or `off` to keep `cad-cli` away from the system
/// keyring altogether.
pub(crate) const KEYRING_VARIABLE: &str = "CAD_CLI_KEYRING";

/// The file the credentials fallback is written to, inside the configuration
/// directory.
const FILE_NAME: &str = "credentials.json";

/// The `kind` of an entry that holds a machine API key.
const API_KEY_KIND: &str = "apiKey";

/// The origin a server is identified by: `scheme://host[:port]`, lowercased,
/// without the default port, any path or a trailing slash.
pub(crate) fn origin_of(server: &ServerConfig) -> String {
    server.base_url().origin().ascii_serialization()
}

/// A login as it is stored.
#[derive(Debug, Clone)]
pub(crate) enum StoredCredential {
    ApiKey {
        key_id: Uuid,
        secret: SecretString,
        user_id: Uuid,
        stored_at: DateTime<Utc>,
    },
}

/// The fields of an `apiKey` entry, in the order they are stored.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiKeyEntry {
    key_id: Uuid,
    secret: SecretString,
    user_id: Uuid,
    stored_at: DateTime<Utc>,
}

impl StoredCredential {
    pub(crate) const fn api_key(
        key_id: Uuid,
        secret: SecretString,
        user_id: Uuid,
        stored_at: DateTime<Utc>,
    ) -> Self {
        Self::ApiKey {
            key_id,
            secret,
            user_id,
            stored_at,
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

    /// The entry as the JSON value both places store.
    pub(crate) fn to_value(&self) -> Value {
        let Self::ApiKey {
            key_id,
            secret,
            user_id,
            stored_at,
        } = self;
        json!({
            "kind": API_KEY_KIND,
            "keyId": key_id,
            "secret": secret.expose_secret(),
            "userId": user_id,
            "storedAt": stored_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        })
    }

    /// Reads an entry stored for `origin`.
    ///
    /// # Errors
    ///
    /// An entry of a `kind` this version does not know, such as one written by
    /// a newer `cad-cli`, is an error that names the kind and the origin; it
    /// is never skipped. So is an entry that is damaged.
    pub(crate) fn from_value(origin: &str, value: Value) -> Result<Self, CliError> {
        let kind = value.get("kind").and_then(Value::as_str).ok_or_else(|| {
            CliError::auth_config(format!(
                "the stored credentials for {origin} have no kind; log in again with \
                 `cad-cli auth login`"
            ))
        })?;
        match kind {
            API_KEY_KIND => {
                let entry: ApiKeyEntry = serde_json::from_value(value).map_err(|error| {
                    CliError::auth_config(format!(
                        "the stored credentials for {origin} are damaged ({error}); log in \
                         again with `cad-cli auth login`"
                    ))
                })?;
                Ok(Self::ApiKey {
                    key_id: entry.key_id,
                    secret: entry.secret,
                    user_id: entry.user_id,
                    stored_at: entry.stored_at,
                })
            }
            other => Err(CliError::auth_config(format!(
                "the stored credentials for {origin} are of kind \"{other}\", which this \
                 version of cad-cli does not know; update cad-cli or log in again with \
                 `cad-cli auth login`"
            ))),
        }
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

/// One place credentials can be kept, addressed by origin. Values are the
/// JSON text of an entry and are held as secrets.
pub(crate) trait Backend: Send + Sync {
    fn get(&self, origin: &str) -> Result<Option<SecretString>, StoreError>;
    fn set(&self, origin: &str, value: &SecretString) -> Result<(), StoreError>;
    /// Removes the entry; `false` when there was none.
    fn remove(&self, origin: &str) -> Result<bool, StoreError>;
}

/// Where a stored login was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Location {
    Keyring,
    File(PathBuf),
}

/// What a lookup found, and why the keyring did not answer if it did not.
#[derive(Debug)]
pub(crate) struct Loaded {
    pub(crate) credential: Option<(StoredCredential, Location)>,
    pub(crate) keyring_failure: Option<String>,
}

/// What `logout` removed.
#[derive(Debug, Default)]
pub(crate) struct Removed {
    pub(crate) keyring: bool,
    pub(crate) file: bool,
    pub(crate) keyring_failure: Option<String>,
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

    /// Looks `origin` up: the keyring first, then the file.
    ///
    /// A keyring that cannot answer is not an error here; the lookup goes on
    /// to the file and reports the reason.
    ///
    /// # Errors
    ///
    /// Fails when the file is unreadable, too open, or damaged, or when an
    /// entry is of an unknown kind.
    pub(crate) fn load(&self, origin: &str) -> Result<Loaded, CliError> {
        let mut keyring_failure = None;
        if let Some(keyring) = &self.keyring {
            match keyring.get(origin) {
                Ok(Some(text)) => {
                    return Ok(Loaded {
                        credential: Some((parse_entry(origin, &text)?, Location::Keyring)),
                        keyring_failure: None,
                    });
                }
                Ok(None) => {}
                Err(StoreError::Unavailable(reason)) => keyring_failure = Some(reason),
                Err(StoreError::Failed(error)) => return Err(error),
            }
        }
        let credential = match &self.file {
            Some(file) => match file.get(origin).map_err(into_cli_error)? {
                Some(text) => Some((
                    parse_entry(origin, &text)?,
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

    /// Saves `credential` in the keyring and removes the origin's file entry.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] when the keyring cannot be used, in which
    /// case nothing has been stored or removed.
    pub(crate) fn save_in_keyring(
        &self,
        origin: &str,
        credential: &StoredCredential,
    ) -> Result<(), StoreError> {
        let keyring = self.keyring.as_ref().ok_or_else(|| {
            StoreError::Unavailable(format!("it is turned off by {KEYRING_VARIABLE}"))
        })?;
        keyring.set(origin, &entry_text(credential))?;
        if let Some(file) = &self.file {
            file.remove(origin)?;
        }
        Ok(())
    }

    /// Saves `credential` in the credentials file and removes the origin's
    /// keyring entry, if there is one and the keyring answers.
    ///
    /// # Errors
    ///
    /// Fails when there is no configuration directory to put the file in, or
    /// the file cannot be read or written.
    pub(crate) fn save_in_file(
        &self,
        origin: &str,
        credential: &StoredCredential,
    ) -> Result<PathBuf, CliError> {
        let file = self.file.as_ref().ok_or_else(|| {
            CliError::auth_config(
                "this platform has no per-user configuration directory to keep credentials in",
            )
        })?;
        file.set(origin, &entry_text(credential))
            .map_err(into_cli_error)?;
        if let Some(keyring) = &self.keyring {
            // Whatever the keyring says, the file now holds the login; an
            // entry that could not be removed stays behind the file's own.
            let _ = keyring.remove(origin);
        }
        Ok(file.path().to_path_buf())
    }

    /// Removes the origin's entry from both places.
    ///
    /// # Errors
    ///
    /// Fails when the credentials file cannot be read, is too open, or cannot
    /// be rewritten. A keyring that cannot answer is reported in the result.
    pub(crate) fn remove(&self, origin: &str) -> Result<Removed, CliError> {
        let mut removed = Removed::default();
        if let Some(keyring) = &self.keyring {
            match keyring.remove(origin) {
                Ok(found) => removed.keyring = found,
                Err(StoreError::Unavailable(reason)) => removed.keyring_failure = Some(reason),
                Err(StoreError::Failed(error)) => return Err(error),
            }
        }
        if let Some(file) = &self.file {
            removed.file = file.remove(origin).map_err(into_cli_error)?;
        }
        Ok(removed)
    }
}

fn entry_text(credential: &StoredCredential) -> SecretString {
    SecretString::from(credential.to_value().to_string())
}

fn parse_entry(origin: &str, text: &SecretString) -> Result<StoredCredential, CliError> {
    let value = serde_json::from_str(text.expose_secret()).map_err(|_| {
        CliError::auth_config(format!(
            "the stored credentials for {origin} are not valid JSON; log in again with \
             `cad-cli auth login`"
        ))
    })?;
    StoredCredential::from_value(origin, value)
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
    !environment.get(KEYRING_VARIABLE).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        )
    })
}
