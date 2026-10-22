use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use cad_api::ids::CompanyId;
use clap::CommandFactory;
use clap_complete::Shell;
use clap_complete::engine::ArgValueCompleter;
use uuid::Uuid;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::cache::{self, Cache, CacheKey, Candidate, Network, TTL};
use super::{Resource, candidates, lookup, offline, preceding_words};
use crate::cli::Cli;
use crate::config::Environment;
use crate::support::credentials::Place;
use crate::support::credentials::memory::empty_store;
use crate::support::profiles::{Login, Profile, ProfileName, ProfileSet, ProfileStore};

const API_KEY: Uuid = Uuid::from_u128(0x1234_5678_9abc_4def_8123_4567_89ab_cdef);
const SECRET: &str = "s3cr3t-never-on-disk";
const COMPANY_ID: Uuid = Uuid::from_u128(0x42);
const OTHER_COMPANY_ID: Uuid = Uuid::from_u128(0x77);
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";
const USER_ROLES_PATH: &str = "/api/accounts/v1/user-roles";
const REPORTING_PERIODS_PATH: &str = "/api/reporting-periods/v1/reporting-periods";
const NO_CANDIDATES: [Candidate; 0] = [];

#[derive(Default)]
struct FakeEnvironment(HashMap<String, String>);

impl FakeEnvironment {
    fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.0.insert(key.to_owned(), value.into());
        self
    }

    /// Credentials plus a private cache directory.
    fn configured(cache: &Path) -> Self {
        Self::default()
            .with("CAD_API_KEY", API_KEY.to_string())
            .with("CAD_API_SECRET", SECRET)
            .with("XDG_CACHE_HOME", cache.to_string_lossy())
    }
}

impl Environment for FakeEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

/// No profile: nothing in the profiles file.
fn no_profiles() -> ProfileStore {
    ProfileStore::new(None)
}

fn words(words: &[&str]) -> Vec<OsString> {
    words.iter().map(OsString::from).collect()
}

fn candidate(value: &str) -> Candidate {
    Candidate::new(value, None)
}

fn key() -> CacheKey {
    CacheKey::session("operable-companies", "https://example.test/", API_KEY)
}

fn hours_ago(hours: u64) -> SystemTime {
    SystemTime::now() - Duration::from_hours(hours)
}

// --- word selection -------------------------------------------------------

#[test]
fn preceding_words_stop_before_the_word_at_the_reported_index() {
    let args = words(&[
        "cad-cli",
        "--",
        "cad-cli",
        "--company-id",
        "",
        "company",
        "list",
    ]);
    // bash and zsh report the index, and pass words after the cursor too.
    assert_eq!(
        preceding_words(&args, Some(2)),
        words(&["cad-cli", "--company-id"])
    );
}

#[test]
fn preceding_words_without_an_index_drop_the_last_word() {
    let args = words(&["cad-cli", "--", "cad-cli", "reporting-period", "get", "11"]);
    assert_eq!(
        preceding_words(&args, None),
        words(&["cad-cli", "reporting-period", "get"])
    );
}

#[test]
fn preceding_words_need_the_completion_separator() {
    assert_eq!(
        preceding_words(&words(&["cad-cli", "company", "list"]), None),
        Vec::<OsString>::new()
    );
}

// --- cache policy ---------------------------------------------------------

#[test]
fn a_fresh_cache_answers_without_a_live_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    cache
        .write(&key(), &[candidate("cached")], SystemTime::now())
        .unwrap();
    let called = Cell::new(false);

    let values = cache::lookup(
        Some(&cache),
        &key(),
        Network::Allowed,
        SystemTime::now(),
        || {
            called.set(true);
            Some(vec![candidate("live")])
        },
    );

    assert_eq!(values, [candidate("cached")]);
    assert!(!called.get());
}

#[test]
fn a_stale_cache_is_refreshed_by_a_successful_live_lookup() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    let expired = TTL.as_secs() / 3600 + 1;
    cache
        .write(&key(), &[candidate("stale")], hours_ago(expired))
        .unwrap();

    let now = SystemTime::now();
    let values = cache::lookup(Some(&cache), &key(), Network::Allowed, now, || {
        Some(vec![candidate("live")])
    });
    assert_eq!(values, [candidate("live")]);

    // The refreshed list is fresh again: the next lookup does not go live.
    let values = cache::lookup(Some(&cache), &key(), Network::Allowed, now, || {
        panic!("a fresh cache must not trigger a live lookup")
    });
    assert_eq!(values, [candidate("live")]);
}

#[test]
fn a_stale_cache_answers_when_the_live_lookup_fails() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    cache
        .write(&key(), &[candidate("stale")], hours_ago(24 * 30))
        .unwrap();

    let values = cache::lookup(
        Some(&cache),
        &key(),
        Network::Allowed,
        SystemTime::now(),
        || None,
    );

    assert_eq!(values, [candidate("stale")]);
}

