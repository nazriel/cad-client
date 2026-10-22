//! `use` and `use profile` end to end through the command group, with a
//! scripted terminal, a `wiremock` platform, the in-memory keyring, and a
//! credentials file and a profiles file in a temporary directory. The real
//! keyring and the real configuration directory are never touched.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use clap::Parser;
use secrecy::SecretString;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Report;
use crate::app::context::CommandContext;
use crate::cli::{Cli, OutputFormat};
use crate::cmds::Command;
use crate::config::Environment;
use crate::error::CliError;
use crate::support::credentials::memory::MemoryBackend;
use crate::support::credentials::{Backend, CredentialStore, FileBackend, StoreError};
use crate::support::profiles::{ProfileName, ProfileSet, ProfileStore};
use crate::support::terminal::{Progress, StatusKind, Terminal, Validator};

const KEY_ID: &str = "00000000-0000-0000-0000-0000000000aa";
const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";
const USER_ID: &str = "00000000-0000-0000-0000-0000000000bb";
const COMPANY_ID: &str = "00000000-0000-0000-0000-0000000000cc";
const EMAIL: &str = "jane@example.test";
const OTHER_COMPANY_ID: &str = "00000000-0000-0000-0000-0000000000dd";
const THIRD_COMPANY_ID: &str = "00000000-0000-0000-0000-0000000000ee";
const FORBIDDEN_COMPANY_ID: &str = "00000000-0000-0000-0000-0000000000ff";
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";
const USER_ROLES_PATH: &str = "/api/accounts/v1/user-roles";
const USER_PATH: &str = "/api/accounts/v1/users/00000000-0000-0000-0000-0000000000bb";
const KEYRING_DOWN: &str = "no secret service is running";

fn token() -> String {
    format!("{KEY_ID}.{SECRET}")
}

fn name(value: &str) -> ProfileName {
    value.parse().expect("a valid profile name")
}

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

/// A terminal that answers prompts from a script and records everything.
#[derive(Default)]
struct Scripted {
    interactive: bool,
    stdin: Vec<u8>,
    urls: Mutex<VecDeque<String>>,
    key_ids: Mutex<VecDeque<String>>,
    secrets: Mutex<VecDeque<String>>,
    answers: Mutex<VecDeque<bool>>,
    /// The indexes a person picks in `select`, in order; Esc when none is left.
    selections: Mutex<VecDeque<usize>>,
    /// What `select` showed: the label, then the options.
    menus: Mutex<Vec<(String, Vec<String>)>>,
    /// Where the cursor started in each of those menus.
    starts: Mutex<Vec<usize>>,
    labels: Mutex<Vec<String>>,
    questions: Mutex<Vec<String>>,
    opened: Mutex<Vec<String>>,
    statuses: Mutex<Vec<(StatusKind, String)>>,
}

impl Scripted {
    fn piped(stdin: &str) -> Self {
        Self {
            stdin: stdin.as_bytes().to_vec(),
            ..Self::default()
        }
    }

    fn typing(key_ids: &[&str], secrets: &[&str], answers: &[bool]) -> Self {
        Self::at_a_terminal(&[], key_ids, secrets, answers)
    }

    fn at_a_terminal(urls: &[&str], key_ids: &[&str], secrets: &[&str], answers: &[bool]) -> Self {
        let queue = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        Self {
            interactive: true,
            urls: Mutex::new(queue(urls)),
            key_ids: Mutex::new(queue(key_ids)),
            secrets: Mutex::new(queue(secrets)),
            answers: Mutex::new(answers.iter().copied().collect()),
            ..Self::default()
        }
    }

    /// What a person typing into `inquire` does: a rejected value is shown with
    /// its reason as a warning and typed again; with nothing left to type, they
    /// press Esc.
    fn answer(
        &self,
        queue: &Mutex<VecDeque<String>>,
        validate: Validator,
    ) -> Result<String, CliError> {
        loop {
            let typed = queue
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(CliError::Cancelled)?;
            match validate(&typed) {
                Ok(()) => return Ok(typed),
                Err(reason) => self.status(StatusKind::Warning, &reason)?,
            }
        }
    }

    /// A terminal whose person picks `indexes` from the menus, in order.
    fn picking(indexes: &[usize]) -> Self {
        Self {
            interactive: true,
            selections: Mutex::new(indexes.iter().copied().collect()),
            ..Self::default()
        }
    }

    fn menus(&self) -> Vec<(String, Vec<String>)> {
        self.menus.lock().unwrap().clone()
    }

    fn starts(&self) -> Vec<usize> {
        self.starts.lock().unwrap().clone()
    }

    fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }

    fn questions(&self) -> Vec<String> {
        self.questions.lock().unwrap().clone()
    }

    fn labels(&self) -> Vec<String> {
        self.labels.lock().unwrap().clone()
    }

    fn statuses(&self) -> Vec<(StatusKind, String)> {
        self.statuses.lock().unwrap().clone()
    }
}

struct NoProgress;

impl Progress for NoProgress {}

impl Terminal for Scripted {
    fn status(&self, kind: StatusKind, message: &str) -> Result<(), CliError> {
        self.statuses
            .lock()
            .unwrap()
            .push((kind, message.to_owned()));
        Ok(())
    }

    fn notes(&self, _heading: &str, _body: &str) -> Result<(), CliError> {
        Ok(())
    }

    fn confirm(&self, _prompt: &str, _assume_yes: bool) -> Result<(), CliError> {
        panic!("`use` never goes through `confirm`: `--yes` must not answer its questions");
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(self.stdin.clone())
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn prompt(&self, label: &str, validate: Validator) -> Result<String, CliError> {
        self.labels.lock().unwrap().push(label.to_owned());
        if label.starts_with("Server URL") {
            return self.answer(&self.urls, validate);
        }
        assert_eq!(label, "Key ID:");
        self.answer(&self.key_ids, validate)
    }

    fn prompt_secret(&self, label: &str, validate: Validator) -> Result<SecretString, CliError> {
        assert_eq!(label, "Secret:");
        self.answer(&self.secrets, validate).map(SecretString::from)
    }

    fn ask(&self, question: &str) -> Result<bool, CliError> {
        self.questions.lock().unwrap().push(question.to_owned());
        Ok(self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .expect("a scripted answer"))
    }

    fn select(&self, label: &str, options: &[String], start: usize) -> Result<usize, CliError> {
        self.menus
            .lock()
            .unwrap()
            .push((label.to_owned(), options.to_vec()));
        self.starts.lock().unwrap().push(start);
        self.selections
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(CliError::Cancelled)
    }

    fn open_url(&self, url: &str) -> bool {
        self.opened.lock().unwrap().push(url.to_owned());
        true
    }

    fn write_output(&self, _value: &Value, _format: OutputFormat) -> Result<(), CliError> {
        Ok(())
    }

    fn write_bytes(&self, _bytes: &[u8]) -> Result<(), CliError> {
        Ok(())
    }

    fn progress(&self, _message: &str) -> Box<dyn Progress> {
        Box::new(NoProgress)
    }
}

/// A keyring that counts how often it is read.
#[derive(Default)]
struct CountingKeyring {
    reads: Arc<AtomicUsize>,
}

impl Backend for CountingKeyring {
    fn get(&self, _profile: &str) -> Result<Option<SecretString>, StoreError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(None)
    }

    fn set(&self, _profile: &str, _value: &SecretString) -> Result<(), StoreError> {
        Ok(())
    }

    fn remove(&self, _profile: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
}

/// A company the platform lists.
fn company_json(id: &str, name: Option<&str>, status: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "displayName": name,
        "createdAt": "2026-01-01T00:00:00Z",
        "isConsultingCompany": false,
        "isDemoCompany": false,
        "availableClientCompanies": null,
        "companyStatus": status
    })
}

/// A role the user holds, as the platform returns it.
fn role_json(company_id: &str, role: &str) -> Value {
    json!({
        "id": Uuid::new_v4(),
        "userId": USER_ID,
        "companyId": company_id,
        "userRole": role
    })
}

/// The one company a platform lists unless a test says otherwise.
fn acme() -> Value {
    company_json(COMPANY_ID, Some("Acme GmbH"), "ACTIVE")
}

