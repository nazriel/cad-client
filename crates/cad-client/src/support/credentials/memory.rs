//! An in-memory stand-in for the system keyring, so no test touches the real
//! one.

use std::collections::HashMap;
use std::sync::Mutex;

use secrecy::{ExposeSecret, SecretString};

use super::{Backend, CredentialStore, StoreError};

/// A keyring that remembers entries in memory, or fails like one that cannot
/// be reached.
#[derive(Debug, Default)]
pub(crate) struct MemoryBackend {
    entries: Mutex<HashMap<String, String>>,
    failure: Option<String>,
}

impl MemoryBackend {
    /// A keyring every call to which fails with `reason`, as when no Secret
    /// Service is running.
    pub(crate) fn unavailable(reason: &str) -> Self {
        Self {
            entries: Mutex::default(),
            failure: Some(reason.to_owned()),
        }
    }

    pub(crate) fn put(&self, profile: &str, text: &str) {
        self.entries
            .lock()
            .expect("entries lock")
            .insert(profile.to_owned(), text.to_owned());
    }

    pub(crate) fn text(&self, profile: &str) -> Option<String> {
        self.entries
            .lock()
            .expect("entries lock")
            .get(profile)
            .cloned()
    }

    fn check(&self) -> Result<(), StoreError> {
        self.failure.as_ref().map_or(Ok(()), |reason| {
            Err(StoreError::Unavailable(reason.clone()))
        })
    }
}

impl Backend for MemoryBackend {
    fn get(&self, profile: &str) -> Result<Option<SecretString>, StoreError> {
        self.check()?;
        Ok(self.text(profile).map(SecretString::from))
    }

    fn set(&self, profile: &str, value: &SecretString) -> Result<(), StoreError> {
        self.check()?;
        self.put(profile, value.expose_secret());
        Ok(())
    }

    fn remove(&self, profile: &str) -> Result<bool, StoreError> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .expect("entries lock")
            .remove(profile)
            .is_some())
    }
}

/// Lets a test keep a handle on the keyring it hands to the store.
impl Backend for std::sync::Arc<MemoryBackend> {
    fn get(&self, profile: &str) -> Result<Option<SecretString>, StoreError> {
        (**self).get(profile)
    }

    fn set(&self, profile: &str, value: &SecretString) -> Result<(), StoreError> {
        (**self).set(profile, value)
    }

    fn remove(&self, profile: &str) -> Result<bool, StoreError> {
        (**self).remove(profile)
    }
}

/// A store with an empty in-memory keyring and no file: what a test gets when
/// the credentials in the environment are all that is being exercised.
pub(crate) fn empty_store() -> CredentialStore {
    CredentialStore::new(Some(Box::new(MemoryBackend::default())), None)
}
