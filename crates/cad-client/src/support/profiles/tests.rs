//! The profiles file and the names in it. The file is a real one in a
//! temporary directory.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Company, Login, Profile, ProfileName, ProfileSet, ProfileStore};
use crate::error::CliError;
use crate::support::credentials::Place;

fn name(value: &str) -> ProfileName {
    value.parse().unwrap()
}

fn stored_at() -> DateTime<Utc> {
    "2026-10-21T12:00:00Z".parse().unwrap()
}

fn login() -> Login {
    Login {
        key_id: Uuid::from_u128(0xaa),
        user_id: Uuid::from_u128(0xbb).into(),
        email: Some("jane@example.test".to_owned()),
        storage: Place::Keyring,
        stored_at: stored_at(),
    }
}

fn profile(url: &str) -> Profile {
    Profile {
        url: url.to_owned(),
        allow_insecure_http: false,
        login: None,
        company: None,
    }
}

fn store(dir: &Path) -> (ProfileStore, PathBuf) {
    let path = dir.join("cad-cli").join("profiles.json");
    (ProfileStore::new(Some(path.clone())), path)
}

// --- names -------------------------------------------------------------------

#[test]
fn names_are_one_to_thirty_two_of_lowercase_digits_dash_and_underscore() {
    for valid in [
        "a",
        "0",
        "production",
        "my-profile_2",
        "9lives",
        &"a".repeat(32),
    ] {
        assert_eq!(valid.parse::<ProfileName>().unwrap().as_str(), valid);
    }
    for invalid in [
        "",
        "-a",
        "_a",
        "A",
        "Prod",
        "a b",
        "a.b",
        "a/b",
        "é",
        "a:b",
        &"a".repeat(33),
    ] {
        let reason = invalid.parse::<ProfileName>().unwrap_err();
        assert!(
            reason.contains("1 to 32 characters"),
            "{invalid:?}: {reason}"
        );
        assert!(reason.contains("a-z, 0-9, - and _"), "{reason}");
    }
}

#[test]
fn names_are_checked_when_read_from_the_file_too() {
    let error = serde_json::from_value::<ProfileSet>(
        json!({"version": 1, "profiles": {"Bad Name": {"url": "https://x.test"}}}),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("not a valid profile name"),
        "{error}"
    );
}

// --- the set -----------------------------------------------------------------

#[test]
fn an_active_name_that_is_not_a_profile_is_no_active_profile() {
    let set: ProfileSet = serde_json::from_value(json!({
        "version": 1,
        "active": "gone",
        "profiles": {"staging": {"url": "https://staging.example.test"}}
    }))
    .unwrap();
    assert_eq!(set.active(), None);
    assert!(set.get(&name("staging")).is_some());
}

#[test]
fn removing_the_active_profile_clears_active_and_other_profiles_stay() {
    let mut set = ProfileSet::default();
    set.insert(name("a"), profile("https://a.test"));
    set.insert(name("b"), profile("https://b.test"));
    set.activate(name("a"));

    let (removed, was_active) = set.remove(&name("b")).unwrap();
    assert_eq!(removed.url, "https://b.test");
    assert!(!was_active);
    assert_eq!(set.active(), Some(&name("a")));

    let (_, was_active) = set.remove(&name("a")).unwrap();
    assert!(was_active);
    assert_eq!(set.active(), None);
    assert!(set.remove(&name("a")).is_none());
    assert!(set.is_empty());
}

#[test]
fn profiles_are_found_by_origin_and_listed_sorted() {
    let mut set = ProfileSet::default();
    for (profile_name, url) in [
        ("zeta", "https://shared.test"),
        ("alpha", "https://shared.test"),
        ("mid", "https://other.test"),
    ] {
        set.insert(name(profile_name), profile(url));
    }
    let names: Vec<_> = set.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["alpha", "mid", "zeta"]);
    let shared: Vec<_> = set
        .with_origin("https://shared.test")
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(shared, ["alpha", "zeta"]);
}

// --- the file ----------------------------------------------------------------

#[test]
fn a_missing_file_is_an_empty_set_and_a_store_without_a_path_too() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = store(dir.path());
    let set = store.load().unwrap();
    assert!(set.is_empty());
    assert_eq!(set.active(), None);
    assert!(!path.exists(), "reading creates nothing");

    assert!(ProfileStore::new(None).load().unwrap().is_empty());
    let error = ProfileStore::new(None).save(&set).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no per-user configuration directory")
    );
}