/// A platform that accepts exactly one key, knows its user and lists
/// `companies`. Its identity endpoint resolves the company named by the
/// `X-CAD-Company-Id` header, or refuses it with a 403 when it is
/// `FORBIDDEN_COMPANY_ID`; without the header it resolves `COMPANY_ID`.
async fn platform_with(
    companies: Option<Vec<Value>>,
    roles: Vec<Value>,
    email_known: Arc<AtomicBool>,
) -> MockServer {
    let server = MockServer::start().await;
    let bearer = format!("Bearer {}", token());
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(move |request: &wiremock::Request| {
            let accepted = request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok())
                == Some(bearer.as_str());
            if !accepted {
                return ResponseTemplate::new(401).set_body_json(json!({"detail": "nope"}));
            }
            let requested = request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok());
            if requested == Some(FORBIDDEN_COMPANY_ID) {
                return ResponseTemplate::new(403).set_body_json(json!({"detail": "no access"}));
            }
            ResponseTemplate::new(200).set_body_json(json!({
                "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
                "companySelection": {
                    "resolvedCompanyId": requested.unwrap_or(COMPANY_ID),
                    "source": if requested.is_some() { "HEADER" } else { "DEFAULT" }
                }
            }))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_PATH))
        .respond_with(move |_: &wiremock::Request| {
            if email_known.load(Ordering::SeqCst) {
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id": USER_ID, "email": EMAIL, "firstName": "Jane"}))
            } else {
                ResponseTemplate::new(500)
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_ROLES_PATH))
        .and(query_param("userId_equals", USER_ID))
        .respond_with(ResponseTemplate::new(200).set_body_json(roles))
        .mount(&server)
        .await;
    if let Some(companies) = companies {
        Mock::given(method("GET"))
            .and(path(COMPANIES_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(companies))
            .mount(&server)
            .await;
    }
    server
}

struct World {
    server: MockServer,
    /// Whether the platform answers the request for the account's e-mail.
    email_known: Arc<AtomicBool>,
    origin: String,
    keyring: Arc<MemoryBackend>,
    store: CredentialStore,
    credentials_file: PathBuf,
    profiles: ProfileStore,
    profiles_file: PathBuf,
    _dir: tempfile::TempDir,
}

async fn world() -> World {
    world_with(MemoryBackend::default()).await
}

async fn world_with(keyring: MemoryBackend) -> World {
    world_fixture(keyring, Some(vec![acme()])).await
}

/// A world whose platform lists `companies`, or has no such endpoint.
async fn world_listing(companies: &[Value]) -> World {
    world_fixture(MemoryBackend::default(), Some(companies.to_vec())).await
}

/// A world whose platform lists `companies` and gives the user exactly `roles`.
async fn world_with_roles(companies: &[Value], roles: Vec<Value>) -> World {
    world_roles(MemoryBackend::default(), Some(companies.to_vec()), roles).await
}

/// A world where the user holds an `EXPERT` role in every company listed, so
/// that each of them is one the login can act on.
async fn world_fixture(keyring: MemoryBackend, companies: Option<Vec<Value>>) -> World {
    let roles = companies
        .iter()
        .flatten()
        .filter_map(|company| company["id"].as_str())
        .map(|id| role_json(id, "EXPERT"))
        .collect();
    world_roles(keyring, companies, roles).await
}

async fn world_roles(
    keyring: MemoryBackend,
    companies: Option<Vec<Value>>,
    roles: Vec<Value>,
) -> World {
    let email_known = Arc::new(AtomicBool::new(true));
    let server = platform_with(companies, roles, Arc::clone(&email_known)).await;
    let dir = tempfile::tempdir().unwrap();
    let credentials_file = dir.path().join("cad-cli").join("credentials.json");
    let profiles_file = dir.path().join("cad-cli").join("profiles.json");
    let keyring = Arc::new(keyring);
    let store = CredentialStore::new(
        Some(Box::new(Arc::clone(&keyring))),
        Some(FileBackend::new(credentials_file.clone())),
    );
    World {
        origin: server.uri(),
        email_known,
        server,
        keyring,
        store,
        credentials_file,
        profiles: ProfileStore::new(Some(profiles_file.clone())),
        profiles_file,
        _dir: dir,
    }
}

impl World {
    /// `cad-cli use <rest>`, with `--output json` unless `rest` says otherwise.
    async fn run(
        &self,
        rest: &[&str],
        terminal: &Scripted,
        environment: &Env,
    ) -> Result<Report, CliError> {
        self.run_store(rest, terminal, environment, &self.store)
            .await
    }

    async fn run_store(
        &self,
        rest: &[&str],
        terminal: &Scripted,
        environment: &Env,
        store: &CredentialStore,
    ) -> Result<Report, CliError> {
        let mut args = vec!["cad-cli"];
        if !rest.contains(&"--output") {
            args.extend(["--output", "json"]);
        }
        args.extend_from_slice(rest);
        let cli = Cli::try_parse_from(args).expect("arguments parse");
        let Command::Use(group) = cli.command else {
            panic!("expected a use command");
        };
        let context = CommandContext::new(terminal, cli.global.yes);
        group
            .run(&context, &cli.global, environment, store, &self.profiles)
            .await
    }

    /// `use profile <profile> --url <this server> --allow-insecure-http <more>`.
    async fn profile(
        &self,
        profile: &str,
        more: &[&str],
        terminal: &Scripted,
        environment: &Env,
    ) -> Result<Report, CliError> {
        let mut rest = vec![
            "use",
            "profile",
            profile,
            "--url",
            &self.origin,
            "--allow-insecure-http",
        ];
        rest.extend_from_slice(more);
        self.run(&rest, terminal, environment).await
    }

    fn set(&self) -> ProfileSet {
        self.profiles.load().unwrap()
    }

    fn stored(&self) -> bool {
        self.keyring.text("local").is_some() || self.credentials_file.exists()
    }

    async fn requests(&self) -> usize {
        self.server.received_requests().await.unwrap().len()
    }
}

fn exit_code(error: &CliError) -> ExitCode {
    error.exit_code()
}

/// Everything a report shows the user, as one string.
fn shown(report: &Report) -> String {
    let messages: Vec<_> = report
        .messages
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    format!("{} {} {report:?}", report.value, messages.join("\n"))
}

fn page(origin: &str) -> String {
    format!("{origin}/app/user-account-settings/api-keys")
}

// --- parsing -----------------------------------------------------------------

#[test]
fn use_takes_no_arguments_and_profile_takes_a_name_and_its_flags() {
    for rest in [
        vec!["use"],
        vec!["use", "profile", "staging"],
        vec!["use", "profile", "staging", "--login"],
        vec!["use", "profile", "staging", "--url", "https://x.test"],
        vec![
            "use",
            "profile",
            "staging",
            "--with-token",
            "--insecure-storage",
        ],
        vec!["use", "profile", "staging", "--login", "--no-browser"],
        vec!["use", "profile", "staging", "--forget"],
    ] {
        let mut args = vec!["cad-cli"];
        args.extend(rest);
        Cli::try_parse_from(args).expect("parses");
    }
    Cli::try_parse_from(["cad-cli", "use", "profile"]).expect_err("a name is required");
    Cli::try_parse_from(["cad-cli", "use", "staging"]).expect_err("`staging` is not a command");
    Cli::try_parse_from(["cad-cli", "use", "--login"]).expect_err("`use` has no such flag");
}

#[test]
fn forget_conflicts_with_every_other_flag() {
    for flag in [
        vec!["--url", "https://x.test"],
        vec!["--login"],
        vec!["--with-token"],
        vec!["--no-browser"],
        vec!["--insecure-storage"],
        vec!["--allow-insecure-http"],
    ] {
        let mut args = vec!["cad-cli", "use", "profile", "staging", "--forget"];
        args.extend(flag.iter().copied());
        let error = Cli::try_parse_from(args).expect_err("--forget conflicts");
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::ArgumentConflict,
            "{flag:?}"
        );
    }
}

#[test]
fn use_is_neither_offline_nor_an_online_platform_command() {
    for rest in [vec!["use"], vec!["use", "profile", "staging"]] {
        let mut args = vec!["cad-cli"];
        args.extend(rest);
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(matches!(
            cli.command.into_invocation(),
            crate::cmds::Invocation::Use(_)
        ));
    }
}

#[tokio::test]
async fn use_profile_refuses_the_global_profile_and_server_options() {
    let world = world().await;
    for (rest, expected) in [
        (
            vec![
                "--profile",
                "staging",
                "use",
                "profile",
                "dev",
                "--url",
                "https://x.test",
            ],
            "--profile does not apply to `cad-cli use profile`; name the profile as its argument",
        ),
        (
            vec!["--server-url", "https://x.test", "use", "profile", "dev"],
            "--server-url does not apply to `cad-cli use profile`; use --url",
        ),
    ] {
        let error = world
            .run(&rest, &Scripted::default(), &Env::default())
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::Input(_)));
        assert_eq!(exit_code(&error), ExitCode::from(2));
        assert!(error.to_string().contains(expected), "{error}");
    }
    assert!(!world.profiles_file.exists());
}

// --- creating, switching and logging in --------------------------------------

#[tokio::test]
async fn a_new_profile_with_a_token_stores_the_login_the_email_and_the_active_profile() {
    let world = world().await;
    let terminal = Scripted::piped(&format!("{}\n", token()));
    let report = world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    let file = world.credentials_file.display().to_string();
    assert_eq!(report.value["keyId"].as_str(), Some(KEY_ID));
    assert_eq!(report.value["previous"], Value::Null);
    let row = &report.value;
    assert_eq!(row["name"], "local");
    assert_eq!(row["url"], world.origin.as_str());
    assert_eq!(row["active"], true);
    assert_eq!(row["selected"], true);
    assert_eq!(row["userId"], USER_ID);
    assert_eq!(row["email"], EMAIL);
    assert_eq!(row["storage"], "file");
    assert!(row["storedAt"].as_str().unwrap().ends_with('Z'));

    assert_eq!(
        report.messages,
        [
            (
                StatusKind::Success,
                format!("logged in to {} as {EMAIL}, stored in {file}", world.origin)
            ),
            (
                StatusKind::Success,
                "using company Acme GmbH for profile local".to_owned()
            ),
            (
                StatusKind::Success,
                format!("now using local ({}) as {EMAIL}", world.origin)
            ),
        ]
    );
    assert_eq!(row["companyId"], COMPANY_ID);
    assert_eq!(row["companyName"], "Acme GmbH");

    // profiles.json holds the metadata and no secret.
    let text = std::fs::read_to_string(&world.profiles_file).unwrap();
    assert!(!text.contains(SECRET), "{text}");
    let document: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(document["version"], 1);
    assert_eq!(document["active"], "local");
    assert_eq!(document["profiles"]["local"]["url"], world.origin.as_str());
    assert_eq!(document["profiles"]["local"]["allowInsecureHttp"], true);
    assert_eq!(document["profiles"]["local"]["login"]["email"], EMAIL);
    assert_eq!(
        document["profiles"]["local"]["company"],
        json!({"id": COMPANY_ID, "name": "Acme GmbH"})
    );

    // The secret is filed under the profile's name, with its origin.
    assert!(
        world.keyring.text("local").is_none(),
        "the keyring was skipped"
    );
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(&world.credentials_file).unwrap()).unwrap();
    let entry = &stored["credentials"]["local"];
    assert_eq!(entry["secret"], SECRET);
    assert_eq!(entry["origin"], world.origin.as_str());
    assert_eq!(entry["userId"], USER_ID);
    assert!(!shown(&report).contains(SECRET));
    assert_eq!(
        world.requests().await,
        4,
        "the identity, the user, the companies and the roles"
    );
}

#[tokio::test]
async fn login_prefers_the_keyring_and_files_the_entry_under_the_profile() {
    let world = world().await;
    let report = world
        .profile(
            "local",
            &["--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(report.value["storage"], "keyring");
    assert_eq!(
        report.messages[0].1,
        format!(
            "logged in to {} as {EMAIL}, stored in the system keyring",
            world.origin
        )
    );
    let stored: Value = serde_json::from_str(&world.keyring.text("local").unwrap()).unwrap();
    assert_eq!(stored["kind"], "apiKey");
    assert_eq!(stored["keyId"], KEY_ID);
    assert_eq!(stored["origin"], world.origin.as_str());
    assert!(!world.credentials_file.exists());
    assert!(!shown(&report).contains(SECRET));
}

#[tokio::test]
async fn a_rejected_key_exits_three_and_stores_nothing_not_even_the_profile() {
    let world = world().await;
    let other = token().replace("AbCdEf", "ZzZzZz");
    let error = world
        .profile(
            "local",
            &["--with-token"],
            &Scripted::piped(&other),
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Authentication { .. }), "{error}");
    assert_eq!(exit_code(&error), ExitCode::from(3));
    assert!(!world.stored());
    assert!(
        !world.profiles_file.exists(),
        "a failed login creates no profile"
    );
    assert!(!error.to_string().contains("ZzZzZz"));
}

#[tokio::test]
async fn a_network_error_stores_nothing() {
    let world = world().await;
    let error = world
        .run(
            &[
                "use",
                "profile",
                "local",
                "--url",
                "http://127.0.0.1:1",
                "--allow-insecure-http",
                "--with-token",
                "--insecure-storage",
            ],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Transport { .. }), "{error}");
    assert_eq!(exit_code(&error), ExitCode::from(5));
    assert!(!world.credentials_file.exists());
    assert!(!world.profiles_file.exists());
}

