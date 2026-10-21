//! The stored entry, the two places it lives in, and the order commands look
//! for credentials in. The keyring is always the in-memory double; the file is
//! a real file in a temporary directory.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cad_api::ServerConfig;
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde_json::{Value, json};
use uuid::Uuid;

use super::memory::MemoryBackend;
use super::{
    CredentialStore, FileBackend, Location, Source, SourceKind, StoreError, StoredCredential,
    keyring_enabled, origin_of, resolve,
};
use crate::config::Environment;
use crate::error::CliError;

const ORIGIN: &str = "https://staging.carbonaltdelete.eu";
const OTHER_ORIGIN: &str = "https://app.carbonaltdelete.eu";
const KEY_ID: Uuid = Uuid::from_u128(0xaa);
const OTHER_KEY_ID: Uuid = Uuid::from_u128(0xab);
const USER_ID: Uuid = Uuid::from_u128(0xbb);
const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";
const OTHER_SECRET: &str = "cad_aks_ZyXwVuTsRqPoNmLkJiHgFeDcBa9876543210-_ZyXwV";

#[derive(Default)]
struct Env(HashMap<String, String>);

impl Env {
    fn with(mut self, key: &str, value: &str) -> Self {
        self.0.insert(key.to_owned(), value.to_owned());
        self
    }
}

impl Environment for Env {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

fn stored_at() -> DateTime<Utc> {
    "2026-10-21T12:00:00Z".parse().unwrap()
}

fn credential(key_id: Uuid, secret: &str) -> StoredCredential {
    StoredCredential::api_key(key_id, SecretString::from(secret), USER_ID, stored_at())
}

/// A keyring double, the file in `dir`, and a store made of both.
struct Fixture {
    keyring: Arc<MemoryBackend>,
    store: CredentialStore,
    path: std::path::PathBuf,
}

fn fixture(dir: &Path) -> Fixture {
    with_keyring(dir, MemoryBackend::default())
}

fn with_keyring(dir: &Path, keyring: MemoryBackend) -> Fixture {
    let keyring = Arc::new(keyring);
    let path = dir.join("cad-cli").join("credentials.json");
    let store = CredentialStore::new(
        Some(Box::new(Arc::clone(&keyring))),
        Some(FileBackend::new(path.clone())),
    );
    Fixture {
        keyring,
        store,
        path,
    }
}

fn found(loaded: super::Loaded) -> (StoredCredential, Location) {
    loaded.credential.expect("an entry was found")
}

fn key_of(credential: &StoredCredential) -> Uuid {
    credential.key_id()
}

// --- the entry ---------------------------------------------------------------

#[test]
fn an_entry_is_stored_as_tagged_json_and_read_back() {
    let value = credential(KEY_ID, SECRET).to_value();
    assert_eq!(
        value,
        json!({
            "kind": "apiKey",
            "keyId": KEY_ID,
            "secret": SECRET,
            "userId": USER_ID,
            "storedAt": "2026-10-21T12:00:00Z",
        })
    );
    let read = StoredCredential::from_value(ORIGIN, value).unwrap();
    assert_eq!(key_of(&read), KEY_ID);
    assert_eq!(read.stored_at(), stored_at());
}

#[test]
fn an_entry_of_an_unknown_kind_names_the_kind_and_the_origin() {
    let error = StoredCredential::from_value(
        ORIGIN,
        json!({"kind": "oauth", "accessToken": "never-shown", "refreshToken": "never-shown"}),
    )
    .unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)));
    let message = error.to_string();
    assert!(message.contains("oauth"), "{message}");
    assert!(message.contains(ORIGIN), "{message}");
    assert!(!message.contains("never-shown"), "{message}");
}

#[test]
fn an_entry_without_a_kind_or_with_missing_fields_is_an_error_not_a_skip() {
    for value in [
        json!({"keyId": KEY_ID}),
        json!({"kind": "apiKey", "keyId": KEY_ID}),
        json!({"kind": "apiKey", "keyId": "nope", "secret": SECRET, "userId": USER_ID,
               "storedAt": "2026-10-21T12:00:00Z"}),
        json!("a string"),
    ] {
        let error = StoredCredential::from_value(ORIGIN, value.clone()).unwrap_err();
        assert!(matches!(error, CliError::AuthConfig(_)), "{value}");
        assert!(error.to_string().contains(ORIGIN), "{value}");
    }
}