#[test]
fn offline_mode_answers_from_any_cache_and_never_goes_live() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    let never = || -> Option<Vec<Candidate>> { panic!("offline mode must not go live") };

    assert_eq!(
        cache::lookup(
            Some(&cache),
            &key(),
            Network::Disabled,
            SystemTime::now(),
            never
        ),
        NO_CANDIDATES
    );

    cache
        .write(&key(), &[candidate("stale")], hours_ago(24))
        .unwrap();
    assert_eq!(
        cache::lookup(
            Some(&cache),
            &key(),
            Network::Disabled,
            SystemTime::now(),
            never
        ),
        [candidate("stale")]
    );
}

#[test]
fn without_a_cache_directory_the_live_answer_is_still_used() {
    let values = cache::lookup(None, &key(), Network::Allowed, SystemTime::now(), || {
        Some(vec![candidate("live")])
    });
    assert_eq!(values, [candidate("live")]);
}

#[test]
fn an_unreadable_cache_file_is_a_miss() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    cache
        .write(&key(), &[candidate("cached")], SystemTime::now())
        .unwrap();
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        std::fs::write(entry.unwrap().path(), b"{not json").unwrap();
    }

    let values = cache::lookup(
        Some(&cache),
        &key(),
        Network::Allowed,
        SystemTime::now(),
        || None,
    );
    assert_eq!(values, NO_CANDIDATES);
}

#[test]
fn cache_writes_are_atomic_and_leave_no_temporary_files() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().join("nested"));
    cache
        .write(&key(), &[candidate("one")], SystemTime::now())
        .unwrap();
    cache
        .write(&key(), &[candidate("two")], SystemTime::now())
        .unwrap();

    let names: Vec<_> = std::fs::read_dir(dir.path().join("nested"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].starts_with("operable-companies-"), "{names:?}");
    assert_eq!(
        Path::new(&names[0])
            .extension()
            .and_then(|extension| extension.to_str()),
        Some("json")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.path().join("nested"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}

#[test]
fn cache_keys_isolate_servers_api_keys_companies_and_resources() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::at(dir.path().to_owned());
    let other_key = Uuid::from_u128(1);
    let keys = [
        CacheKey::session("operable-companies", "https://a.test/", API_KEY),
        CacheKey::session("operable-companies", "https://b.test/", API_KEY),
        CacheKey::session("operable-companies", "https://a.test/", other_key),
        CacheKey::company(
            "reporting-periods",
            "https://a.test/",
            API_KEY,
            CompanyId::from(COMPANY_ID),
        ),
        CacheKey::company(
            "reporting-periods",
            "https://a.test/",
            API_KEY,
            CompanyId::from(OTHER_COMPANY_ID),
        ),
    ];
    for (index, key) in keys.iter().enumerate() {
        cache
            .write(key, &[candidate(&index.to_string())], SystemTime::now())
            .unwrap();
    }

    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), keys.len());
    for (index, key) in keys.iter().enumerate() {
        let values = cache::lookup(
            Some(&cache),
            key,
            Network::Disabled,
            SystemTime::now(),
            || None,
        );
        assert_eq!(values, [candidate(&index.to_string())], "{key:?}");
    }
}

#[test]
fn the_cache_directory_follows_xdg_cache_home_then_the_platform_default() {
    // An absolute path on the platform running the test: `/xdg` is not one
    // on Windows.
    let xdg_root = std::env::temp_dir().join("xdg");
    let xdg = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", xdg_root.to_string_lossy())
        .with("HOME", std::env::temp_dir().join("home").to_string_lossy())
        .with(
            "LOCALAPPDATA",
            std::env::temp_dir().join("local").to_string_lossy(),
        );
    let cache = format!("{:?}", Cache::from_environment(&xdg).unwrap());
    assert!(cache.contains("cad-cli"), "{cache}");
    assert!(cache.contains("xdg"), "{cache}");

    let relative = FakeEnvironment::default().with("XDG_CACHE_HOME", "relative");
    assert!(Cache::from_environment(&relative).is_none());
}

#[test]
fn the_offline_switch_accepts_common_truthy_spellings() {
    for (value, expected) in [
        ("1", true),
        ("true", true),
        ("yes", true),
        ("0", false),
        ("false", false),
        ("", false),
    ] {
        let environment = FakeEnvironment::default().with(super::OFFLINE_VAR, value);
        assert_eq!(offline(&environment), expected, "{value:?}");
    }
    assert!(!offline(&FakeEnvironment::default()));
}

// --- completers -----------------------------------------------------------