#[tokio::test]
async fn malformed_tokens_are_refused_before_any_request() {
    let world = world().await;
    for input in [
        "",
        "\n",
        KEY_ID,
        SECRET,
        &format!("not-a-uuid.{SECRET}"),
        &format!("{KEY_ID}.cad_aks_tooshort"),
        &format!("{KEY_ID}.{SECRET}extra"),
    ] {
        let error = world
            .profile(
                "local",
                &["--with-token"],
                &Scripted::piped(input),
                &Env::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::Input(_)), "{input:?}: {error}");
        assert_eq!(exit_code(&error), ExitCode::from(2));
        assert!(!error.to_string().contains(SECRET));
    }
    assert_eq!(world.requests().await, 0);
    assert!(!world.stored());
}

#[tokio::test]
async fn a_failing_users_request_still_logs_in_with_no_email() {
    let world = world().await;
    // The more specific mock goes first: wiremock answers with the first match.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
            "companySelection": {"resolvedCompanyId": COMPANY_ID, "source": "DEFAULT"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"detail": "boom"})))
        .mount(&server)
        .await;
    let origin = server.uri();
    let report = world
        .run(
            &[
                "use",
                "profile",
                "local",
                "--url",
                &origin,
                "--allow-insecure-http",
                "--with-token",
            ],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();

    let login = &report.value;
    assert_eq!(login["email"], Value::Null);
    assert_eq!(login["userId"], USER_ID);
    assert_eq!(report.messages[0].0, StatusKind::Success);
    assert!(report.messages[0].1.contains(&format!("as {USER_ID}")));
    assert_eq!(report.messages[1].0, StatusKind::Warning);
    assert!(report.messages[1].1.contains("could not read the e-mail"));
    assert_eq!(
        world
            .set()
            .get(&name("local"))
            .unwrap()
            .login
            .as_ref()
            .unwrap()
            .email,
        None
    );
    assert!(world.keyring.text("local").is_some(), "the login is stored");
}

#[tokio::test]
async fn without_a_terminal_or_a_token_login_names_the_flag() {
    let world = world().await;
    let terminal = Scripted::default();
    let error = world
        .profile("local", &["--login"], &terminal, &Env::default())
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("--with-token"), "{error}");
    assert_eq!(world.requests().await, 0);
    assert!(!world.profiles_file.exists());
    // The page is still printed, so the key can be made and piped in later.
    assert!(terminal.statuses()[0].1.contains(&page(&world.origin)));
}

#[tokio::test]
async fn a_keyring_that_fails_exits_two_and_names_the_way_out_without_a_terminal() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;
    for more in [
        vec!["--with-token"],
        // `--yes` does not answer the question: nothing may be written.
        vec!["--with-token", "--yes"],
    ] {
        let terminal = Scripted::piped(&token());
        let error = world
            .profile("local", &more, &terminal, &Env::default())
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::AuthConfig { .. }));
        assert_eq!(exit_code(&error), ExitCode::from(2));
        let message = error.to_string();
        assert!(message.contains(KEYRING_DOWN), "{message}");
        assert!(message.contains("--insecure-storage"), "{message}");
        assert!(message.contains("CAD_API_KEY"), "{message}");
        assert!(
            message.contains(&world.credentials_file.display().to_string()),
            "{message}"
        );
        assert!(!message.contains(SECRET));
        assert!(!world.credentials_file.exists());
        assert!(!world.profiles_file.exists());
        assert_eq!(terminal.questions(), Vec::<String>::new());
    }
}

#[tokio::test]
async fn a_keyring_turned_off_behaves_like_one_that_fails() {
    let world = world().await;
    let store = CredentialStore::new(None, Some(FileBackend::new(world.credentials_file.clone())));
    let error = world
        .run_store(
            &[
                "use",
                "profile",
                "local",
                "--url",
                &world.origin,
                "--allow-insecure-http",
                "--with-token",
            ],
            &Scripted::piped(&token()),
            &Env::default(),
            &store,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("CAD_CLI_KEYRING"), "{error}");
    assert!(error.to_string().contains("--insecure-storage"), "{error}");
    assert!(!world.credentials_file.exists());
}

#[tokio::test]
async fn at_a_terminal_a_failed_keyring_offers_the_file_and_only_yes_writes_it() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;

    let declined = Scripted::typing(&[KEY_ID], &[SECRET], &[false]);
    let error = world
        .profile(
            "local",
            &["--login", "--no-browser"],
            &declined,
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("CAD_API_KEY"), "{error}");
    assert!(!world.stored());
    assert!(!world.profiles_file.exists());
    let file = world.credentials_file.display().to_string();
    assert_eq!(
        declined.questions(),
        [format!(
            "Store the credentials in {file} instead, readable only by you?"
        )]
    );
    assert!(
        declined
            .statuses()
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Warning && text.contains(KEYRING_DOWN))
    );

    // `--yes` is not an answer: the question is asked all the same.
    let accepted = Scripted::typing(&[KEY_ID], &[SECRET], &[true]);
    let report = world
        .run(
            &[
                "--yes",
                "use",
                "profile",
                "local",
                "--url",
                &world.origin,
                "--allow-insecure-http",
                "--login",
                "--no-browser",
            ],
            &accepted,
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.questions().len(), 1);
    assert_eq!(report.value["storage"], "file");
    assert!(world.credentials_file.exists());
}

#[tokio::test]
async fn at_a_terminal_the_two_values_are_asked_apart_and_asked_again_when_malformed() {
    let world = world().await;
    let terminal = Scripted::typing(
        &["not-a-uuid", &format!(" {KEY_ID} ")],
        &["nope", "cad_aks_short", SECRET],
        &[],
    );
    let report = world
        .profile("local", &["--login"], &terminal, &Env::default())
        .await
        .unwrap();

    assert_eq!(report.value["storage"], "keyring");
    let statuses = terminal.statuses();
    let warnings: Vec<_> = statuses
        .iter()
        .filter(|(kind, _)| *kind == StatusKind::Warning)
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(warnings.len(), 3, "{warnings:?}");
    assert!(warnings[0].contains("Key ID"));
    assert!(warnings[1].contains("Secret") && warnings[2].contains("Secret"));
    assert!(
        warnings
            .iter()
            .all(|text| !text.contains(SECRET) && !text.contains("short"))
    );
    // The page is printed first and opened in the browser.
    assert!(statuses[0].1.contains(&page(&world.origin)));
    assert_eq!(terminal.opened(), [page(&world.origin)]);
}

#[tokio::test]
async fn a_value_that_stays_malformed_is_asked_again_until_the_person_cancels() {
    let world = world().await;
    let terminal = Scripted::typing(&["a", "b", "c", "d"], &[], &[]);
    let error = world
        .profile(
            "local",
            &["--login", "--no-browser"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Cancelled));
    assert_eq!(exit_code(&error), ExitCode::from(130));
    assert_eq!(
        terminal
            .statuses()
            .iter()
            .filter(|(kind, _)| *kind == StatusKind::Warning)
            .count(),
        4
    );
    assert_eq!(world.requests().await, 0);
    assert!(!world.profiles_file.exists());
}