#[test]
fn no_debug_output_shows_the_secret() {
    let credential = credential(KEY_ID, SECRET);
    assert!(!format!("{credential:?}").contains(SECRET));
    let environment = Env::default()
        .with("CAD_API_KEY", &KEY_ID.to_string())
        .with("CAD_API_SECRET", SECRET);
    let resolved = resolve(&environment, &super::memory::empty_store(), ORIGIN).unwrap();
    assert!(!format!("{resolved:?}").contains(SECRET));
}

// --- the origin --------------------------------------------------------------

fn origin(url: &str) -> String {
    origin_of(&ServerConfig::new(url).unwrap())
}

#[test]
fn the_same_origin_written_differently_is_one_key() {
    let canonical = "https://staging.carbonaltdelete.eu";
    for url in [
        "https://staging.carbonaltdelete.eu",
        "https://staging.carbonaltdelete.eu/",
        "HTTPS://Staging.CarbonAltDelete.EU:443/",
        "https://staging.carbonaltdelete.eu/some/path",
    ] {
        assert_eq!(origin(url), canonical, "{url}");
    }
}

#[test]
fn another_host_or_another_port_is_another_origin() {
    assert_ne!(
        origin("https://staging.carbonaltdelete.eu"),
        origin(OTHER_ORIGIN)
    );
    assert_ne!(
        origin("https://staging.carbonaltdelete.eu"),
        origin("https://staging.carbonaltdelete.eu:8443")
    );
    assert_eq!(
        origin("https://staging.carbonaltdelete.eu:8443"),
        "https://staging.carbonaltdelete.eu:8443"
    );
    let local = |port: u16| {
        origin_of(&ServerConfig::insecure_loopback(format!("http://localhost:{port}")).unwrap())
    };
    assert_eq!(local(5001), "http://localhost:5001");
    assert_ne!(local(5001), local(5002));
}

#[test]
fn only_an_explicit_off_value_turns_the_keyring_off() {
    for value in ["0", "false", "no", "off", "OFF", " False "] {
        assert!(
            !keyring_enabled(&Env::default().with("CAD_CLI_KEYRING", value)),
            "{value}"
        );
    }
    for value in ["1", "true", "yes", "on", "", "anything"] {
        assert!(
            keyring_enabled(&Env::default().with("CAD_CLI_KEYRING", value)),
            "{value}"
        );
    }
    assert!(keyring_enabled(&Env::default()));
}

// --- the places --------------------------------------------------------------

#[test]
fn a_keyring_entry_wins_over_a_file_entry_and_each_origin_has_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(OTHER_KEY_ID, OTHER_SECRET))
        .unwrap();
    // `save_in_keyring` would remove the file entry, so plant the keyring's.
    fixture
        .keyring
        .put(ORIGIN, &credential(KEY_ID, SECRET).to_value().to_string());

    let (entry, location) = found(fixture.store.load(ORIGIN).unwrap());
    assert_eq!(key_of(&entry), KEY_ID);
    assert_eq!(location, Location::Keyring);

    assert!(
        fixture
            .store
            .load(OTHER_ORIGIN)
            .unwrap()
            .credential
            .is_none()
    );
    assert!(
        fixture
            .store
            .load("https://staging.carbonaltdelete.eu:8443")
            .unwrap()
            .credential
            .is_none()
    );
}

#[test]
fn an_unavailable_keyring_falls_through_to_the_file_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = with_keyring(dir.path(), MemoryBackend::unavailable("no secret service"));
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();

    let loaded = fixture.store.load(ORIGIN).unwrap();
    assert_eq!(loaded.keyring_failure.as_deref(), Some("no secret service"));
    let (entry, location) = found(loaded);
    assert_eq!(key_of(&entry), KEY_ID);
    assert_eq!(location, Location::File(fixture.path.clone()));

    let loaded = fixture.store.load(OTHER_ORIGIN).unwrap();
    assert!(loaded.credential.is_none());
    assert_eq!(loaded.keyring_failure.as_deref(), Some("no secret service"));
}