#[test]
fn completers_are_attached_to_the_company_and_reporting_period_ids() {
    let mut root = Cli::command();
    root.build();
    let has_completer = |command: &clap::Command, id: &str| {
        command
            .get_arguments()
            .find(|arg| arg.get_id() == id)
            .is_some_and(|arg| arg.get::<ArgValueCompleter>().is_some())
    };

    assert!(has_completer(&root, "company_id"));
    let periods = root.find_subcommand("reporting-period").unwrap();
    for name in ["get", "details", "update", "delete"] {
        let command = periods.find_subcommand(name).unwrap();
        assert!(has_completer(command, "id"), "reporting-period {name}");
        // Global options reach every subcommand together with their completer.
        assert!(
            has_completer(command, "company_id"),
            "reporting-period {name}"
        );
    }
    // The company `use company` takes is completed with the same companies.
    let use_company = root
        .find_subcommand("use")
        .unwrap()
        .find_subcommand("company")
        .unwrap();
    assert!(has_completer(use_company, "company"));
}

/// What the engine offers for the last of `line`, as the shell would ask.
fn offered(line: &[&str]) -> Vec<(String, Option<String>)> {
    let args: Vec<OsString> = line.iter().map(OsString::from).collect();
    let index = args.len() - 1;
    clap_complete::engine::complete(&mut Cli::command(), args, index, None)
        .unwrap()
        .into_iter()
        .map(|candidate| {
            (
                candidate.get_value().to_string_lossy().into_owned(),
                candidate.get_help().map(ToString::to_string),
            )
        })
        .collect()
}

#[test]
fn a_dash_is_offered_to_pick_a_profile_or_a_company_interactively() {
    let dash = ("-".to_owned(), Some("Select interactively".to_owned()));
    for line in [
        ["cad-cli", "use", "profile", ""].as_slice(),
        ["cad-cli", "use", "profile", "-"].as_slice(),
        ["cad-cli", "use", "company", ""].as_slice(),
        ["cad-cli", "use", "company", "-"].as_slice(),
        ["cad-cli", "--company-id", ""].as_slice(),
        ["cad-cli", "--company-id", "-"].as_slice(),
        ["cad-cli", "reporting-period", "list", "--company-id", "-"].as_slice(),
    ] {
        assert!(
            offered(line).contains(&dash),
            "{line:?}: {:?}",
            offered(line)
        );
    }
}

#[test]
fn a_dash_is_not_offered_where_it_picks_nothing() {
    // A word that is not a prefix of `-` is no request for it.
    for line in [
        ["cad-cli", "use", "profile", "s"].as_slice(),
        ["cad-cli", "use", "company", "0"].as_slice(),
        ["cad-cli", "--company-id", "0"].as_slice(),
        // `--profile` names a profile; it has nothing to pick from a list.
        ["cad-cli", "--profile", ""].as_slice(),
        ["cad-cli", "--profile", "-"].as_slice(),
        // A report's reporting period is an ID, picked by none of these.
        ["cad-cli", "reporting-period", "get", ""].as_slice(),
    ] {
        assert!(
            offered(line).iter().all(|(value, _)| value != "-"),
            "{line:?}: {:?}",
            offered(line)
        );
    }
}

#[test]
fn registration_matches_the_complete_env_protocol_for_every_shell() {
    for shell in [
        Shell::Bash,
        Shell::Elvish,
        Shell::Fish,
        Shell::PowerShell,
        Shell::Zsh,
    ] {
        let mut buffer = Vec::new();
        super::write_registration(shell, &mut buffer).unwrap();
        let script = String::from_utf8(buffer).unwrap();
        assert!(script.contains("COMPLETE"), "{shell}: {script}");
        assert!(script.contains("cad-cli"), "{shell}: {script}");
    }
}

#[test]
fn without_credentials_nothing_is_completed_and_nothing_is_cached() {
    let dir = tempfile::tempdir().unwrap();
    let environment =
        FakeEnvironment::default().with("XDG_CACHE_HOME", dir.path().to_string_lossy());
    let line = words(&[
        "cad-cli",
        "--server-url",
        "https://staging.carbonaltdelete.eu",
    ]);

    for resource in [Resource::OperableCompanies, Resource::ReportingPeriods] {
        assert_eq!(
            candidates(
                resource,
                &line,
                &environment,
                &empty_store(),
                &no_profiles(),
                SystemTime::now()
            ),
            NO_CANDIDATES
        );
    }
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn without_a_server_nothing_is_completed() {
    let dir = tempfile::tempdir().unwrap();
    let environment = FakeEnvironment::configured(dir.path());
    assert_eq!(
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &empty_store(),
            &no_profiles(),
            SystemTime::now()
        ),
        NO_CANDIDATES
    );
}

#[test]
fn an_invalid_company_variable_completes_no_reporting_periods() {
    let dir = tempfile::tempdir().unwrap();
    let environment = FakeEnvironment::configured(dir.path()).with("CAD_COMPANY_ID", "nope");
    assert_eq!(
        candidates(
            Resource::ReportingPeriods,
            &words(&[
                "cad-cli",
                "--server-url",
                "https://staging.carbonaltdelete.eu",
                "reporting-period",
                "get"
            ]),
            &environment,
            &empty_store(),
            &no_profiles(),
            SystemTime::now()
        ),
        NO_CANDIDATES
    );
}