#[tokio::test]
async fn the_browser_opens_only_when_it_can_help() {
    let world = world().await;
    let keyed = || Scripted::typing(&[KEY_ID], &[SECRET], &[]);

    let terminal = keyed();
    world
        .profile("local", &["--login"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(terminal.opened(), [page(&world.origin)]);

    let terminal = keyed();
    world
        .profile(
            "local",
            &["--login", "--no-browser"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(terminal.opened(), Vec::<String>::new());

    let terminal = Scripted::piped(&token());
    world
        .profile("local", &["--with-token"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(terminal.opened(), Vec::<String>::new());
}

#[tokio::test]
async fn login_warns_when_the_environment_would_still_win() {
    let world = world().await;
    for environment in [
        Env::default().with("CAD_API_KEY", KEY_ID),
        Env::default().with("CAD_API_SECRET", "x"),
    ] {
        let report = world
            .profile(
                "local",
                &["--with-token"],
                &Scripted::piped(&token()),
                &environment,
            )
            .await
            .unwrap();
        let warnings: Vec<_> = report
            .messages
            .iter()
            .filter(|(kind, _)| *kind == StatusKind::Warning)
            .collect();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains("CAD_API_KEY"));
    }
    let report = world
        .profile(
            "local",
            &["--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(
        report
            .messages
            .iter()
            .all(|(kind, _)| *kind != StatusKind::Warning)
    );
}

#[tokio::test]
async fn logging_in_again_replaces_the_other_place() {
    let world = world().await;
    let piped = || Scripted::piped(&token());
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &piped(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(world.credentials_file.exists());
    let report = world
        .profile("local", &["--with-token"], &piped(), &Env::default())
        .await
        .unwrap();
    assert!(
        !world.credentials_file.exists(),
        "the file's entry moved to the keyring"
    );
    assert!(world.keyring.text("local").is_some());
    assert_eq!(world.set().iter().count(), 1);
    assert_eq!(report.value["storage"], "keyring");
}

// --- the server of a new profile ---------------------------------------------

#[tokio::test]
async fn a_known_name_without_a_terminal_or_a_url_takes_its_suggestion_and_says_so() {
    let world = world().await;
    for (profile, url) in [
        ("production", "https://app.carbonaltdelete.eu"),
        ("staging", "https://staging.carbonaltdelete.eu"),
        ("dev", "https://dev.carbonaltdelete.eu"),
    ] {
        let terminal = Scripted::default();
        let report = world
            .run(&["use", "profile", profile], &terminal, &Env::default())
            .await
            .unwrap();
        assert_eq!(report.value["url"], url);
        assert_eq!(world.set().get(&name(profile)).unwrap().url, url);
        assert!(terminal.statuses().contains(&(
            StatusKind::Info,
            format!("using {url} for profile {profile}")
        )));
        // No login was asked for, so no request was made and the profile says so.
        assert_eq!(report.value["keyId"], Value::Null);
        assert!(
            report
                .messages
                .iter()
                .any(|(kind, text)| *kind == StatusKind::Warning
                    && text.contains(&format!("cad-cli use profile {profile} --with-token"))
                    && text.contains("CAD_API_KEY"))
        );
        assert!(
            report.messages.last().unwrap().1.contains("with no login"),
            "{:?}",
            report.messages
        );
    }
    assert_eq!(world.set().active(), Some(&name("dev")));
}

#[tokio::test]
async fn another_name_without_a_terminal_or_a_url_fails_naming_the_flag() {
    let world = world().await;
    let error = world
        .run(
            &["use", "profile", "acme"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("--url"), "{error}");
    assert!(!world.profiles_file.exists());
}

#[tokio::test]
async fn at_a_terminal_a_known_name_suggests_its_url_and_enter_accepts_it() {
    let world = world().await;
    let terminal = Scripted::at_a_terminal(&[""], &[], &[], &[false]);
    let report = world
        .run(&["use", "profile", "staging"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(
        terminal.labels(),
        ["Server URL (Enter for https://staging.carbonaltdelete.eu):"]
    );
    assert_eq!(report.value["url"], "https://staging.carbonaltdelete.eu");
    // The login is offered, and declining it still switches.
    assert_eq!(
        terminal.questions(),
        ["No login for profile staging (https://staging.carbonaltdelete.eu). Log in now?"]
    );
    assert_eq!(report.value["keyId"], Value::Null);
    assert!(
        report
            .messages
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Warning && text.contains("no login"))
    );
    assert_eq!(world.set().active(), Some(&name("staging")));
}

#[tokio::test]
async fn at_a_terminal_another_name_asks_without_a_suggestion_until_the_url_is_valid() {
    let world = world().await;
    let terminal = Scripted::at_a_terminal(
        &[
            "",
            "not a url",
            "http://example.test",
            "https://acme.example.test/some/path",
        ],
        &[],
        &[],
        &[false],
    );
    let report = world
        .run(&["use", "profile", "acme"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(terminal.labels(), ["Server URL:"]);
    // The path is dropped: a profile is a server, kept as its origin.
    assert_eq!(report.value["url"], "https://acme.example.test");
    let warnings = terminal
        .statuses()
        .into_iter()
        .filter(|(kind, _)| *kind == StatusKind::Warning)
        .count();
    assert_eq!(warnings, 3, "an empty, a malformed and a plain-HTTP answer");
}

#[tokio::test]
async fn a_new_profile_at_a_terminal_that_wants_a_login_logs_in_when_the_question_is_answered() {
    let world = world().await;
    let terminal = Scripted::typing(&[KEY_ID], &[SECRET], &[true]);
    let report = world
        .profile("local", &["--no-browser"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(
        terminal.questions(),
        [format!(
            "No login for profile local ({}). Log in now?",
            world.origin
        )]
    );
    assert_eq!(report.value["email"], EMAIL);
}

#[tokio::test]
async fn a_url_is_validated_and_plain_http_needs_the_flag_and_is_remembered() {
    let world = world().await;
    let error = world
        .run(
            &["use", "profile", "local", "--url", &world.origin],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("--allow-insecure-http"),
        "{error}"
    );

    for url in ["not a url", "http://example.test", "ftp://example.test"] {
        world
            .run(
                &[
                    "use",
                    "profile",
                    "local",
                    "--url",
                    url,
                    "--allow-insecure-http",
                ],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .expect_err(url);
    }
    assert!(!world.profiles_file.exists());

    world
        .profile("local", &[], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    let profile = world.set().get(&name("local")).unwrap().clone();
    assert!(profile.allow_insecure_http);
    assert_eq!(profile.url, world.origin);
}

// --- switching ---------------------------------------------------------------

#[tokio::test]
async fn switching_to_a_logged_in_profile_makes_no_request_and_does_not_read_the_keyring() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(world.set().active(), Some(&name("staging")));
    let before = world.requests().await;

    let reads = Arc::new(AtomicUsize::new(0));
    let counting = CredentialStore::new(
        Some(Box::new(CountingKeyring {
            reads: Arc::clone(&reads),
        })),
        Some(FileBackend::new(world.credentials_file.clone())),
    );
    let terminal = Scripted::default();
    let report = world
        .run_store(
            &["use", "profile", "local"],
            &terminal,
            &Env::default(),
            &counting,
        )
        .await
        .unwrap();

    assert_eq!(world.requests().await, before, "no request");
    assert_eq!(reads.load(Ordering::SeqCst), 0, "the keyring was not read");
    assert_eq!(terminal.questions(), Vec::<String>::new());
    assert_eq!(world.set().active(), Some(&name("local")));
    assert_eq!(report.value["previous"], "staging");
    assert_eq!(report.value["email"], EMAIL);
    assert_eq!(
        report.messages.last().unwrap().1,
        format!(
            "now using local ({}) as {EMAIL} (was staging)",
            world.origin
        )
    );
}

#[tokio::test]
async fn an_existing_profile_keeps_its_server_and_a_different_one_is_refused() {
    let world = world().await;
    world
        .profile("local", &[], &Scripted::default(), &Env::default())
        .await
        .unwrap();

    // The same origin, written differently, is fine.
    world
        .run(
            &[
                "use",
                "profile",
                "local",
                "--url",
                &format!("{}/some/path", world.origin),
                "--allow-insecure-http",
            ],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    let error = world
        .run(
            &[
                "use",
                "profile",
                "local",
                "--url",
                "https://elsewhere.example.test",
            ],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        format!(
            "invalid input: profile local is for {}; forget it first with `cad-cli use profile \
             local --forget`, or choose another name",
            world.origin
        )
    );
    assert_eq!(world.set().get(&name("local")).unwrap().url, world.origin);
}

#[tokio::test]
async fn allow_insecure_http_is_remembered_for_an_existing_profile() {
    let world = world().await;
    let file = world.profiles_file.clone();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        json!({
            "version": 1,
            "profiles": {"local": {"url": world.origin, "allowInsecureHttp": false}}
        })
        .to_string(),
    )
    .unwrap();
    world
        .run(
            &["use", "profile", "local", "--allow-insecure-http"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(world.set().get(&name("local")).unwrap().allow_insecure_http);
}

#[tokio::test]
async fn cad_profile_overrides_the_switch_in_this_shell_and_says_so() {
    let world = world().await;
    world
        .profile("local", &[], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    let report = world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default().with("CAD_PROFILE", "local"),
        )
        .await
        .unwrap();
    assert_eq!(report.value["active"], true);
    assert_eq!(report.value["selected"], false);
    let last = report.messages.last().unwrap();
    assert_eq!(last.0, StatusKind::Warning);
    assert!(last.1.contains("CAD_PROFILE is set to local"), "{last:?}");
}

#[tokio::test]
async fn a_damaged_profiles_file_is_an_error_naming_it_and_is_never_overwritten() {
    let world = world().await;
    std::fs::create_dir_all(world.profiles_file.parent().unwrap()).unwrap();
    std::fs::write(&world.profiles_file, "not json").unwrap();
    for rest in [vec!["use"], vec!["use", "profile", "staging"]] {
        let error = world
            .run(&rest, &Scripted::default(), &Env::default())
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&world.profiles_file.display().to_string()),
            "{error}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(&world.profiles_file).unwrap(),
        "not json"
    );
}

// --- listing -----------------------------------------------------------------

#[tokio::test]
async fn without_profiles_use_lists_nothing_and_says_how_to_start() {
    let world = world().await;
    let report = world
        .run(&["use"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(report.value, json!([]));
    assert_eq!(
        report.messages,
        [(
            StatusKind::Warning,
            "no profile yet; run `cad-cli use profile <name>`".to_owned()
        )]
    );
    assert!(!world.profiles_file.exists(), "listing writes nothing");
}

#[tokio::test]
async fn use_lists_the_profiles_sorted_with_their_login_metadata_and_never_connects() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    world
        .run(
            &["use", "profile", "dev"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let before = world.requests().await;

    let report = world
        .run(&["use"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(world.requests().await, before);
    let rows = report.value.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "dev");
    assert_eq!(rows[1]["name"], "local");
    assert_eq!(rows[0]["active"], true);
    assert_eq!(rows[0]["selected"], true);
    for field in [
        "email",
        "userId",
        "keyId",
        "storage",
        "storedAt",
        "companyId",
        "companyName",
    ] {
        assert_eq!(
            rows[0][field],
            Value::Null,
            "a profile with no login: {field}"
        );
    }
    assert_eq!(rows[1]["active"], false);
    assert_eq!(rows[1]["selected"], false);
    let mut keys: Vec<_> = rows[1]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "active",
            "companyId",
            "companyName",
            "email",
            "keyId",
            "name",
            "selected",
            "storage",
            "storedAt",
            "url",
            "userId"
        ]
    );
    assert_eq!(rows[1]["companyId"], COMPANY_ID);
    assert_eq!(rows[1]["companyName"], "Acme GmbH");
    assert_eq!(rows[1]["keyId"], KEY_ID);
    assert_eq!(rows[1]["userId"], USER_ID);
    assert_eq!(rows[1]["email"], EMAIL);
    assert_eq!(rows[1]["storage"], "file");
    assert!(rows[1]["storedAt"].as_str().unwrap().ends_with('Z'));
    assert!(!shown(&report).contains(SECRET));
    assert_eq!(
        report.messages,
        [(
            StatusKind::Info,
            "using profile dev (the active profile)".to_owned()
        )]
    );
}

#[tokio::test]
async fn selected_follows_what_a_command_would_use_and_differs_from_active() {
    let world = world().await;
    for profile in ["staging", "dev"] {
        world
            .run(
                &["use", "profile", profile],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .unwrap();
    }
    let selected = |report: &Report| -> Vec<(String, bool, bool)> {
        report
            .value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["name"].as_str().unwrap().to_owned(),
                    row["active"].as_bool().unwrap(),
                    row["selected"].as_bool().unwrap(),
                )
            })
            .collect()
    };

    let report = world
        .run(
            &["use"],
            &Scripted::default(),
            &Env::default().with("CAD_PROFILE", "staging"),
        )
        .await
        .unwrap();
    assert_eq!(
        selected(&report),
        [("dev".into(), true, false), ("staging".into(), false, true)]
    );
    assert_eq!(
        report.messages[0].1,
        "using profile staging (selected by CAD_PROFILE)"
    );

    let report = world
        .run(
            &["--profile", "staging", "use"],
            &Scripted::default(),
            &Env::default().with("CAD_PROFILE", "dev"),
        )
        .await
        .unwrap();
    assert_eq!(
        selected(&report),
        [("dev".into(), true, false), ("staging".into(), false, true)]
    );
    assert_eq!(
        report.messages[0].1,
        "using profile staging (selected by --profile)"
    );

    let report = world
        .run(
            &["use"],
            &Scripted::default(),
            &Env::default().with("CAD_SERVER_URL", "https://staging.carbonaltdelete.eu/x"),
        )
        .await
        .unwrap();
    assert_eq!(
        selected(&report),
        [("dev".into(), true, false), ("staging".into(), false, true)]
    );
    assert_eq!(
        report.messages[0].1,
        "using profile staging (selected by CAD_SERVER_URL)"
    );
}

#[tokio::test]
async fn a_server_no_profile_has_selects_none_and_an_unknown_profile_is_an_error() {
    let world = world().await;
    for profile in ["staging", "dev"] {
        world
            .run(
                &["use", "profile", profile],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .unwrap();
    }
    let report = world
        .run(
            &["--server-url", "https://other.example.test", "use"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let flags: Vec<_> = report
        .value
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["active"].as_bool(), row["selected"].as_bool()))
        .collect();
    assert_eq!(
        flags,
        [(Some(true), Some(false)), (Some(false), Some(false))]
    );
    assert!(report.messages[0].1.contains("no profile is selected"));

    let error = world
        .run(
            &["--profile", "nope", "use"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("there is no profile nope"),
        "{error}"
    );
}

#[tokio::test]
async fn the_rows_are_flat_with_the_account_in_plain_columns_for_every_format() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    // The same flat shape whatever the output format asks for.
    for output in ["table", "json", "jsonl"] {
        let report = world
            .run(
                &["--output", output, "use"],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .unwrap();
        assert_eq!(report.value[0]["email"], EMAIL, "{output}");
        assert!(
            report.value[0]
                .as_object()
                .unwrap()
                .values()
                .all(|value| !value.is_object() && !value.is_array()),
            "{output}: {}",
            report.value
        );
    }

    // Without an e-mail the user ID is still a column of its own.
    let mut set = world.set();
    let mut profile = set.get(&name("local")).unwrap().clone();
    profile.login.as_mut().unwrap().email = None;
    set.insert(name("local"), profile);
    world.profiles.save(&set).unwrap();
    let report = world
        .run(&["use"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(report.value[0]["email"], Value::Null);
    assert_eq!(report.value[0]["userId"], USER_ID);

    // The switch is the row with `previous` beside it, not a nested object.
    let report = world
        .run(
            &["use", "profile", "local"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.value["name"], "local");
    assert_eq!(report.value["userId"], USER_ID);
    assert_eq!(report.value["previous"], "local");
    assert!(
        report
            .messages
            .last()
            .unwrap()
            .1
            .contains(&format!("as {USER_ID}"))
    );
}

// --- forgetting --------------------------------------------------------------

#[tokio::test]
async fn forget_removes_the_login_from_both_places_and_the_profile_without_connecting() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    world
        .keyring
        .put("local", &json!({"kind": "apiKey"}).to_string());
    let before = world.requests().await;

    let report = world
        .run(
            &["use", "profile", "local", "--forget"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        report.value,
        json!({"profile": "local", "removed": ["keyring", "file"], "wasActive": true})
    );
    assert_eq!(report.messages[0].0, StatusKind::Success);
    let text = &report.messages[0].1;
    assert!(text.contains("stays valid until it is revoked"), "{text}");
    assert!(text.contains(&page(&world.origin)), "{text}");
    assert!(
        report
            .messages
            .iter()
            .any(|(_, text)| text.contains("no profile is active now"))
    );
    assert!(!world.stored());
    let set = world.set();
    assert!(set.is_empty());
    assert_eq!(set.active(), None);
    assert_eq!(world.requests().await, before, "forget never connects");
}

#[tokio::test]
async fn forgetting_a_profile_with_no_stored_login_succeeds_and_one_that_is_unknown_fails() {
    let world = world().await;
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let report = world
        .run(
            &["use", "profile", "staging", "--forget"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        report.value,
        json!({"profile": "staging", "removed": [], "wasActive": true})
    );
    assert!(report.messages[0].1.contains("it had no stored login"));

    let error = world
        .run(
            &["use", "profile", "staging", "--forget"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert!(
        error
            .to_string()
            .contains("there is no profile staging to forget"),
        "{error}"
    );
}

#[tokio::test]
async fn forget_leaves_other_profiles_alone_and_warns_about_the_environment() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    // Another profile's entry in the same file.
    world
        .store
        .save_in_file(
            &name("staging"),
            &crate::support::credentials::StoredCredential::api_key(
                "https://staging.carbonaltdelete.eu".to_owned(),
                Uuid::from_u128(9),
                SecretString::from(SECRET),
                Uuid::from_u128(10).into(),
                chrono::Utc::now(),
            ),
        )
        .unwrap();

    let report = world
        .run(
            &["use", "profile", "local", "--forget"],
            &Scripted::default(),
            &Env::default().with("CAD_API_SECRET", "x"),
        )
        .await
        .unwrap();
    assert_eq!(report.value["removed"], json!(["file"]));
    assert_eq!(report.value["wasActive"], false);
    let last = report.messages.last().unwrap();
    assert_eq!(last.0, StatusKind::Warning);
    assert!(last.1.contains("CAD_API_KEY"));
    assert!(
        world
            .store
            .load(&name("staging"))
            .unwrap()
            .credential
            .is_some()
    );
    let set = world.set();
    assert!(set.get(&name("local")).is_none());
    assert!(set.get(&name("staging")).is_some());
    assert_eq!(set.active(), Some(&name("staging")));
}

#[tokio::test]
async fn forget_with_an_unreachable_keyring_says_so() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let report = world
        .run(
            &["use", "profile", "staging", "--forget"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(
        report
            .messages
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Warning && text.contains(KEYRING_DOWN))
    );
}

// --- the company ---------------------------------------------------------------

fn other_company() -> Value {
    company_json(OTHER_COMPANY_ID, Some("Other AG"), "ACTIVE")
}

/// A world with two companies, and a profile `local` logged in on it without
/// a company: with several to choose from and no terminal, the login leaves it
/// unset.
async fn world_with_two_companies() -> World {
    let world = world_listing(&[acme(), other_company()]).await;
    login_local(&world).await;
    assert_eq!(world.set().get(&name("local")).unwrap().company, None);
    world
}

async fn login_local(world: &World) -> Report {
    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap()
}

fn company_of(world: &World, profile: &str) -> Option<(String, Option<String>)> {
    world
        .set()
        .get(&name(profile))
        .unwrap()
        .company
        .as_ref()
        .map(|company| (company.id.to_string(), company.name.clone()))
}

fn chosen(id: &str, company_name: Option<&str>) -> (String, Option<String>) {
    (id.to_owned(), company_name.map(str::to_owned))
}

const NO_COMPANY_HINT: &str = "no company is selected; run `cad-cli use company <name|id>`";

#[test]
fn use_company_and_use_check_parse_and_the_old_context_command_is_gone() {
    for rest in [
        vec!["use", "company", "Acme GmbH"],
        vec!["use", "company", "00000000-0000-0000-0000-0000000000cc"],
        vec!["--profile", "staging", "use", "company", "Acme"],
        vec!["use", "--check"],
        vec![
            "--company-id",
            COMPANY_ID,
            "--output",
            "json",
            "use",
            "--check",
        ],
    ] {
        let mut args = vec!["cad-cli"];
        args.extend(rest);
        Cli::try_parse_from(args).expect("parses");
    }
    Cli::try_parse_from(["cad-cli", "use", "company"]).expect_err("a company is required");
    Cli::try_parse_from(["cad-cli", "use", "company", "  "]).expect_err("blank is no company");
    Cli::try_parse_from(["cad-cli", "use", "--check", "company", "Acme"])
        .expect_err("--check stands alone");
    Cli::try_parse_from(["cad-cli", "use", "--check", "profile", "dev"])
        .expect_err("--check stands alone");
    Cli::try_parse_from(["cad-cli", "context", "show"]).expect_err("`context` is gone");
}

#[test]
fn the_argument_is_an_id_when_it_is_one_and_a_name_otherwise() {
    use super::company::CompanyChoice;

    assert_eq!(
        format!(" {COMPANY_ID} ").parse::<CompanyChoice>(),
        Ok(CompanyChoice::Id(COMPANY_ID.parse().unwrap()))
    );
    assert_eq!(
        " Acme GmbH ".parse::<CompanyChoice>(),
        Ok(CompanyChoice::Name("Acme GmbH".to_owned()))
    );
    assert!("  ".parse::<CompanyChoice>().is_err());
}

#[tokio::test]
async fn a_company_is_chosen_by_id_and_saved_on_the_profile() {
    let world = world_with_two_companies().await;
    let before = world.requests().await;

    let report = world
        .run(
            &["use", "company", OTHER_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        report.value,
        json!({
            "profile": "local",
            "companyId": OTHER_COMPANY_ID,
            "companyName": "Other AG",
            "previousCompanyId": null,
            "previousCompanyName": null
        })
    );
    assert_eq!(
        report.messages,
        [(
            StatusKind::Success,
            format!(
                "Using company Other AG on local ({}) as {EMAIL}",
                world.origin
            )
        )]
    );
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(OTHER_COMPANY_ID, Some("Other AG")))
    );
    // The key, the companies, the roles, and access to the one chosen.
    assert_eq!(world.requests().await, before + 4);
    let requests = world.server.received_requests().await.unwrap();
    let last = requests.last().unwrap();
    assert_eq!(last.url.path(), IDENTITY_PATH);
    assert_eq!(
        last.headers
            .get("X-CAD-Company-Id")
            .and_then(|value| value.to_str().ok()),
        Some(OTHER_COMPANY_ID)
    );
    assert!(
        requests[before..requests.len() - 1]
            .iter()
            .all(|request| !request.headers.contains_key("X-CAD-Company-Id")),
        "the key is checked and the companies and roles are read with no company selected"
    );
    // Active is untouched.
    assert_eq!(world.set().active(), Some(&name("local")));
}

#[tokio::test]
async fn a_company_is_chosen_by_name_ignoring_case_and_spaces_and_the_old_one_is_reported() {
    let world = world_with_two_companies().await;
    world
        .run(
            &["use", "company", OTHER_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    let report = world
        .run(
            &["use", "company", "  aCME gmbh "],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        report.value,
        json!({
            "profile": "local",
            "companyId": COMPANY_ID,
            "companyName": "Acme GmbH",
            "previousCompanyId": OTHER_COMPANY_ID,
            "previousCompanyName": "Other AG"
        })
    );
    assert_eq!(
        report.messages,
        [(
            StatusKind::Success,
            format!(
                "Using company Acme GmbH on local ({}) as {EMAIL} (was Other AG)",
                world.origin
            )
        )]
    );
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(COMPANY_ID, Some("Acme GmbH")))
    );

    // The same company again changes nothing and reports no change.
    let report = world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(!report.messages[0].1.contains("(was"));
}

#[tokio::test]
async fn an_id_the_list_does_not_have_is_refused_without_asking_the_platform_for_access() {
    let world = world_with_two_companies().await;
    let before = world.set();

    let error = world
        .run(
            &["use", "company", THIRD_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Input(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        format!(
            "invalid input: your login cannot act on company {THIRD_COMPANY_ID}; it can act \
             on:\n{COMPANY_ID} Acme GmbH\n{OTHER_COMPANY_ID} Other AG"
        )
    );
    assert_eq!(world.set(), before, "nothing was saved");
    assert_no_access_check(&world).await;
}

/// No request selected a company: the platform was never asked for access.
async fn assert_no_access_check(world: &World) {
    let requests = world.server.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !request.headers.contains_key("X-CAD-Company-Id")),
        "the platform was asked to check access"
    );
}

#[tokio::test]
async fn a_visible_company_without_a_role_is_refused_and_the_message_lists_the_operable_ones() {
    // Acme is the user's own; Other AG is held through a role; Client AG is
    // visible in the list, but the user has no role in it; the admin role
    // grants nothing.
    let client = company_json(THIRD_COMPANY_ID, Some("Client AG"), "ACTIVE");
    let world = world_with_roles(
        &[acme(), other_company(), client],
        vec![
            role_json(OTHER_COMPANY_ID, "READ_ONLY"),
            role_json(THIRD_COMPANY_ID, "ADMIN"),
        ],
    )
    .await;
    login_local(&world).await;
    let before = world.set();

    for wanted in [THIRD_COMPANY_ID, "Client AG"] {
        let error = world
            .run(
                &["use", "company", wanted],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .unwrap_err();
        let head = if wanted == THIRD_COMPANY_ID {
            format!("your login cannot act on company {THIRD_COMPANY_ID}")
        } else {
            "no company your login can act on is named \"Client AG\"".to_owned()
        };
        assert!(matches!(error, CliError::Input(_)), "{error:?}");
        assert_eq!(exit_code(&error), ExitCode::from(2));
        assert_eq!(
            error.to_string(),
            format!(
                "invalid input: {head}; it can act on:\n{COMPANY_ID} Acme GmbH\n\
                 {OTHER_COMPANY_ID} Other AG"
            )
        );
    }
    assert_eq!(world.set(), before, "nothing was saved");
    assert_no_access_check(&world).await;

    // A company a role grants is chosen, and the platform is asked then.
    world
        .run(
            &["use", "company", "Other AG"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(OTHER_COMPANY_ID, Some("Other AG")))
    );
}

#[tokio::test]
async fn a_refusal_lists_twenty_companies_and_counts_the_rest() {
    let companies: Vec<_> = (0..22_u128)
        .map(|number| {
            company_json(
                &Uuid::from_u128(0x1000 + number).to_string(),
                Some(&format!("Client {number:02}")),
                "ACTIVE",
            )
        })
        .collect();
    let roles = companies
        .iter()
        .map(|company| role_json(company["id"].as_str().unwrap(), "EXPERT"))
        .collect();
    let world = world_with_roles(&companies, roles).await;
    login_local(&world).await;

    let error = world
        .run(
            &["use", "company", COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();

    let text = error.to_string();
    assert!(text.starts_with(&format!(
        "invalid input: your login cannot act on company {COMPANY_ID}; it can act on:\n"
    )));
    assert_eq!(text.lines().count(), 1 + 20 + 1, "{text}");
    assert!(text.ends_with("\n… and 2 more"), "{text}");
    assert!(text.contains("Client 19"), "{text}");
    assert!(!text.contains("Client 20"), "{text}");
}

#[tokio::test]
async fn the_login_step_sets_the_only_company_the_login_can_act_on() {
    // A consultancy admin with no role in any client company: the list carries
    // the clients, but only their own company is one they can act on.
    let world = world_with_roles(
        &[acme(), other_company(), acme_twin()],
        vec![role_json(OTHER_COMPANY_ID, "ADMIN")],
    )
    .await;

    let report = login_local(&world).await;

    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(COMPANY_ID, Some("Acme GmbH")))
    );
    assert!(
        report
            .messages
            .iter()
            .any(|(_, text)| text == "using company Acme GmbH for profile local"),
        "{:?}",
        report.messages
    );
}

#[tokio::test]
async fn at_a_terminal_the_menu_offers_only_the_companies_the_login_can_act_on() {
    let world = world_with_roles(
        &[acme(), other_company(), acme_twin()],
        vec![role_json(OTHER_COMPANY_ID, "EXPERT")],
    )
    .await;
    let terminal = Scripted {
        stdin: token().into_bytes(),
        ..Scripted::picking(&[1])
    };

    world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        terminal.menus(),
        [(
            "Company for profile local:".to_owned(),
            vec![
                format!("Acme GmbH ({COMPANY_ID})"),
                format!("Other AG ({OTHER_COMPANY_ID})"),
            ]
        )]
    );
}

#[tokio::test]
async fn a_name_nobody_has_is_refused_and_a_deleted_company_has_no_name_to_match() {
    let world = world_listing(&[
        acme(),
        company_json(OTHER_COMPANY_ID, Some("Gone Ltd"), "DELETED"),
        acme_twin(),
    ])
    .await;
    login_local(&world).await;
    world
        .run(
            &["use", "company", COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let before = world.set();

    for wanted in ["Nowhere", "Gone Ltd", "Acme"] {
        let error = world
            .run(
                &["use", "company", wanted],
                &Scripted::default(),
                &Env::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::Input(_)));
        assert_eq!(exit_code(&error), ExitCode::from(2));
        assert_eq!(
            error.to_string(),
            format!(
                "invalid input: no company your login can act on is named \"{wanted}\"; it can \
                 act on:\n{COMPANY_ID} Acme GmbH\n{THIRD_COMPANY_ID} ACME GMBH"
            )
        );
    }
    assert_eq!(world.set(), before, "nothing was saved");
}

/// Another company with the same name as Acme's, in another case.
fn acme_twin() -> Value {
    company_json(THIRD_COMPANY_ID, Some("ACME GMBH"), "ACTIVE")
}

/// A company the platform lists with a `name` and a different `displayName`.
fn company_called(id: &str, name: Option<&str>, display_name: Option<&str>) -> Value {
    let mut company = company_json(id, name, "ACTIVE");
    company["displayName"] = json!(display_name);
    company
}

#[tokio::test]
async fn a_company_is_matched_by_its_name_and_never_by_its_display_name() {
    let world = world_listing(&[company_called(
        COMPANY_ID,
        Some("Acme GmbH"),
        Some("Acme, the best company"),
    )])
    .await;
    login_local(&world).await;
    let before = world.set();

    let error = world
        .run(
            &["use", "company", "Acme, the best company"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(world.set(), before, "nothing was saved");

    let report = world
        .run(
            &["use", "company", "acme gmbh"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.value["companyName"], "Acme GmbH");
}

#[tokio::test]
async fn a_multi_line_name_is_one_line_in_the_status_and_in_the_output() {
    let world = world_listing(&[company_called(
        COMPANY_ID,
        Some("  Acme\n\tGmbH \r\n  Holding\n"),
        Some("A long\ndisplay text"),
    )])
    .await;
    login_local(&world).await;

    let report = world
        .run(
            &["use", "company", "acme gmbh holding"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(report.value["companyName"], "Acme GmbH Holding");
    assert_eq!(
        report.messages,
        [(
            StatusKind::Success,
            format!(
                "Using company Acme GmbH Holding on local ({}) as {EMAIL}",
                world.origin
            )
        )]
    );
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(COMPANY_ID, Some("Acme GmbH Holding")))
    );
}

#[tokio::test]
async fn several_companies_with_the_name_are_listed_with_their_ids_and_nothing_is_saved() {
    let world = world_listing(&[acme(), other_company(), acme_twin()]).await;
    login_local(&world).await;
    let before = world.set();

    let error = world
        .run(
            &["use", "company", "acme gmbh"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        format!(
            "invalid input: several companies your login can act on are named \"acme gmbh\"; \
             use the ID of the one you mean:\n{COMPANY_ID} Acme GmbH\n{THIRD_COMPANY_ID} ACME GMBH"
        )
    );
    assert_eq!(world.set(), before);
}

#[tokio::test]
async fn a_company_the_platform_refuses_saves_nothing_and_keeps_the_errors_exit_code() {
    // The roles grant it, so it is asked of the platform, which says no.
    let world = world_listing(&[
        acme(),
        other_company(),
        company_json(FORBIDDEN_COMPANY_ID, Some("Forbidden SE"), "ACTIVE"),
    ])
    .await;
    login_local(&world).await;
    world
        .run(
            &["use", "company", OTHER_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let before = world.set();

    let error = world
        .run(
            &["use", "company", FORBIDDEN_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Api { .. }), "{error:?}");
    assert_eq!(exit_code(&error), ExitCode::from(4));
    assert!(error.to_string().contains("HTTP 403"), "{error}");
    assert_eq!(world.set(), before, "nothing was saved");
}

#[tokio::test]
async fn a_rejected_key_exits_three_before_anything_is_listed() {
    let world = world_with_two_companies().await;
    let error = world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default()
                .with("CAD_API_KEY", KEY_ID)
                .with("CAD_API_SECRET", "wrong"),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(3));
    assert_eq!(company_of(&world, "local"), None);
}

#[tokio::test]
async fn the_company_goes_to_the_profile_named_with_profile_and_active_stays() {
    let world = world_with_two_companies().await;
    // A second profile on the same server; it becomes the active one.
    world
        .profile(
            "second",
            &["--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(world.set().active(), Some(&name("second")));
    assert_eq!(company_of(&world, "second"), None);

    let report = world
        .run(
            &["--profile", "local", "use", "company", "Other AG"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(report.value["profile"], "local");
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(OTHER_COMPANY_ID, Some("Other AG")))
    );
    assert_eq!(
        company_of(&world, "second"),
        None,
        "the active profile is left alone"
    );
    assert_eq!(world.set().active(), Some(&name("second")));

    // Without --profile it is the active profile that changes.
    let report = world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.value["profile"], "second");
}

#[tokio::test]
async fn use_company_refuses_company_id_needs_a_profile_and_a_login() {
    let world = world_with_two_companies().await;
    let before = world.set();
    let error = world
        .run(
            &["--company-id", COMPANY_ID, "use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(
        error.to_string(),
        "invalid input: --company-id does not apply to `cad-cli use company`; name the company \
         as its argument"
    );
    assert_eq!(world.set(), before);

    // A profile with no login.
    world
        .run(
            &["use", "profile", "dev"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let error = world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(
        error.to_string().contains("profile dev has no login"),
        "{error}"
    );

    // No profile at all, even with a server and a key in the environment.
    let empty = self::world().await;
    let error = empty
        .run(
            &[
                "--server-url",
                &empty.origin,
                "--allow-insecure-http",
                "use",
                "company",
                "Acme",
            ],
            &Scripted::default(),
            &Env::default()
                .with("CAD_API_KEY", KEY_ID)
                .with("CAD_API_SECRET", SECRET),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Config { .. }));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        "configuration error: `cad-cli use company` needs a profile; create one with `cad-cli \
         use profile <name>`"
    );
    assert_eq!(empty.requests().await, 0);
}

#[tokio::test]
async fn a_company_in_the_environment_is_said_to_win_over_the_profiles() {
    let world = world_with_two_companies().await;
    let report = world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default().with("CAD_COMPANY_ID", OTHER_COMPANY_ID),
        )
        .await
        .unwrap();
    assert_eq!(report.messages.len(), 2);
    assert_eq!(report.messages[1].0, StatusKind::Warning);
    assert_eq!(
        report.messages[1].1,
        format!(
            "CAD_COMPANY_ID is set to {OTHER_COMPANY_ID} and overrides the company of the \
             profile in this shell"
        )
    );
    // Even a malformed variable does not stop the choice: it does not apply.
    world
        .run(
            &["use", "company", "Other AG"],
            &Scripted::default(),
            &Env::default().with("CAD_COMPANY_ID", "nonsense"),
        )
        .await
        .unwrap();
}

// --- the company offered after a login -----------------------------------------

#[tokio::test]
async fn several_companies_and_no_terminal_leave_it_unset_with_the_way_to_set_it() {
    let world = world_listing(&[acme(), other_company()]).await;
    let report = login_local(&world).await;

    assert_eq!(report.value["companyId"], Value::Null);
    assert_eq!(report.value["companyName"], Value::Null);
    let messages: Vec<_> = report
        .messages
        .iter()
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(messages.len(), 3, "{messages:?}");
    assert_eq!(
        report.messages[1],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );
    assert_eq!(company_of(&world, "local"), None);
}

#[tokio::test]
async fn at_a_terminal_the_person_picks_among_the_companies_sorted_by_name() {
    // Listed out of order: the menu is by name.
    let world = world_listing(&[other_company(), acme()]).await;
    let terminal = Scripted {
        stdin: token().into_bytes(),
        ..Scripted::picking(&[1])
    };

    let report = world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(
        terminal.menus(),
        [(
            "Company for profile local:".to_owned(),
            vec![
                format!("Acme GmbH ({COMPANY_ID})"),
                format!("Other AG ({OTHER_COMPANY_ID})"),
            ]
        )]
    );
    assert_eq!(report.value["companyId"], OTHER_COMPANY_ID);
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(OTHER_COMPANY_ID, Some("Other AG")))
    );
    assert!(
        report
            .messages
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Success
                && text == "using company Other AG for profile local")
    );
}

#[tokio::test]
async fn giving_up_the_menu_leaves_the_company_unset_with_the_hint_and_the_login_stays() {
    let world = world_listing(&[acme(), other_company()]).await;
    let terminal = Scripted {
        stdin: token().into_bytes(),
        ..Scripted::picking(&[])
    };

    let report = world
        .profile(
            "local",
            &["--with-token", "--insecure-storage"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(terminal.menus().len(), 1);
    assert_eq!(company_of(&world, "local"), None);
    assert_eq!(
        report.messages[1],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );
    assert_eq!(report.value["email"], EMAIL, "the login was kept");
    assert!(world.stored());
}

#[tokio::test]
async fn no_companies_or_a_list_that_fails_leave_it_unset_and_the_login_succeeds() {
    let none = world_listing(&[]).await;
    let report = login_local(&none).await;
    assert_eq!(
        report.messages[1],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );
    assert_eq!(company_of(&none, "local"), None);

    let only_deleted = world_listing(&[company_json(COMPANY_ID, Some("Gone"), "DELETED")]).await;
    let report = login_local(&only_deleted).await;
    assert_eq!(
        report.messages[1],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );

    let failing = world_fixture(MemoryBackend::default(), None).await;
    let report = login_local(&failing).await;
    assert_eq!(report.messages[1].0, StatusKind::Warning);
    assert!(
        report.messages[1]
            .1
            .starts_with("could not list the companies ("),
        "{:?}",
        report.messages
    );
    assert_eq!(
        report.messages[2],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );
    assert_eq!(company_of(&failing, "local"), None);
    assert!(failing.stored(), "the login stays");
}

#[tokio::test]
async fn logging_in_as_another_user_clears_the_company_and_the_same_user_keeps_it() {
    let world = world_with_two_companies().await;
    world
        .run(
            &["use", "company", OTHER_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    // The same user logging in again keeps the company and asks nothing.
    let before = world.requests().await;
    let report = world
        .profile(
            "local",
            &["--login", "--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        world.requests().await,
        before + 2,
        "the identity and the user"
    );
    assert_eq!(report.value["companyId"], OTHER_COMPANY_ID);

    // The profile's login was made by someone else: the company was theirs.
    let mut set = world.set();
    let mut profile = set.get(&name("local")).unwrap().clone();
    profile.login.as_mut().unwrap().user_id = Uuid::from_u128(0x99).into();
    set.insert(name("local"), profile);
    world.profiles.save(&set).unwrap();

    let report = world
        .profile(
            "local",
            &["--login", "--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.value["companyId"], Value::Null);
    assert!(
        report.messages.iter().any(|(_, text)| text
            == "the login is for another account; the company of profile local was cleared"),
        "{:?}",
        report.messages
    );
    // ...and, with several to choose from and no terminal, it stays unset.
    assert!(
        report
            .messages
            .contains(&(StatusKind::Warning, NO_COMPANY_HINT.to_owned()))
    );
    assert_eq!(company_of(&world, "local"), None);
}

#[tokio::test]
async fn switching_to_a_profile_without_a_company_makes_no_request_and_says_how_to_set_it() {
    let world = world_with_two_companies().await;
    let before = world.requests().await;

    let report = world
        .run(
            &["use", "profile", "local"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(world.requests().await, before);
    assert_eq!(
        report.messages[0],
        (StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );

    // A profile that has a company says nothing.
    world
        .run(
            &["use", "company", "Acme GmbH"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let report = world
        .run(
            &["use", "profile", "local"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(
        report
            .messages
            .iter()
            .all(|(kind, _)| *kind != StatusKind::Warning)
    );
}

// --- use --check ---------------------------------------------------------------

#[tokio::test]
async fn check_reports_the_profile_the_login_the_user_and_the_company_with_their_sources() {
    let world = world_with_two_companies().await;
    world
        .run(
            &["use", "company", "Other AG"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    // The company of the profile.
    let before = world.requests().await;
    let report = world
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(
        report.value,
        json!({
            "profile": "local",
            "url": world.origin,
            "credentialsSource": "file",
            "keyId": KEY_ID,
            "userId": USER_ID,
            "email": EMAIL,
            "companyId": OTHER_COMPANY_ID,
            "companyName": "Other AG",
            "companySource": "profile",
            "resolvedCompanyId": OTHER_COMPANY_ID,
            "resolvedCompanySource": "HEADER"
        })
    );
    // One identity request, carrying the company, and the user's e-mail.
    let requests = world.server.received_requests().await.unwrap();
    assert_eq!(requests.len(), before + 2);
    assert_eq!(requests[before].url.path(), IDENTITY_PATH);
    assert_eq!(
        requests[before]
            .headers
            .get("X-CAD-Company-Id")
            .and_then(|value| value.to_str().ok()),
        Some(OTHER_COMPANY_ID)
    );
    assert_eq!(requests[before + 1].url.path(), USER_PATH);
    assert_eq!(report.messages[0].0, StatusKind::Success);
    assert_eq!(
        report.messages[0].1,
        format!(
            "{}: authenticated as {EMAIL} with profile local",
            world.origin
        )
    );
    assert_eq!(
        report.messages[1],
        (
            StatusKind::Info,
            format!("company Other AG ({OTHER_COMPANY_ID})")
        )
    );
}

#[tokio::test]
async fn check_names_the_variable_the_flag_and_the_environments_key_as_sources() {
    let world = world_with_two_companies().await;
    world
        .run(
            &["use", "company", "Other AG"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();

    // The variable beats the profile: a company the profile has no label for.
    let report = world
        .run(
            &["use", "--check"],
            &Scripted::default(),
            &Env::default().with("CAD_COMPANY_ID", COMPANY_ID),
        )
        .await
        .unwrap();
    assert_eq!(report.value["companyId"], COMPANY_ID);
    assert_eq!(report.value["companyName"], Value::Null);
    assert_eq!(report.value["companySource"], "environment");
    assert_eq!(report.value["resolvedCompanyId"], COMPANY_ID);
    assert_eq!(report.value["resolvedCompanySource"], "HEADER");

    // The flag beats both.
    let report = world
        .run(
            &["--company-id", THIRD_COMPANY_ID, "use", "--check"],
            &Scripted::default(),
            &Env::default().with("CAD_COMPANY_ID", COMPANY_ID),
        )
        .await
        .unwrap();
    assert_eq!(report.value["companyId"], THIRD_COMPANY_ID);
    assert_eq!(report.value["companyName"], Value::Null);
    assert_eq!(report.value["companySource"], "flag");

    // The environment's key, and no profile at all.
    let report = world
        .run(
            &[
                "--server-url",
                &world.origin,
                "--allow-insecure-http",
                "--profile",
                "local",
                "use",
                "--check",
            ],
            &Scripted::default(),
            &Env::default()
                .with("CAD_API_KEY", KEY_ID)
                .with("CAD_API_SECRET", SECRET),
        )
        .await
        .unwrap();
    assert_eq!(report.value["credentialsSource"], "environment");
    assert_eq!(report.value["keyId"], KEY_ID);
    assert_eq!(report.value["profile"], "local");
}

#[tokio::test]
async fn check_names_the_keyring_when_the_login_is_there() {
    let world = world().await;
    world
        .profile(
            "local",
            &["--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    let report = world
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(report.value["credentialsSource"], "keyring");
    assert_eq!(report.value["keyId"], KEY_ID);
}

#[tokio::test]
async fn check_without_a_company_reports_none_and_is_not_an_error() {
    let world = world_with_two_companies().await;

    let report = world
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap();

    for field in ["companyId", "companyName", "companySource"] {
        assert_eq!(report.value[field], Value::Null, "{field}");
    }
    assert_eq!(report.value["profile"], "local");
    // Nothing was selected, so the platform's answer is its own.
    assert_eq!(report.value["resolvedCompanyId"], COMPANY_ID);
    assert_eq!(report.value["resolvedCompanySource"], "DEFAULT");
    assert_eq!(
        report.messages.last().unwrap(),
        &(StatusKind::Warning, NO_COMPANY_HINT.to_owned())
    );
    let requests = world.server.received_requests().await.unwrap();
    let identity = requests
        .iter()
        .rev()
        .find(|request| request.url.path() == IDENTITY_PATH)
        .unwrap();
    assert!(!identity.headers.contains_key("X-CAD-Company-Id"));
}

#[tokio::test]
async fn check_reads_the_email_afresh_and_survives_not_getting_it() {
    let world = world_with_two_companies().await;
    world.email_known.store(false, Ordering::SeqCst);

    let report = world
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap();

    assert_eq!(report.value["userId"], USER_ID);
    assert_eq!(report.value["email"], Value::Null);
    assert!(
        report
            .messages
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Warning
                && text.starts_with("could not read the e-mail of the account (")),
        "{:?}",
        report.messages
    );
    assert_eq!(
        report.messages[0].1,
        format!(
            "{}: authenticated as {USER_ID} with profile local",
            world.origin
        )
    );
}

#[tokio::test]
async fn check_fails_like_a_command_without_credentials_with_a_rejected_key_or_a_refused_company() {
    let world = world_with_two_companies().await;

    // A profile with no login: exit 2, and nothing was asked of the server.
    world
        .run(
            &["use", "profile", "dev"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let before = world.requests().await;
    let error = world
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert_eq!(world.requests().await, before);

    // No profile and no credentials either.
    let empty = self::world().await;
    let error = empty
        .run(&["use", "--check"], &Scripted::default(), &Env::default())
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));

    // A key the server rejects: exit 3.
    let error = world
        .run(
            &["--profile", "local", "use", "--check"],
            &Scripted::default(),
            &Env::default()
                .with("CAD_API_KEY", KEY_ID)
                .with("CAD_API_SECRET", "wrong"),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(3));

    // A company it cannot act on: the exit code of that error.
    let error = world
        .run(
            &[
                "--profile",
                "local",
                "--company-id",
                FORBIDDEN_COMPANY_ID,
                "use",
                "--check",
            ],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(4));
}

// --- `-` picks from a list ------------------------------------------------------

#[test]
fn a_dash_parses_as_the_choice_to_pick_for_the_profile_and_the_company() {
    use super::company::CompanyChoice;
    use super::profile::ProfileChoice;
    use crate::cli::CompanyArg;

    let parse = |rest: &[&str]| {
        let mut args = vec!["cad-cli"];
        args.extend_from_slice(rest);
        Cli::try_parse_from(args).expect("parses")
    };

    let Command::Use(group) = parse(&["use", "profile", "-"]).command else {
        panic!("expected a use command");
    };
    let Some(super::Command::Profile(args)) = group.command else {
        panic!("expected `use profile`");
    };
    assert_eq!(args.name, ProfileChoice::Pick);

    let Command::Use(group) = parse(&["use", "company", "-"]).command else {
        panic!("expected a use command");
    };
    let Some(super::Command::Company(args)) = group.command else {
        panic!("expected `use company`");
    };
    assert_eq!(args.company, CompanyChoice::Pick);

    let cli = parse(&["--company-id", "-", "use", "--check"]);
    assert_eq!(cli.global.company_id, Some(CompanyArg::Pick));

    // Neither a name nor a longer word that starts with a dash is a dash.
    assert_eq!("-".parse::<CompanyChoice>(), Ok(CompanyChoice::Pick));
    assert_eq!(
        "-x".parse::<CompanyChoice>(),
        Ok(CompanyChoice::Name("-x".to_owned()))
    );
    Cli::try_parse_from(["cad-cli", "use", "profile", "-x"]).expect_err("not a profile name");
}

/// A world with the logged-in profile `local` and `staging` (which has no
/// login), the active one.
async fn world_with_two_profiles() -> World {
    let world = world().await;
    login_local(&world).await;
    world
        .run(
            &["use", "profile", "staging"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(world.set().active(), Some(&name("staging")));
    world
}

#[tokio::test]
async fn use_profile_dash_lists_the_profiles_and_switches_to_the_one_picked_without_a_request() {
    let world = world_with_two_profiles().await;
    let before = world.requests().await;
    let reads = Arc::new(AtomicUsize::new(0));
    let counting = CredentialStore::new(
        Some(Box::new(CountingKeyring {
            reads: Arc::clone(&reads),
        })),
        Some(FileBackend::new(world.credentials_file.clone())),
    );
    let terminal = Scripted::picking(&[0]);

    let report = world
        .run_store(
            &["use", "profile", "-"],
            &terminal,
            &Env::default(),
            &counting,
        )
        .await
        .unwrap();

    assert_eq!(
        terminal.menus(),
        [(
            "Profile:".to_owned(),
            vec![
                format!("local  {}  {EMAIL}", world.origin),
                "staging  https://staging.carbonaltdelete.eu  no login".to_owned(),
            ]
        )]
    );
    assert_eq!(
        terminal.starts(),
        [1],
        "the cursor starts on the active one"
    );
    assert_eq!(world.requests().await, before, "no request");
    assert_eq!(reads.load(Ordering::SeqCst), 0, "the keyring was not read");
    assert_eq!(world.set().active(), Some(&name("local")));
    assert_eq!(report.value["name"], "local");
    assert_eq!(report.value["previous"], "staging");
    assert_eq!(
        report.messages.last().unwrap().1,
        format!(
            "now using local ({}) as {EMAIL} (was staging)",
            world.origin
        )
    );
}

#[tokio::test]
async fn use_profile_dash_gives_up_with_the_list_and_changes_nothing() {
    let world = world_with_two_profiles().await;
    let terminal = Scripted::picking(&[]);

    let error = world
        .run(&["use", "profile", "-"], &terminal, &Env::default())
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Cancelled), "{error}");
    assert_eq!(world.set().active(), Some(&name("staging")));
}

#[tokio::test]
async fn use_profile_dash_refuses_a_url_and_no_profiles_and_no_terminal() {
    let world = world_with_two_profiles().await;
    let before = world.requests().await;

    let terminal = Scripted::picking(&[0]);
    let error = world
        .run(
            &["use", "profile", "-", "--url", "https://x.test"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "invalid input: --url needs a profile name, not -"
    );

    let error = world
        .run(
            &["use", "profile", "-"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "invalid input: `-` picks a profile at a terminal, and there is none; name the profile"
    );
    assert_eq!(terminal.menus(), []);
    assert_eq!(world.requests().await, before);
    assert_eq!(world.set().active(), Some(&name("staging")));

    let empty = self::world().await;
    let terminal = Scripted::picking(&[0]);
    let error = empty
        .run(&["use", "profile", "-"], &terminal, &Env::default())
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "invalid input: there is no profile to pick; create one with `cad-cli use profile <name>`"
    );
    assert_eq!(terminal.menus(), []);
}

#[tokio::test]
async fn use_company_dash_lists_the_companies_and_saves_the_one_picked_after_its_access_check() {
    let world = world_with_two_companies().await;
    world
        .run(
            &["use", "company", OTHER_COMPANY_ID],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap();
    let before = world.requests().await;
    let terminal = Scripted::picking(&[0]);

    let report = world
        .run(&["use", "company", "-"], &terminal, &Env::default())
        .await
        .unwrap();

    assert_eq!(
        terminal.menus(),
        [(
            "Company:".to_owned(),
            vec![
                format!("Acme GmbH  {COMPANY_ID}"),
                format!("Other AG  {OTHER_COMPANY_ID}"),
            ]
        )]
    );
    assert_eq!(
        terminal.starts(),
        [1],
        "the cursor starts on the profile's company"
    );
    assert_eq!(report.value["companyId"], COMPANY_ID);
    assert_eq!(report.value["previousCompanyId"], OTHER_COMPANY_ID);
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(COMPANY_ID, Some("Acme GmbH")))
    );
    // The key, the companies, the roles, and access to the one picked.
    assert_eq!(world.requests().await, before + 4);
    let requests = world.server.received_requests().await.unwrap();
    let last = requests.last().unwrap();
    assert_eq!(last.url.path(), IDENTITY_PATH);
    assert_eq!(
        last.headers
            .get("X-CAD-Company-Id")
            .and_then(|value| value.to_str().ok()),
        Some(COMPANY_ID)
    );
}

#[tokio::test]
async fn use_company_dash_starts_on_the_first_company_when_the_profile_has_none() {
    let world = world_with_two_companies().await;
    let terminal = Scripted::picking(&[1]);

    world
        .run(&["use", "company", "-"], &terminal, &Env::default())
        .await
        .unwrap();

    assert_eq!(terminal.starts(), [0]);
    assert_eq!(
        company_of(&world, "local"),
        Some(chosen(OTHER_COMPANY_ID, Some("Other AG")))
    );
}

#[tokio::test]
async fn use_company_dash_gives_up_with_the_list_and_saves_nothing() {
    let world = world_with_two_companies().await;
    let terminal = Scripted::picking(&[]);

    let error = world
        .run(&["use", "company", "-"], &terminal, &Env::default())
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Cancelled), "{error}");
    assert_eq!(company_of(&world, "local"), None);
}

#[tokio::test]
async fn use_company_dash_without_a_terminal_names_what_to_pass_and_requests_nothing() {
    let world = world_with_two_companies().await;
    let before = world.requests().await;

    let error = world
        .run(
            &["use", "company", "-"],
            &Scripted::default(),
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Input(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "invalid input: `-` picks a company at a terminal, and there is none; pass its ID or name"
    );
    assert_eq!(world.requests().await, before);
}

#[tokio::test]
async fn use_company_dash_when_the_login_can_act_on_no_company_is_refused() {
    let world = world_with_roles(&[other_company()], Vec::new()).await;
    login_local(&world).await;
    let terminal = Scripted::picking(&[0]);

    let error = world
        .run(&["use", "company", "-"], &terminal, &Env::default())
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Input(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "invalid input: your login cannot pick a company; it can act on no company"
    );
    assert_eq!(terminal.menus(), []);
}