#[test]
fn a_set_round_trips_in_the_documented_shape() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = store(dir.path());
    let mut set = ProfileSet::default();
    let mut production = profile("https://app.carbonaltdelete.eu");
    production.login = Some(login());
    production.company = Some(Company {
        id: Uuid::from_u128(0xcc).into(),
        name: Some("Acme GmbH".to_owned()),
    });
    set.insert(name("production"), production);
    let mut local = profile("http://localhost:5001");
    local.allow_insecure_http = true;
    set.insert(name("local"), local);
    set.activate(name("production"));
    store.save(&set).unwrap();

    let written: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        written,
        json!({
            "version": 1,
            "active": "production",
            "profiles": {
                "local": {"url": "http://localhost:5001", "allowInsecureHttp": true},
                "production": {
                    "url": "https://app.carbonaltdelete.eu",
                    "allowInsecureHttp": false,
                    "login": {
                        "keyId": Uuid::from_u128(0xaa),
                        "userId": Uuid::from_u128(0xbb),
                        "email": "jane@example.test",
                        "storage": "keyring",
                        "storedAt": "2026-10-21T12:00:00Z"
                    },
                    "company": {"id": Uuid::from_u128(0xcc), "name": "Acme GmbH"}
                }
            }
        })
    );
    assert_eq!(store.load().unwrap(), set);
    // Nothing is left beside it: the temporary file was renamed away.
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["profiles.json"]);
}

#[test]
fn a_company_is_optional_and_its_name_may_be_null() {
    let set: ProfileSet = serde_json::from_value(json!({
        "version": 1,
        "profiles": {
            "a": {"url": "https://a.test"},
            "b": {"url": "https://b.test", "company": {"id": Uuid::from_u128(0xcc), "name": null}}
        }
    }))
    .unwrap();
    assert_eq!(set.get(&name("a")).unwrap().company, None);
    let company = set.get(&name("b")).unwrap().company.clone().unwrap();
    assert_eq!(company.name, None);
    assert_eq!(company.label(), Uuid::from_u128(0xcc).to_string());
    assert_eq!(
        serde_json::to_value(&company).unwrap(),
        json!({"id": Uuid::from_u128(0xcc), "name": null})
    );
}

#[test]
fn a_login_without_an_email_is_written_as_null_and_read_back() {
    let mut without = login();
    without.email = None;
    let value = serde_json::to_value(&without).unwrap();
    assert_eq!(value["email"], Value::Null);
    assert_eq!(serde_json::from_value::<Login>(value).unwrap(), without);
    assert_eq!(without.account(), Uuid::from_u128(0xbb).to_string());
    assert_eq!(login().account(), "jane@example.test");
}

#[test]
fn fields_this_version_does_not_know_are_tolerated() {
    let set: ProfileSet = serde_json::from_value(json!({
        "version": 1,
        "profiles": {"a": {"url": "https://a.test", "theme": "later"}}
    }))
    .unwrap();
    assert!(set.get(&name("a")).is_some());
}

#[test]
fn a_damaged_or_newer_file_is_an_error_naming_it_and_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let (store, path) = store(dir.path());
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();

    for (content, expected) in [
        ("not json", "is not a profiles file"),
        ("[]", "is not a profiles file"),
        (r#"{"version":1}"#, "is not a profiles file"),
        (
            r#"{"version":1,"profiles":{"A":{"url":"https://a.test"}}}"#,
            "is not a profiles file",
        ),
        (r#"{"version":2,"profiles":{}}"#, "is version 2"),
    ] {
        std::fs::write(&path, content).unwrap();
        let error = store.load().unwrap_err();
        assert!(matches!(error, CliError::Config { .. }), "{content}");
        let message = error.to_string();
        assert!(message.contains(&path.display().to_string()), "{message}");
        assert!(message.contains(expected), "{content}: {message}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
}

#[cfg(unix)]
#[test]
fn the_file_is_written_0600_in_a_0700_directory_and_reading_does_not_check_the_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let (store, path) = store(dir.path());
    let mut set = ProfileSet::default();
    set.insert(name("a"), profile("https://a.test"));
    store.save(&set).unwrap();
    let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&path), 0o600);
    assert_eq!(mode(path.parent().unwrap()), 0o700);

    // A second write replaces the file and keeps both modes.
    set.insert(name("b"), profile("https://b.test"));
    store.save(&set).unwrap();
    assert_eq!(mode(&path), 0o600);
    assert_eq!(store.load().unwrap(), set);

    // It holds no secret, so a more open file is read all the same.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(store.load().unwrap(), set);
}