// --- live lookups against a loopback platform -----------------------------

fn identity(company: Uuid) -> serde_json::Value {
    serde_json::json!({
        "principal": { "userId": Uuid::nil(), "affiliatedCompanyId": COMPANY_ID },
        "companySelection": { "resolvedCompanyId": company, "source": "DEFAULT" }
    })
}

fn company(id: Uuid, name: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "displayName": null,
        "createdAt": "2026-01-01T00:00:00Z",
        "isConsultingCompany": false,
        "isDemoCompany": false,
        "availableClientCompanies": null,
        "companyStatus": status
    })
}

/// A role of the identity's user (`Uuid::nil()`) in `company`.
fn role(company: Uuid, role: &str) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::new_v4(),
        "userId": Uuid::nil(),
        "companyId": company,
        "userRole": role
    })
}

/// Mounts the roles of the identity's user, the way the platform filters them.
async fn mount_roles(server: &MockServer, roles: &[serde_json::Value]) {
    Mock::given(method("GET"))
        .and(path(USER_ROLES_PATH))
        .and(query_param("userId_equals", Uuid::nil().to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(roles))
        .mount(server)
        .await;
}

fn period(id: u128, short_name: &str, start: &str, end: &str) -> serde_json::Value {
    serde_json::json!({
        "id": Uuid::from_u128(id),
        "description": short_name,
        "shortName": short_name,
        "startDate": start,
        "endDate": end,
        "isLocked": false
    })
}

async fn platform() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(COMPANY_ID)))
        .mount(&server)
        .await;
    server
}

fn server_words(server: &MockServer, rest: &[&str]) -> Vec<OsString> {
    let mut line = words(&[
        "cad-cli",
        "--server-url",
        &server.uri(),
        "--allow-insecure-http",
    ]);
    line.extend(words(rest));
    line
}