#[test]
fn one_origin_has_one_stored_entry() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());

    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    fixture
        .store
        .save_in_keyring(ORIGIN, &credential(OTHER_KEY_ID, OTHER_SECRET))
        .unwrap();
    // The file held nothing else, so it is gone with its only entry.
    assert!(!fixture.path.exists());
    assert!(fixture.keyring.text(ORIGIN).is_some());

    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    assert!(fixture.keyring.text(ORIGIN).is_none());
    assert!(fixture.path.exists());
}

#[test]
fn an_unreachable_keyring_does_not_fail_a_file_write() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = with_keyring(dir.path(), MemoryBackend::unavailable("locked"));
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .expect("an unreachable keyring does not fail the file write");
}

#[test]
fn an_unusable_keyring_stores_and_removes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = with_keyring(dir.path(), MemoryBackend::unavailable("locked"));
    fixture
        .store
        .save_in_file(OTHER_ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    let before = std::fs::read_to_string(&fixture.path).unwrap();

    let error = fixture
        .store
        .save_in_keyring(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap_err();
    assert!(matches!(&error, StoreError::Unavailable(reason) if reason == "locked"));
    assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), before);

    // A keyring turned off by CAD_CLI_KEYRING is unavailable the same way.
    let store = CredentialStore::new(None, Some(FileBackend::new(fixture.path.clone())));
    let error = store
        .save_in_keyring(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap_err();
    assert!(
        matches!(&error, StoreError::Unavailable(reason) if reason.contains("CAD_CLI_KEYRING"))
    );
    assert!(store.load(ORIGIN).unwrap().keyring_failure.is_none());
}

#[test]
fn without_a_configuration_directory_the_file_cannot_be_written() {
    let store = CredentialStore::new(None, None);
    let error = store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)));
    assert!(store.file_path().is_none());
    assert!(store.load(ORIGIN).unwrap().credential.is_none());
}

#[test]
fn removing_reports_where_an_entry_was_and_keeps_other_origins() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    fixture
        .store
        .save_in_file(OTHER_ORIGIN, &credential(OTHER_KEY_ID, OTHER_SECRET))
        .unwrap();
    fixture
        .keyring
        .put(ORIGIN, &credential(KEY_ID, SECRET).to_value().to_string());

    let removed = fixture.store.remove(ORIGIN).unwrap();
    assert!(removed.keyring && removed.file);
    assert!(fixture.keyring.text(ORIGIN).is_none());
    assert!(
        fixture.path.exists(),
        "another origin's entry keeps the file"
    );
    assert!(
        fixture
            .store
            .load(OTHER_ORIGIN)
            .unwrap()
            .credential
            .is_some()
    );

    let removed = fixture.store.remove(ORIGIN).unwrap();
    assert!(!removed.keyring && !removed.file);

    let removed = fixture.store.remove(OTHER_ORIGIN).unwrap();
    assert!(removed.file);
    assert!(!fixture.path.exists(), "the file goes with its last entry");
}

#[test]
fn removing_reports_a_keyring_that_cannot_be_reached() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = with_keyring(dir.path(), MemoryBackend::unavailable("locked"));
    let removed = fixture.store.remove(ORIGIN).unwrap();
    assert!(!removed.keyring && !removed.file);
    assert_eq!(removed.keyring_failure.as_deref(), Some("locked"));
}

// --- the file ----------------------------------------------------------------

#[test]
fn the_file_has_the_documented_shape() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    let written = fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    assert_eq!(written, fixture.path);

    let document: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.path).unwrap()).unwrap();
    assert_eq!(
        document,
        json!({
            "version": 1,
            "credentials": {ORIGIN: credential(KEY_ID, SECRET).to_value()},
        })
    );
    // Nothing else is left beside it: the temporary file was renamed away.
    let names: Vec<_> = std::fs::read_dir(fixture.path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["credentials.json"]);
}

#[cfg(unix)]
#[test]
fn the_file_is_written_0600_in_a_0700_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&fixture.path), 0o600);
    assert_eq!(mode(fixture.path.parent().unwrap()), 0o700);

    // A second write replaces the file and keeps both modes.
    fixture
        .store
        .save_in_file(OTHER_ORIGIN, &credential(OTHER_KEY_ID, OTHER_SECRET))
        .unwrap();
    assert_eq!(mode(&fixture.path), 0o600);
}

