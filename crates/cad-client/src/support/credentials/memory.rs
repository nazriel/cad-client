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

    pub(crate) fn put(&self, origin: &str, text: &str) {
        self.entries
            .lock()
            .expect("entries lock")
            .insert(origin.to_owned(), text.to_owned());
    }

    pub(crate) fn text(&self, origin: &str) -> Option<String> {
        self.entries
            .lock()
            .expect("entries lock")
            .get(origin)
            .cloned()
    }

    fn check(&self) -> Result<(), StoreError> {
        self.failure.as_ref().map_or(Ok(()), |reason| {
            Err(StoreError::Unavailable(reason.clone()))
        })
    }
}

impl Backend for MemoryBackend {
    fn get(&self, origin: &str) -> Result<Option<SecretString>, StoreError> {
        self.check()?;
        Ok(self.text(origin).map(SecretString::from))
    }

    fn set(&self, origin: &str, value: &SecretString) -> Result<(), StoreError> {
        self.check()?;
        self.put(origin, value.expose_secret());
        Ok(())
    }

    fn remove(&self, origin: &str) -> Result<bool, StoreError> {
        self.check()?;
        Ok(self
            .entries
            .lock()
            .expect("entries lock")
            .remove(origin)
            .is_some())
    }
}

/// Lets a test keep a handle on the keyring it hands to the store.
impl Backend for std::sync::Arc<MemoryBackend> {
    fn get(&self, origin: &str) -> Result<Option<SecretString>, StoreError> {
        (**self).get(origin)
    }

    fn set(&self, origin: &str, value: &SecretString) -> Result<(), StoreError> {
        (**self).set(origin, value)
    }

    fn remove(&self, origin: &str) -> Result<bool, StoreError> {
        (**self).remove(origin)
    }
}

/// A store with an empty in-memory keyring and no file: what a test gets when
/// the credentials in the environment are all that is being exercised.
pub(crate) fn empty_store() -> CredentialStore {
    CredentialStore::new(Some(Box::new(MemoryBackend::default())), None)
}
