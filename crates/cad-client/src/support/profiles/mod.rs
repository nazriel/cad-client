//! The profiles `cad-cli` knows: `<config dir>/cad-cli/profiles.json`.
//!
//! ```json
//! {
//!   "version": 1,
//!   "active": "production",
//!   "profiles": {
//!     "production": {
//!       "url": "https://app.carbonaltdelete.eu",
//!       "allowInsecureHttp": false,
//!       "login": {"keyId": "…", "userId": "…", "email": "jane@example.com",
//!                 "storage": "keyring", "storedAt": "2026-10-21T12:00:00Z"},
//!       "company": {"id": "…", "name": "Acme GmbH"}
//!     }
//!   }
//! }
//! ```
//!
//! A profile is a name, a server and the facts about its login. The login's
//! secret is not here: it stays in the keyring or the credentials file, filed
//! under the profile's name (see [`super::credentials`]). Nothing in this file
//! is secret, so reading it does not check its mode; it is written the way
//! the credentials file is, in a private directory through a private
//! temporary file that is renamed into place.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::str::FromStr;

use cad_api::ids::{CompanyId, UserId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::config_dir::app_config_dir;
use super::credentials::Place;
use super::private_dir::create_private_dir;
use crate::config::Environment;
use crate::error::CliError;

/// The file inside the configuration directory.
const FILE_NAME: &str = "profiles.json";

/// Bumped whenever the layout changes; a newer file is refused, not guessed at.
const VERSION: u32 = 1;

/// The longest profile name.
const MAX_NAME_LENGTH: usize = 32;

/// What a profile name has to look like, as shown with a name that does not.
const NAME_RULE: &str = "a profile name is 1 to 32 characters of a-z, 0-9, - and _, starting \
                         with a letter or digit";

/// The name of a profile: lowercase letters, digits, `-` and `_`, starting
/// with a letter or digit, at most 32 characters. It is also the account the
/// profile's secret is filed under in the keyring, so it is never anything
/// else.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct ProfileName(String);

impl ProfileName {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for ProfileName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid_character = |character: char| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || matches!(character, '-' | '_')
        };
        let starts_well = value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_lowercase() || first.is_ascii_digit());
        if starts_well && value.len() <= MAX_NAME_LENGTH && value.chars().all(valid_character) {
            Ok(Self(value.to_owned()))
        } else {
            Err(format!(
                "`{value}` is not a valid profile name: {NAME_RULE}"
            ))
        }
    }
}

impl TryFrom<String> for ProfileName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ProfileName> for String {
    fn from(name: ProfileName) -> Self {
        name.0
    }
}

impl fmt::Display for ProfileName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What is known about a profile's login. The secret is not part of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Login {
    pub(crate) key_id: Uuid,
    pub(crate) user_id: UserId,
    /// The account's e-mail as it was when the login was made; `None` when it
    /// could not be read then.
    pub(crate) email: Option<String>,
    pub(crate) storage: Place,
    pub(crate) stored_at: DateTime<Utc>,
}

impl Login {
    /// Who the login is for, as a person would name them: the e-mail, or the
    /// user ID when the e-mail is not known.
    pub(crate) fn account(&self) -> String {
        self.email
            .clone()
            .unwrap_or_else(|| self.user_id.to_string())
    }
}

/// The company a profile acts on.
///
/// The name is a label recorded when the company was chosen, so that it can be
/// shown back to the person. It is never matched against anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Company {
    pub(crate) id: CompanyId,
    pub(crate) name: Option<String>,
}

impl Company {
    /// The company as a person would name it: the name, or the ID when the
    /// name is not known.
    pub(crate) fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.id.to_string())
    }
}

/// A named server, its login and its company.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Profile {
    /// The server's origin: `scheme://host[:port]`.
    pub(crate) url: String,
    /// Whether commands may use plain HTTP for this loopback server.
    #[serde(default)]
    pub(crate) allow_insecure_http: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) login: Option<Login>,
    /// The company commands act on for this profile, once one was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) company: Option<Company>,
}

/// Every profile, and which one is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileSet {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<ProfileName>,
    profiles: BTreeMap<ProfileName, Profile>,
}

impl Default for ProfileSet {
    fn default() -> Self {
        Self {
            version: VERSION,
            active: None,
            profiles: BTreeMap::new(),
        }
    }
}