#[cfg(unix)]
#[test]
fn a_file_with_any_group_or_other_bit_is_refused_with_the_fix() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();

    for mode in [0o644, 0o640, 0o604, 0o660, 0o666, 0o601] {
        std::fs::set_permissions(&fixture.path, std::fs::Permissions::from_mode(mode)).unwrap();
        let path = fixture.path.display().to_string();
        let expected = format!("{mode:03o}");
        let loaded = fixture.store.load(ORIGIN).unwrap_err();
        assert!(matches!(loaded, CliError::AuthConfig(_)), "{mode:o}");
        let message = loaded.to_string();
        assert!(message.contains(&path), "{message}");
        assert!(message.contains(&expected), "{message}");
        assert!(message.contains(&format!("chmod 600 {path}")), "{message}");
        assert!(!message.contains(SECRET), "{message}");

        // Writing and removing refuse too, and leave the file as it was.
        assert!(
            fixture
                .store
                .save_in_file(OTHER_ORIGIN, &credential(KEY_ID, SECRET))
                .is_err()
        );
        assert!(fixture.store.remove(ORIGIN).is_err());
    }

    std::fs::set_permissions(&fixture.path, std::fs::Permissions::from_mode(0o400)).unwrap();
    assert!(fixture.store.load(ORIGIN).unwrap().credential.is_some());
}

#[test]
fn a_file_that_is_not_ours_or_is_newer_is_an_error_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();

    for content in ["not json", "[]", r#"{"version":2,"credentials":{}}"#] {
        std::fs::write(&fixture.path, content).unwrap();
        let error = fixture.store.load(ORIGIN).unwrap_err();
        assert!(matches!(error, CliError::AuthConfig(_)), "{content}");
        assert!(
            error
                .to_string()
                .contains(&fixture.path.display().to_string())
        );
        assert!(
            fixture
                .store
                .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&fixture.path).unwrap(), content);
    }
}

#[test]
fn an_entry_of_an_unknown_kind_is_an_error_and_survives_other_writes() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    std::fs::create_dir_all(fixture.path.parent().unwrap()).unwrap();
    let newer = json!({"kind": "oauth", "accessToken": "never-shown"});
    let document = json!({"version": 1, "credentials": {ORIGIN: newer}});
    std::fs::write(&fixture.path, document.to_string()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fixture.path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    let error = fixture.store.load(ORIGIN).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("oauth") && message.contains(ORIGIN),
        "{message}"
    );
    assert!(!message.contains("never-shown"));

    // Another origin can still be logged in, and the entry we cannot read stays.
    fixture
        .store
        .save_in_file(OTHER_ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    let after: Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture.path).unwrap()).unwrap();
    assert_eq!(after["credentials"][ORIGIN], newer);
    assert!(
        fixture
            .store
            .load(OTHER_ORIGIN)
            .unwrap()
            .credential
            .is_some()
    );
}

// --- resolution --------------------------------------------------------------

fn resolved_key(environment: &Env, store: &CredentialStore) -> Result<(Uuid, Source), CliError> {
    resolve(environment, store, ORIGIN)
        .map(|resolved| (resolved.key_id(), resolved.source().clone()))
}

#[test]
fn the_environment_beats_the_keyring_which_beats_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    let environment = Env::default();
    fixture
        .store
        .save_in_file(ORIGIN, &credential(Uuid::from_u128(1), SECRET))
        .unwrap();

    let (key, source) = resolved_key(&environment, &fixture.store).unwrap();
    assert_eq!(key, Uuid::from_u128(1));
    assert_eq!(source, Source::Stored(Location::File(fixture.path.clone())));

    fixture.keyring.put(
        ORIGIN,
        &credential(Uuid::from_u128(2), SECRET)
            .to_value()
            .to_string(),
    );
    let (key, source) = resolved_key(&environment, &fixture.store).unwrap();
    assert_eq!(key, Uuid::from_u128(2));
    assert_eq!(source, Source::Stored(Location::Keyring));

    let environment = Env::default()
        .with("CAD_API_KEY", &Uuid::from_u128(3).to_string())
        .with("CAD_API_SECRET", "from-the-environment");
    let (key, source) = resolved_key(&environment, &fixture.store).unwrap();
    assert_eq!(key, Uuid::from_u128(3));
    assert_eq!(source, Source::Environment);
}

