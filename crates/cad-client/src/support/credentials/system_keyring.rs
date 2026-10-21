//! The system keyring: macOS Keychain, Windows Credential Manager, or the
//! Secret Service (through libdbus) on other Unix systems.

use std::sync::LazyLock;

use keyring_core::Entry;
use secrecy::{ExposeSecret, SecretString};

use super::{Backend, StoreError};

/// The service name entries are filed under.
const SERVICE: &str = "cad-cli";

/// The platform's credential store, set up the first time it is needed and
/// made the default `keyring_core` entries are created in. The error says why
/// the platform has none, or why it failed to start.
static STORE: LazyLock<Result<(), String>> = LazyLock::new(|| {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(target_os = "windows")]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(all(
        unix,
        not(any(target_os = "macos", target_os = "ios", target_os = "android"))
    ))]
    let store = dbus_secret_service_keyring_store::Store::new();
    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        )
    ))]
    {
        store
            .map(|store| keyring_core::set_default_store(store))
            .map_err(|error| error.to_string())
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        all(
            unix,
            not(any(target_os = "macos", target_os = "ios", target_os = "android"))
        )
    )))]
    {
        Err("no system keyring on this platform".to_owned())
    }
});

/// What a keyring call can come back with besides success.
enum Failure {
    NoEntry,
    Unavailable(String),
}

impl From<Failure> for StoreError {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::NoEntry => Self::Unavailable("there is no such entry".to_owned()),
            Failure::Unavailable(reason) => Self::Unavailable(reason),
        }
    }
}

/// Every call runs on a thread of its own, the first one's setting up of the
/// store included.
///
/// The Secret Service client blocks while it waits for the answer, which must
/// never happen on a thread of the application's runtime. A thread started
/// here, and joined at once, is outside it however the caller is reached.
#[derive(Debug, Default)]
pub(crate) struct KeyringBackend;

impl KeyringBackend {
    fn on_own_thread<T: Send>(
        origin: &str,
        operation: impl FnOnce(&Entry) -> keyring_core::Result<T> + Send,
    ) -> Result<T, Failure> {
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    if let Err(reason) = &*STORE {
                        return Err(Failure::Unavailable(reason.clone()));
                    }
                    let entry = Entry::new(SERVICE, origin)
                        .map_err(|error| Failure::Unavailable(error.to_string()))?;
                    operation(&entry).map_err(|error| match error {
                        keyring_core::Error::NoEntry => Failure::NoEntry,
                        other => Failure::Unavailable(other.to_string()),
                    })
                })
                .join()
                .unwrap_or_else(|_| {
                    Err(Failure::Unavailable(
                        "the keyring client panicked".to_owned(),
                    ))
                })
        })
    }
}

impl Backend for KeyringBackend {
    fn get(&self, origin: &str) -> Result<Option<SecretString>, StoreError> {
        match Self::on_own_thread(origin, Entry::get_password) {
            Ok(text) => Ok(Some(SecretString::from(text))),
            Err(Failure::NoEntry) => Ok(None),
            Err(failure) => Err(failure.into()),
        }
    }

    fn set(&self, origin: &str, value: &SecretString) -> Result<(), StoreError> {
        Ok(Self::on_own_thread(origin, |entry| {
            entry.set_password(value.expose_secret())
        })?)
    }

    fn remove(&self, origin: &str) -> Result<bool, StoreError> {
        match Self::on_own_thread(origin, Entry::delete_credential) {
            Ok(()) => Ok(true),
            Err(Failure::NoEntry) => Ok(false),
            Err(failure) => Err(failure.into()),
        }
    }
}