/// Runs a completer off the async runtime: it blocks on its own lookup thread.
async fn complete(
    resource: Resource,
    line: Vec<OsString>,
    environment: FakeEnvironment,
) -> Vec<Candidate> {
    tokio::task::spawn_blocking(move || {
        candidates(
            resource,
            &line,
            &environment,
            &empty_store(),
            &no_profiles(),
            SystemTime::now(),
        )
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn company_ids_are_fetched_through_the_sdk_cached_and_reused() {
    let server = platform().await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            company(OTHER_COMPANY_ID, "Zeta AG", "ACTIVE"),
            company(Uuid::from_u128(0x99), "Gone GmbH", "DELETED"),
            company(Uuid::from_u128(0x98), "Client GmbH", "ACTIVE"),
            company(COMPANY_ID, "Acme GmbH", "ACTIVE"),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    // Zeta is held through a role, Gone is deleted, Client is visible with
    // no role that grants it: it is not offered.
    Mock::given(method("GET"))
        .and(path(USER_ROLES_PATH))
        .and(query_param("userId_equals", Uuid::nil().to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            role(OTHER_COMPANY_ID, "EXPERT"),
            role(Uuid::from_u128(0x99), "EXPERT"),
            role(Uuid::from_u128(0x98), "ADMIN"),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();

    let first = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;
    assert_eq!(
        first,
        [
            Candidate::new(COMPANY_ID.to_string(), Some("Acme GmbH".to_owned())),
            Candidate::new(OTHER_COMPANY_ID.to_string(), Some("Zeta AG".to_owned())),
        ]
    );

    // The second Tab press is answered from the cache: `expect(1)` above fails
    // the test on drop if the companies or roles endpoint is called again.
    let second = complete(
        Resource::OperableCompanies,
        server_words(&server, &["company", "list"]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;
    assert_eq!(second, first);

    let files = walk(dir.path());
    assert_eq!(files.len(), 1);
    let stored = std::fs::read_to_string(&files[0]).unwrap();
    assert!(!stored.contains(SECRET), "{stored}");
    assert!(!stored.contains(&API_KEY.to_string()), "{stored}");
    assert!(!files[0].to_string_lossy().contains(&API_KEY.to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn companies_are_helped_and_sorted_by_name_on_one_line_never_by_display_name() {
    let server = platform().await;
    let mut zeta = company(OTHER_COMPANY_ID, "Zeta\n AG", "ACTIVE");
    zeta["displayName"] = serde_json::json!("Aaa, a long\ndisplay text");
    let mut acme = company(COMPANY_ID, "Acme GmbH", "ACTIVE");
    acme["displayName"] = serde_json::json!("Zzz, another display text");
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([zeta, acme])))
        .mount(&server)
        .await;
    mount_roles(&server, &[role(OTHER_COMPANY_ID, "READ_ONLY")]).await;
    let dir = tempfile::tempdir().unwrap();

    let candidates = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;

    assert_eq!(
        candidates,
        [
            Candidate::new(COMPANY_ID.to_string(), Some("Acme GmbH".to_owned())),
            Candidate::new(OTHER_COMPANY_ID.to_string(), Some("Zeta AG".to_owned())),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reporting_periods_follow_the_selected_company() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .and(header("X-CAD-Company-Id", OTHER_COMPANY_ID.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(OTHER_COMPANY_ID)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(COMPANY_ID)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(REPORTING_PERIODS_PATH))
        .and(header("X-CAD-Company-Id", OTHER_COMPANY_ID.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            period(0x25, "FY25", "2025-01-01", "2025-12-31"),
            period(0x26, "FY26", "2026-01-01", "2026-12-31"),
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();

    let values = complete(
        Resource::ReportingPeriods,
        server_words(
            &server,
            &[
                "reporting-period",
                "get",
                "--company-id",
                &OTHER_COMPANY_ID.to_string(),
            ],
        ),
        FakeEnvironment::configured(dir.path()),
    )
    .await;

    assert_eq!(
        values,
        [
            Candidate::new(
                Uuid::from_u128(0x26).to_string(),
                Some("FY26 (2026-01-01 to 2026-12-31)".to_owned())
            ),
            Candidate::new(
                Uuid::from_u128(0x25).to_string(),
                Some("FY25 (2025-01-01 to 2025-12-31)".to_owned())
            ),
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reporting_periods_use_the_profiles_company_and_nothing_without_one() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .and(header("X-CAD-Company-Id", OTHER_COMPANY_ID.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(OTHER_COMPANY_ID)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(REPORTING_PERIODS_PATH))
        .and(header("X-CAD-Company-Id", OTHER_COMPANY_ID.to_string()))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([period(
                0x26,
                "FY26",
                "2026-01-01",
                "2026-12-31"
            )])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, _) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let line = words(&["cad-cli", "reporting-period", "get"]);
    let run = || {
        candidates(
            Resource::ReportingPeriods,
            &line,
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    };

    // The profile has no company: there is nothing to offer, and nothing was
    // asked of the platform, whose own choice of company is not used.
    assert_eq!(tokio::task::block_in_place(run), Vec::<Candidate>::new());
    assert!(server.received_requests().await.unwrap().is_empty());

    let mut set = profiles.load().unwrap();
    let mut work = set.get(&profile_name("work")).unwrap().clone();
    work.company = Some(crate::support::profiles::Company {
        id: OTHER_COMPANY_ID.into(),
        name: None,
    });
    set.insert(profile_name("work"), work);
    profiles.save(&set).unwrap();

    assert_eq!(
        tokio::task::block_in_place(run),
        [Candidate::new(
            Uuid::from_u128(0x26).to_string(),
            Some("FY26 (2026-01-01 to 2026-12-31)".to_owned())
        )]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn reporting_periods_offer_nothing_when_no_company_is_selected_at_all() {
    let server = platform().await;
    let dir = tempfile::tempdir().unwrap();

    let values = complete(
        Resource::ReportingPeriods,
        server_words(&server, &["reporting-period", "get"]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;

    assert_eq!(values, Vec::<Candidate>::new());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_platform_falls_back_to_the_stale_list() {
    let server = platform().await;
    mount_roles(&server, &[]).await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let origin = format!("{}/", server.uri());
    let cache = Cache::from_environment(&FakeEnvironment::configured(dir.path())).unwrap();
    cache
        .write(
            &CacheKey::session("operable-companies", &origin, API_KEY),
            &[candidate("stale")],
            hours_ago(48),
        )
        .unwrap();

    let values = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;

    assert_eq!(values, [candidate("stale")]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_platform_is_abandoned_after_the_live_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(identity(COMPANY_ID))
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();

    let started = Instant::now();
    let values = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        FakeEnvironment::configured(dir.path()),
    )
    .await;

    assert_eq!(values, NO_CANDIDATES);
    assert!(
        started.elapsed() < lookup::LIVE_TIMEOUT + Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_mode_never_contacts_the_platform() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();

    let values = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        FakeEnvironment::configured(dir.path()).with(super::OFFLINE_VAR, "1"),
    )
    .await;

    assert_eq!(values, NO_CANDIDATES);
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_entry_cached_before_the_companies_were_narrowed_is_never_served() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let environment = FakeEnvironment::configured(dir.path()).with(super::OFFLINE_VAR, "1");
    // What an earlier version cached under the old resource name: every
    // company the key can see.
    Cache::from_environment(&environment)
        .unwrap()
        .write(
            &CacheKey::session("companies", &format!("{}/", server.uri()), API_KEY),
            &[candidate("a-company-the-login-cannot-act-on")],
            SystemTime::now(),
        )
        .unwrap();

    let values = complete(
        Resource::OperableCompanies,
        server_words(&server, &[]),
        environment,
    )
    .await;

    assert_eq!(values, NO_CANDIDATES);
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(walk(&path));
        } else {
            files.push(path);
        }
    }
    files
}

// --- stored credentials ---------------------------------------------------

/// A keyring double that counts how often it is read.
struct CountingKeyring {
    reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl crate::support::credentials::Backend for CountingKeyring {
    fn get(
        &self,
        _profile: &str,
    ) -> Result<Option<secrecy::SecretString>, crate::support::credentials::StoreError> {
        self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(None)
    }

    fn set(
        &self,
        _profile: &str,
        _value: &secrecy::SecretString,
    ) -> Result<(), crate::support::credentials::StoreError> {
        unreachable!("completion never writes credentials")
    }

    fn remove(&self, _profile: &str) -> Result<bool, crate::support::credentials::StoreError> {
        // Only the test's own set-up removes entries (a file write clears the
        // keyring's); completion never does.
        Ok(false)
    }
}

/// A store whose credentials file holds the login of the profile `work`, whose
/// server is `server` and which is the active profile, and whose keyring is
/// read through a counter.
fn store_with_login(
    server: &MockServer,
    dir: &Path,
) -> (
    crate::support::credentials::CredentialStore,
    ProfileStore,
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
) {
    use crate::support::credentials::{CredentialStore, FileBackend, StoredCredential};

    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let store = CredentialStore::new(
        Some(Box::new(CountingKeyring {
            reads: std::sync::Arc::clone(&reads),
        })),
        Some(FileBackend::new(
            dir.join("config").join("credentials.json"),
        )),
    );
    let origin = server.uri();
    store
        .save_in_file(
            &profile_name("work"),
            &StoredCredential::api_key(
                origin.clone(),
                API_KEY,
                secrecy::SecretString::from("cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde"),
                Uuid::nil().into(),
                chrono::Utc::now(),
            ),
        )
        .unwrap();
    reads.store(0, std::sync::atomic::Ordering::SeqCst);
    let profiles = ProfileStore::new(Some(dir.join("config").join("profiles.json")));
    let mut set = ProfileSet::default();
    set.insert(
        profile_name("work"),
        Profile {
            url: origin,
            allow_insecure_http: true,
            login: Some(login_metadata(API_KEY)),
            company: None,
        },
    );
    set.activate(profile_name("work"));
    profiles.save(&set).unwrap();
    (store, profiles, reads)
}

/// What `profiles.json` records of a login, which is all the cache key needs.
fn login_metadata(key_id: Uuid) -> Login {
    Login {
        key_id,
        user_id: Uuid::nil().into(),
        email: None,
        storage: Place::File,
        stored_at: chrono::Utc::now(),
    }
}

fn profile_name(name: &str) -> ProfileName {
    name.parse().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stored_login_keys_the_cache_and_authenticates_the_live_lookup() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .and(header(
            "authorization",
            format!("Bearer {API_KEY}.cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde")
                .as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(COMPANY_ID)))
        .mount(&server)
        .await;
    mount_roles(&server, &[]).await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([company(
                COMPANY_ID,
                "Acme GmbH",
                "ACTIVE"
            )])),
        )
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, reads) = store_with_login(&server, dir.path());
    // No CAD_API_KEY or CAD_API_SECRET: the login is the only source. And no
    // server on the command line: the active profile's is the one.
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let line = words(&["cad-cli"]);

    let run = || {
        candidates(
            Resource::OperableCompanies,
            &line,
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    };
    let first = tokio::task::block_in_place(run);
    assert_eq!(
        first,
        [Candidate::new(
            COMPANY_ID.to_string(),
            Some("Acme GmbH".to_owned())
        )]
    );
    // Found once for the key and the live lookup together: the keyring was
    // asked a single time (it held nothing, so the file answered).
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);

    // The next Tab press is answered from the cache, which the stored Key ID
    // keys: `expect(1)` fails the test on drop if the endpoint is called again.
    let second = tokio::task::block_in_place(run);
    assert_eq!(second, first);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);

    let files = walk(&dir.path().join("cache"));
    assert_eq!(files.len(), 1);
    let stored = std::fs::read_to_string(&files[0]).unwrap();
    assert!(!stored.contains("cad_aks_"), "{stored}");
    assert!(!stored.contains(&API_KEY.to_string()), "{stored}");
}

#[test]
fn without_a_stored_login_nothing_is_completed_and_the_keyring_is_read_once() {
    let dir = tempfile::tempdir().unwrap();
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let store = crate::support::credentials::CredentialStore::new(
        Some(Box::new(CountingKeyring {
            reads: std::sync::Arc::clone(&reads),
        })),
        None,
    );
    let environment =
        FakeEnvironment::default().with("XDG_CACHE_HOME", dir.path().to_string_lossy());
    // The active profile is `staging`; its metadata records a login, but the
    // keyring and the file hold none, which only a live lookup finds out.
    let config = tempfile::tempdir().unwrap();
    let profiles = ProfileStore::new(Some(config.path().join("profiles.json")));
    let mut set = ProfileSet::default();
    set.insert(
        profile_name("staging"),
        Profile {
            url: "https://staging.carbonaltdelete.eu".to_owned(),
            allow_insecure_http: false,
            login: Some(login_metadata(API_KEY)),
            company: None,
        },
    );
    set.activate(profile_name("staging"));
    profiles.save(&set).unwrap();
    let line = words(&["cad-cli"]);

    assert_eq!(
        candidates(
            Resource::OperableCompanies,
            &line,
            &environment,
            &store,
            &profiles,
            SystemTime::now()
        ),
        NO_CANDIDATES
    );
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
}

#[test]
fn the_environment_is_not_overridden_by_a_stored_login_in_completion() {
    let dir = tempfile::tempdir().unwrap();
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let store = crate::support::credentials::CredentialStore::new(
        Some(Box::new(CountingKeyring {
            reads: std::sync::Arc::clone(&reads),
        })),
        None,
    );
    // Offline, and a cache that is fresh for the environment's key.
    let environment = FakeEnvironment::configured(dir.path()).with(super::OFFLINE_VAR, "1");
    let cache = Cache::from_environment(&environment).unwrap();
    cache
        .write(
            &CacheKey::session(
                "operable-companies",
                "https://staging.carbonaltdelete.eu/",
                API_KEY,
            ),
            &[candidate("from-cache")],
            SystemTime::now(),
        )
        .unwrap();

    let values = candidates(
        Resource::OperableCompanies,
        &words(&[
            "cad-cli",
            "--server-url",
            "https://staging.carbonaltdelete.eu",
        ]),
        &environment,
        &store,
        &no_profiles(),
        SystemTime::now(),
    );
    assert_eq!(values, [candidate("from-cache")]);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_profile_flag_and_the_server_it_picks_complete_with_that_profiles_login() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(COMPANY_ID)))
        .mount(&server)
        .await;
    mount_roles(&server, &[]).await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([company(
                COMPANY_ID,
                "Acme GmbH",
                "ACTIVE"
            )])),
        )
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, _) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let expected = [Candidate::new(
        COMPANY_ID.to_string(),
        Some("Acme GmbH".to_owned()),
    )];

    // Another profile is active, so only the flag (or the server) reaches `work`.
    let mut set = profiles.load().unwrap();
    set.insert(
        profile_name("other"),
        Profile {
            url: "https://other.example.test".to_owned(),
            allow_insecure_http: false,
            login: None,
            company: None,
        },
    );
    set.activate(profile_name("other"));
    profiles.save(&set).unwrap();

    for line in [
        words(&["cad-cli", "--profile", "work"]),
        server_words(&server, &[]),
    ] {
        let values = tokio::task::block_in_place(|| {
            candidates(
                Resource::OperableCompanies,
                &line,
                &environment,
                &store,
                &profiles,
                SystemTime::now(),
            )
        });
        assert_eq!(values, expected, "{line:?}");
    }
    // The active profile has no login, and the answer says nothing.
    assert_eq!(
        tokio::task::block_in_place(|| candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )),
        NO_CANDIDATES
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_server_that_is_not_a_profiles_never_borrows_the_active_profiles_login() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, reads) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());

    for line in [
        words(&["cad-cli", "--server-url", "https://elsewhere.example.test"]),
        words(&[
            "cad-cli",
            "--profile",
            "work",
            "--server-url",
            "https://elsewhere.example.test",
        ]),
    ] {
        let values = tokio::task::block_in_place(|| {
            candidates(
                Resource::OperableCompanies,
                &line,
                &environment,
                &store,
                &profiles,
                SystemTime::now(),
            )
        });
        assert_eq!(values, NO_CANDIDATES, "{line:?}");
    }
    // Nothing was sent anywhere, and the store was not even read.
    assert!(server.received_requests().await.unwrap().is_empty());
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(!dir.path().join("cache").exists());
}

/// A profile `work` at `server` that records a login with `key_id`, active.
fn profiles_with_login(dir: &Path, server: &MockServer, key_id: Uuid) -> ProfileStore {
    let profiles = ProfileStore::new(Some(dir.join("config").join("profiles.json")));
    let mut set = profiles.load().unwrap_or_default();
    set.insert(
        profile_name("work"),
        Profile {
            url: server.uri(),
            allow_insecure_http: true,
            login: Some(login_metadata(key_id)),
            company: None,
        },
    );
    set.activate(profile_name("work"));
    profiles.save(&set).unwrap();
    profiles
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fresh_cache_hit_reads_no_keyring_and_sends_nothing() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, reads) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let cache = Cache::from_environment(&environment).unwrap();
    // The Key ID in the profile's metadata alone keys the entry.
    cache
        .write(
            &CacheKey::session("operable-companies", &format!("{}/", server.uri()), API_KEY),
            &[candidate("from-cache")],
            SystemTime::now(),
        )
        .unwrap();

    let values = tokio::task::block_in_place(|| {
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    });

    assert_eq!(values, [candidate("from-cache")]);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_completion_never_reads_the_keyring() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, reads) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy())
        .with(super::OFFLINE_VAR, "1");
    let cache = Cache::from_environment(&environment).unwrap();
    cache
        .write(
            &CacheKey::session("operable-companies", &format!("{}/", server.uri()), API_KEY),
            &[candidate("stale")],
            hours_ago(48),
        )
        .unwrap();

    let run = |resource| {
        tokio::task::block_in_place(|| {
            candidates(
                resource,
                &words(&["cad-cli"]),
                &environment,
                &store,
                &profiles,
                SystemTime::now(),
            )
        })
    };

    assert_eq!(run(Resource::OperableCompanies), [candidate("stale")]);
    // Nothing cached for the second resource: no candidates, still no read.
    assert_eq!(run(Resource::ReportingPeriods), NO_CANDIDATES);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stale_entry_reads_the_keyring_once_to_go_live_and_is_refreshed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(COMPANY_ID)))
        .mount(&server)
        .await;
    mount_roles(&server, &[]).await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([company(
                COMPANY_ID,
                "Acme GmbH",
                "ACTIVE"
            )])),
        )
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles, reads) = store_with_login(&server, dir.path());
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let cache = Cache::from_environment(&environment).unwrap();
    cache
        .write(
            &CacheKey::session("operable-companies", &format!("{}/", server.uri()), API_KEY),
            &[candidate("stale")],
            hours_ago(48),
        )
        .unwrap();

    let values = tokio::task::block_in_place(|| {
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    });

    assert_eq!(
        values,
        [Candidate::new(
            COMPANY_ID.to_string(),
            Some("Acme GmbH".to_owned())
        )]
    );
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stored_login_with_another_key_id_than_the_metadata_does_not_go_live() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    // The credentials file holds the login of `API_KEY`, but the profile's
    // metadata, and so the cache key, name another key.
    let (store, _, reads) = store_with_login(&server, dir.path());
    let other_key = Uuid::from_u128(0xdead);
    let profiles = profiles_with_login(dir.path(), &server, other_key);
    let environment = FakeEnvironment::default()
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let cache = Cache::from_environment(&environment).unwrap();
    cache
        .write(
            &CacheKey::session(
                "operable-companies",
                &format!("{}/", server.uri()),
                other_key,
            ),
            &[candidate("stale")],
            hours_ago(48),
        )
        .unwrap();

    let values = tokio::task::block_in_place(|| {
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    });

    // The stale entry is served, and nothing was sent with the wrong login.
    assert_eq!(values, [candidate("stale")]);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_environments_key_id_keys_the_cache_whatever_the_profile_records() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, _, reads) = store_with_login(&server, dir.path());
    let profiles = profiles_with_login(dir.path(), &server, Uuid::from_u128(0xdead));
    // Only the key: no secret is needed to answer from the cache.
    let environment = FakeEnvironment::default()
        .with("CAD_API_KEY", API_KEY.to_string())
        .with("XDG_CACHE_HOME", dir.path().join("cache").to_string_lossy());
    let cache = Cache::from_environment(&environment).unwrap();
    cache
        .write(
            &CacheKey::session("operable-companies", &format!("{}/", server.uri()), API_KEY),
            &[candidate("from-env-key")],
            SystemTime::now(),
        )
        .unwrap();

    let values = tokio::task::block_in_place(|| {
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &store,
            &profiles,
            SystemTime::now(),
        )
    });

    assert_eq!(values, [candidate("from-env-key")]);
    assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn a_malformed_environment_key_means_no_candidates_not_the_profiles_login() {
    let dir = tempfile::tempdir().unwrap();
    let environment = FakeEnvironment::configured(dir.path()).with("CAD_API_KEY", "not-a-uuid");
    let profiles = ProfileStore::new(Some(dir.path().join("profiles.json")));
    let mut set = ProfileSet::default();
    set.insert(
        profile_name("work"),
        Profile {
            url: "https://staging.carbonaltdelete.eu".to_owned(),
            allow_insecure_http: false,
            login: Some(login_metadata(API_KEY)),
            company: None,
        },
    );
    set.activate(profile_name("work"));
    profiles.save(&set).unwrap();

    assert_eq!(
        candidates(
            Resource::OperableCompanies,
            &words(&["cad-cli"]),
            &environment,
            &empty_store(),
            &profiles,
            SystemTime::now(),
        ),
        NO_CANDIDATES
    );
}