impl ProfileSet {
    pub(crate) fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// The profiles, sorted by name.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&ProfileName, &Profile)> {
        self.profiles.iter()
    }

    pub(crate) fn get(&self, name: &ProfileName) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub(crate) fn get_mut(&mut self, name: &ProfileName) -> Option<&mut Profile> {
        self.profiles.get_mut(name)
    }

    /// The active profile. An `active` that names no profile, as after the
    /// file was edited by hand, is no active profile.
    pub(crate) fn active(&self) -> Option<&ProfileName> {
        self.active
            .as_ref()
            .filter(|name| self.profiles.contains_key(name))
    }

    /// The profiles whose server is `origin`, sorted by name.
    pub(crate) fn with_origin<'a>(
        &'a self,
        origin: &'a str,
    ) -> impl Iterator<Item = (&'a ProfileName, &'a Profile)> {
        self.iter()
            .filter(move |(_, profile)| profile.url == origin)
    }

    /// Adds or replaces a profile.
    pub(crate) fn insert(&mut self, name: ProfileName, profile: Profile) {
        self.profiles.insert(name, profile);
    }

    /// Makes `name` the active profile.
    pub(crate) fn activate(&mut self, name: ProfileName) {
        self.active = Some(name);
    }

    /// Removes a profile, clearing `active` if it named it. Returns the
    /// profile and whether it was the active one.
    pub(crate) fn remove(&mut self, name: &ProfileName) -> Option<(Profile, bool)> {
        let profile = self.profiles.remove(name)?;
        let was_active = self.active.as_ref() == Some(name);
        if was_active {
            self.active = None;
        }
        Some((profile, was_active))
    }
}

/// Where the profiles live: one file, or nowhere when the platform has no
/// per-user configuration directory.
#[derive(Debug, Clone)]
pub(crate) struct ProfileStore {
    path: Option<PathBuf>,
}

impl ProfileStore {
    pub(crate) const fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// The store for this process.
    pub(crate) fn from_environment(environment: &impl Environment) -> Self {
        Self::new(app_config_dir(environment, std::env::consts::OS).map(|dir| dir.join(FILE_NAME)))
    }

    /// The profiles; none when there is no file yet.
    ///
    /// # Errors
    ///
    /// Fails, naming the file, when it cannot be read, is not a profiles
    /// file, or is of a version this one does not read. It is never replaced
    /// by an empty set.
    pub(crate) fn load(&self) -> Result<ProfileSet, CliError> {
        let Some(path) = &self.path else {
            return Ok(ProfileSet::default());
        };
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(ProfileSet::default()),
            Err(error) => {
                return Err(CliError::config(format!(
                    "cannot read {}: {error}",
                    path.display()
                )));
            }
        };
        let set: ProfileSet = serde_json::from_str(&text).map_err(|error| {
            CliError::config(format!(
                "{} is not a profiles file ({error}); fix or remove it",
                path.display()
            ))
        })?;
        if set.version != VERSION {
            return Err(CliError::config(format!(
                "{} is version {}, which this version of cad-cli does not read; update cad-cli \
                 or remove the file",
                path.display(),
                set.version
            )));
        }
        Ok(set)
    }

    /// Writes `set`, replacing the file.
    ///
    /// # Errors
    ///
    /// Fails when there is no configuration directory to put the file in, or
    /// when the file cannot be written.
    pub(crate) fn save(&self, set: &ProfileSet) -> Result<(), CliError> {
        let path = self.path.as_deref().ok_or_else(|| {
            CliError::config(
                "this platform has no per-user configuration directory to keep profiles in",
            )
        })?;
        let Some(dir) = path.parent() else {
            return Err(CliError::config(format!(
                "{} has no directory to write to",
                path.display()
            )));
        };
        create_private_dir(dir)?;
        // The temporary file is created 0600 on Unix, so the content is never
        // visible to others, not even for the moment before a chmod.
        let mut temporary = tempfile::Builder::new()
            .prefix(".profiles-")
            .suffix(".tmp")
            .tempfile_in(dir)?;
        serde_json::to_writer_pretty(&mut temporary, set)
            .map_err(|error| CliError::Io(error.into()))?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(path)
            .map_err(|error| CliError::Io(error.error))?;
        Ok(())
    }
}
