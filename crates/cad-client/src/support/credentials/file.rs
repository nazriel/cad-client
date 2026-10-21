//! The credentials file: `<config dir>/cad-cli/credentials.json`.
//!
//! ```json
//! {"version":1,"credentials":{"production":{"kind":"apiKey", …}}}
//! ```
//!
//! On Unix the directory is created 0700 and the file is written 0600, by
//! writing a private temporary file beside it and renaming that into place.
//! A file that anyone but its owner can access is refused rather than used:
//! it may already have been read. Windows keeps it under the per-user profile.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Backend, StoreError};
use crate::error::CliError;
use crate::support::private_dir::create_private_dir;

/// Bumped whenever the layout changes; a newer file is refused, not guessed at.
const VERSION: u32 = 1;

/// The file's content. Entries stay raw JSON, so an entry of a kind this
/// version does not know survives a rewrite for another profile.
#[derive(Debug, Serialize, Deserialize)]
struct Document {
    version: u32,
    credentials: BTreeMap<String, Value>,
}

#[derive(Debug, Clone)]
pub(crate) struct FileBackend {
    path: PathBuf,
}

impl FileBackend {
    pub(crate) const fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The file's content, or `None` when there is no file.
    fn read(&self) -> Result<Option<Document>, CliError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(self.unreadable(&error)),
        };
        self.refuse_loose_mode(&file)?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| self.unreadable(&error))?;
        let document: Document = serde_json::from_str(&text).map_err(|error| {
            CliError::auth_config(format!(
                "{} is not a credentials file ({error}); fix or remove it",
                self.path.display()
            ))
        })?;
        if document.version != VERSION {
            return Err(CliError::auth_config(format!(
                "{} is version {}, which this version of cad-cli does not read; update \
                 cad-cli or remove the file",
                self.path.display(),
                document.version
            )));
        }
        Ok(Some(document))
    }

    fn unreadable(&self, error: &std::io::Error) -> CliError {
        CliError::auth_config(format!("cannot read {}: {error}", self.path.display()))
    }

    #[cfg(unix)]
    fn refuse_loose_mode(&self, file: &File) -> Result<(), CliError> {
        use std::os::unix::fs::PermissionsExt;

        let mode = file
            .metadata()
            .map_err(|error| self.unreadable(&error))?
            .permissions()
            .mode()
            & 0o777;
        // No group or other bit: the low six bits of the mode are clear.
        if mode.trailing_zeros() >= 6 {
            return Ok(());
        }
        Err(CliError::auth_config(format!(
            "{} holds credentials but can be read by other users (mode {mode:03o}); run \
             `chmod 600 {}`",
            self.path.display(),
            self.path.display()
        )))
    }

    #[cfg(not(unix))]
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    fn refuse_loose_mode(&self, _file: &File) -> Result<(), CliError> {
        Ok(())
    }

    fn write(&self, document: &Document) -> Result<(), CliError> {
        let Some(dir) = self.path.parent() else {
            return Err(CliError::auth_config(format!(
                "{} has no directory to write to",
                self.path.display()
            )));
        };
        create_private_dir(dir)?;
        // `NamedTempFile` creates its file 0600 on Unix, so the content is
        // never visible to others, not even for the moment before a chmod.
        let mut temporary = tempfile::Builder::new()
            .prefix(".credentials-")
            .suffix(".tmp")
            .tempfile_in(dir)?;
        serde_json::to_writer_pretty(&mut temporary, document)
            .map_err(|error| CliError::Io(error.into()))?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|error| CliError::Io(error.error))?;
        Ok(())
    }
}

impl Backend for FileBackend {
    fn get(&self, profile: &str) -> Result<Option<SecretString>, StoreError> {
        let Some(document) = self.read()? else {
            return Ok(None);
        };
        Ok(document
            .credentials
            .get(profile)
            .map(|entry| SecretString::from(entry.to_string())))
    }

    fn set(&self, profile: &str, value: &SecretString) -> Result<(), StoreError> {
        let entry: Value = serde_json::from_str(value.expose_secret())
            .map_err(|error| CliError::Io(error.into()))?;
        let mut document = self.read()?.unwrap_or_else(|| Document {
            version: VERSION,
            credentials: BTreeMap::new(),
        });
        document.credentials.insert(profile.to_owned(), entry);
        Ok(self.write(&document)?)
    }

    fn remove(&self, profile: &str) -> Result<bool, StoreError> {
        let Some(mut document) = self.read()? else {
            return Ok(false);
        };
        if document.credentials.remove(profile).is_none() {
            return Ok(false);
        }
        if document.credentials.is_empty() {
            std::fs::remove_file(&self.path)?;
        } else {
            self.write(&document)?;
        }
        Ok(true)
    }
}