#[test]
fn half_an_environment_login_decides_alone_and_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    fixture
        .keyring
        .put(ORIGIN, &credential(KEY_ID, SECRET).to_value().to_string());

    let only_secret = Env::default().with("CAD_API_SECRET", "s");
    let error = resolve(&only_secret, &fixture.store, ORIGIN).unwrap_err();
    assert!(matches!(&error, CliError::AuthConfig(m) if m == "CAD_API_KEY is not set"));

    let not_a_uuid = Env::default().with("CAD_API_KEY", "nope");
    let error = resolve(&not_a_uuid, &fixture.store, ORIGIN).unwrap_err();
    assert!(error.to_string().contains("CAD_API_KEY is not a UUID"));

    // The Key ID alone is known (completion keys its cache on it); the secret
    // is what is missing when the credentials are asked for.
    for environment in [
        Env::default().with("CAD_API_KEY", &KEY_ID.to_string()),
        Env::default()
            .with("CAD_API_KEY", &KEY_ID.to_string())
            .with("CAD_API_SECRET", ""),
    ] {
        let resolved = resolve(&environment, &fixture.store, ORIGIN).unwrap();
        assert_eq!(resolved.key_id(), KEY_ID);
        assert_eq!(*resolved.source(), Source::Environment);
        let error = resolved.into_credentials().unwrap_err();
        assert!(
            matches!(&error, CliError::AuthConfig(m) if m == "CAD_API_SECRET is not set or empty")
        );
    }
}

#[test]
fn a_stored_login_resolves_to_credentials_with_its_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    let resolved = resolve(&Env::default(), &fixture.store, ORIGIN).unwrap();
    assert_eq!(resolved.key_id(), KEY_ID);
    assert_eq!(resolved.stored_at(), Some(stored_at()));
    assert_eq!(resolved.source().kind(), SourceKind::File);
    assert_eq!(resolved.source().path(), Some(fixture.path.as_path()));
    assert!(resolved.into_credentials().is_ok());
}

#[test]
fn another_origin_has_no_credentials_and_the_error_says_how_to_get_some() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();

    let error = resolve(&Env::default(), &fixture.store, OTHER_ORIGIN).unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)));
    let message = error.to_string();
    assert!(message.contains(OTHER_ORIGIN), "{message}");
    assert!(message.contains("cad-cli auth login"), "{message}");
    assert!(
        message.contains("CAD_API_KEY and CAD_API_SECRET"),
        "{message}"
    );
    assert!(!message.contains("keyring"), "{message}");
    assert!(!message.contains(SECRET), "{message}");
}

#[test]
fn a_failed_keyring_is_named_in_the_missing_credentials_error() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = with_keyring(dir.path(), MemoryBackend::unavailable("no secret service"));
    let error = resolve(&Env::default(), &fixture.store, ORIGIN).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("no secret service"), "{message}");
    assert!(message.contains("cad-cli auth login"), "{message}");

    // The file still answers when the keyring does not.
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    assert!(resolve(&Env::default(), &fixture.store, ORIGIN).is_ok());
}

#[test]
fn a_stored_entry_of_an_unknown_kind_stops_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .keyring
        .put(ORIGIN, r#"{"kind":"oauth","accessToken":"never-shown"}"#);
    // A readable file entry behind it is not used instead.
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    fixture
        .keyring
        .put(ORIGIN, r#"{"kind":"oauth","accessToken":"never-shown"}"#);

    let error = resolve(&Env::default(), &fixture.store, ORIGIN).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("oauth") && message.contains(ORIGIN),
        "{message}"
    );
    assert!(!message.contains("never-shown"));

    fixture.keyring.put(ORIGIN, "{not json");
    let error = resolve(&Env::default(), &fixture.store, ORIGIN).unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)));
}

#[test]
fn the_secret_read_from_a_store_is_the_one_stored() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = fixture(dir.path());
    fixture
        .store
        .save_in_file(ORIGIN, &credential(KEY_ID, SECRET))
        .unwrap();
    let (entry, _) = found(fixture.store.load(ORIGIN).unwrap());
    let StoredCredential::ApiKey { secret, .. } = entry;
    assert_eq!(secret.expose_secret(), SECRET);
}
