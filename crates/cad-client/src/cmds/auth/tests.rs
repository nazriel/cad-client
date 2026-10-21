//! `auth login | status | logout` end to end through the command group, with
//! a scripted terminal, a `wiremock` identity endpoint, the in-memory keyring
//! and a credentials file in a temporary directory. The real keyring and the
//! real configuration directory are never touched.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use clap::Parser;
use secrecy::SecretString;
use serde_json::{Value, json};
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::Report;
use crate::app::context::CommandContext;
use crate::cli::{Cli, OutputFormat};
use crate::cmds::Command;
use crate::config::Environment;
use crate::error::CliError;
use crate::support::credentials::memory::MemoryBackend;
use crate::support::credentials::{CredentialStore, FileBackend};
use crate::support::terminal::{Progress, StatusKind, Terminal};

const KEY_ID: &str = "00000000-0000-0000-0000-0000000000aa";
const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";
const USER_ID: &str = "00000000-0000-0000-0000-0000000000bb";
const COMPANY_ID: &str = "00000000-0000-0000-0000-0000000000cc";
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const KEYRING_DOWN: &str = "no secret service is running";

fn token() -> String {
    format!("{KEY_ID}.{SECRET}")
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
    key_ids: Mutex<VecDeque<String>>,
    secrets: Mutex<VecDeque<String>>,
    answers: Mutex<VecDeque<bool>>,
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
        let queue = |items: &[&str]| items.iter().map(|item| (*item).to_owned()).collect();
        Self {
            interactive: true,
            key_ids: Mutex::new(queue(key_ids)),
            secrets: Mutex::new(queue(secrets)),
            answers: Mutex::new(answers.iter().copied().collect()),
            ..Self::default()
        }
    }

    fn opened(&self) -> Vec<String> {
        self.opened.lock().unwrap().clone()
    }

    fn questions(&self) -> Vec<String> {
        self.questions.lock().unwrap().clone()
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
        panic!("auth never goes through `confirm`: `--yes` must not answer its questions");
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(self.stdin.clone())
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn prompt(&self, label: &str) -> Result<String, CliError> {
        assert_eq!(label, "Key ID:");
        Ok(self
            .key_ids
            .lock()
            .unwrap()
            .pop_front()
            .expect("a scripted Key ID"))
    }

    fn prompt_secret(&self, label: &str) -> Result<SecretString, CliError> {
        assert_eq!(label, "Secret:");
        Ok(SecretString::from(
            self.secrets
                .lock()
                .unwrap()
                .pop_front()
                .expect("a scripted Secret"),
        ))
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

/// A platform that accepts exactly one key.
async fn platform() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
            "companySelection": {"resolvedCompanyId": COMPANY_ID, "source": "DEFAULT"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"detail": "nope"})))
        .mount(&server)
        .await;
    server
}

struct World {
    server: MockServer,
    origin: String,
    keyring: Arc<MemoryBackend>,
    store: CredentialStore,
    file: PathBuf,
    _dir: tempfile::TempDir,
}

async fn world() -> World {
    world_with(MemoryBackend::default()).await
}

async fn world_with(keyring: MemoryBackend) -> World {
    let server = platform().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("cad-cli").join("credentials.json");
    let keyring = Arc::new(keyring);
    let store = CredentialStore::new(
        Some(Box::new(Arc::clone(&keyring))),
        Some(FileBackend::new(file.clone())),
    );
    World {
        origin: server.uri(),
        server,
        keyring,
        store,
        file,
        _dir: dir,
    }
}

impl World {
    fn args<'a>(&'a self, rest: &[&'a str]) -> Vec<&'a str> {
        let mut args = vec![
            "cad-cli",
            "--server-url",
            self.origin.as_str(),
            "--allow-insecure-http",
        ];
        args.extend_from_slice(rest);
        args
    }

    async fn run(
        &self,
        rest: &[&str],
        terminal: &Scripted,
        environment: &Env,
    ) -> Result<Report, CliError> {
        let cli = Cli::try_parse_from(self.args(rest)).expect("arguments parse");
        let Command::Auth(group) = cli.command else {
            panic!("expected an auth command");
        };
        let context = CommandContext::new(terminal, cli.global.yes);
        group
            .run(&context, &cli.global, environment, &self.store)
            .await
    }

    fn stored(&self) -> bool {
        self.keyring.text(&self.origin).is_some() || self.file.exists()
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
fn the_group_needs_a_subcommand_and_login_takes_its_flags() {
    Cli::try_parse_from(["cad-cli", "auth"]).expect_err("a bare `auth` shows help");
    for rest in [
        vec!["login"],
        vec!["login", "--no-browser"],
        vec!["login", "--with-token", "--insecure-storage"],
        vec!["status"],
        vec!["logout"],
    ] {
        let mut args = vec!["cad-cli", "auth"];
        args.extend(rest);
        Cli::try_parse_from(args).expect("parses");
    }
    Cli::try_parse_from(["cad-cli", "auth", "status", "--with-token"]).expect_err("login only");
}

#[test]
fn auth_is_neither_offline_nor_an_online_platform_command() {
    let cli = Cli::try_parse_from(["cad-cli", "auth", "login"]).unwrap();
    assert!(matches!(
        cli.command.into_invocation(),
        crate::cmds::Invocation::Auth(_)
    ));
}

// --- login -------------------------------------------------------------------

#[tokio::test]
async fn login_with_a_token_validates_stores_in_the_file_and_reports() {
    let world = world().await;
    let terminal = Scripted::piped(&format!("{}\n", token()));
    let report = world
        .run(
            &["auth", "login", "--with-token", "--insecure-storage"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    let file = world.file.display().to_string();
    assert_eq!(
        report.value,
        json!({
            "origin": world.origin,
            "keyId": KEY_ID,
            "userId": USER_ID,
            "affiliatedCompanyId": COMPANY_ID,
            "storage": "file",
            "path": file,
        })
    );
    assert_eq!(
        report.messages,
        [(
            StatusKind::Success,
            format!(
                "logged in to {} as {USER_ID}, stored in {file}",
                world.origin
            )
        )]
    );
    assert!(
        world.keyring.text(&world.origin).is_none(),
        "the keyring was skipped"
    );
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(&world.file).unwrap()).unwrap();
    assert_eq!(stored["credentials"][&world.origin]["secret"], SECRET);
    assert_eq!(stored["credentials"][&world.origin]["userId"], USER_ID);
    assert!(!shown(&report).contains(SECRET));
    assert_eq!(world.requests().await, 1);
}

#[tokio::test]
async fn login_prefers_the_keyring() {
    let world = world().await;
    let terminal = Scripted::piped(&token());
    let report = world
        .run(
            &["auth", "login", "--with-token"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();

    assert_eq!(report.value["storage"], "keyring");
    assert_eq!(report.value["path"], Value::Null);
    assert_eq!(
        report.messages[0].1,
        format!(
            "logged in to {} as {USER_ID}, stored in the system keyring",
            world.origin
        )
    );
    let stored: Value = serde_json::from_str(&world.keyring.text(&world.origin).unwrap()).unwrap();
    assert_eq!(stored["kind"], "apiKey");
    assert_eq!(stored["keyId"], KEY_ID);
    assert!(!world.file.exists());
    assert!(!shown(&report).contains(SECRET));
}

#[tokio::test]
async fn a_rejected_key_exits_three_and_stores_nothing() {
    let world = world().await;
    let other = token().replace("AbCdEf", "ZzZzZz");
    let terminal = Scripted::piped(&other);
    let error = world
        .run(
            &["auth", "login", "--with-token"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CliError::Authentication { .. }), "{error}");
    assert_eq!(exit_code(&error), ExitCode::from(3));
    assert!(!world.stored());
    assert!(!error.to_string().contains("ZzZzZz"));
}

#[tokio::test]
async fn a_network_error_stores_nothing() {
    let world = world().await;
    let terminal = Scripted::piped(&token());
    let cli = Cli::try_parse_from([
        "cad-cli",
        "--server-url",
        "http://127.0.0.1:1",
        "--allow-insecure-http",
        "auth",
        "login",
        "--with-token",
        "--insecure-storage",
    ])
    .unwrap();
    let Command::Auth(group) = cli.command else {
        unreachable!()
    };
    let context = CommandContext::new(&terminal, false);
    let error = group
        .run(&context, &cli.global, &Env::default(), &world.store)
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Transport { .. }), "{error}");
    assert_eq!(exit_code(&error), ExitCode::from(5));
    assert!(!world.file.exists());
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
        let terminal = Scripted::piped(input);
        let error = world
            .run(
                &["auth", "login", "--with-token"],
                &terminal,
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
async fn without_a_terminal_or_a_token_login_names_the_flag() {
    let world = world().await;
    let terminal = Scripted::default();
    let error = world
        .run(&["auth", "login"], &terminal, &Env::default())
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("--with-token"), "{error}");
    assert_eq!(world.requests().await, 0);
    // The page is still printed, so the key can be made and piped in later.
    assert!(terminal.statuses()[0].1.contains(&page(&world.origin)));
}

#[tokio::test]
async fn a_keyring_that_fails_exits_two_and_names_the_way_out_without_a_terminal() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;
    for rest in [
        vec!["auth", "login", "--with-token"],
        // `--yes` does not answer the question: nothing may be written.
        vec!["--yes", "auth", "login", "--with-token"],
    ] {
        let terminal = Scripted::piped(&token());
        let error = world
            .run(&rest, &terminal, &Env::default())
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::AuthConfig(_)));
        assert_eq!(exit_code(&error), ExitCode::from(2));
        let message = error.to_string();
        assert!(message.contains(KEYRING_DOWN), "{message}");
        assert!(message.contains("--insecure-storage"), "{message}");
        assert!(message.contains("CAD_API_KEY"), "{message}");
        assert!(
            message.contains(&world.file.display().to_string()),
            "{message}"
        );
        assert!(!message.contains(SECRET));
        assert!(!world.file.exists());
        assert_eq!(terminal.questions(), Vec::<String>::new());
    }
}

#[tokio::test]
async fn a_keyring_turned_off_behaves_like_one_that_fails() {
    let server = platform().await;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("cad-cli").join("credentials.json");
    let store = CredentialStore::new(None, Some(FileBackend::new(file.clone())));
    let origin = server.uri();
    let cli = Cli::try_parse_from([
        "cad-cli",
        "--server-url",
        &origin,
        "--allow-insecure-http",
        "auth",
        "login",
        "--with-token",
    ])
    .unwrap();
    let Command::Auth(group) = cli.command else {
        unreachable!()
    };
    let terminal = Scripted::piped(&token());
    let context = CommandContext::new(&terminal, false);
    let error = group
        .run(&context, &cli.global, &Env::default(), &store)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("CAD_CLI_KEYRING"), "{error}");
    assert!(error.to_string().contains("--insecure-storage"), "{error}");
    assert!(!file.exists());
}

#[tokio::test]
async fn at_a_terminal_a_failed_keyring_offers_the_file_and_only_yes_writes_it() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;

    let declined = Scripted::typing(&[KEY_ID], &[SECRET], &[false]);
    let error = world
        .run(
            &["auth", "login", "--no-browser"],
            &declined,
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("CAD_API_KEY"), "{error}");
    assert!(!world.stored());
    let file = world.file.display().to_string();
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
            &["--yes", "auth", "login", "--no-browser"],
            &accepted,
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(accepted.questions().len(), 1);
    assert_eq!(report.value["storage"], "file");
    assert_eq!(report.value["path"], file);
    assert!(world.file.exists());
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
        .run(&["auth", "login"], &terminal, &Env::default())
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
async fn a_value_that_stays_malformed_is_given_up_on_after_three_tries() {
    let world = world().await;
    let terminal = Scripted::typing(&["a", "b", "c"], &[], &[]);
    let error = world
        .run(
            &["auth", "login", "--no-browser"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert_eq!(
        terminal
            .statuses()
            .iter()
            .filter(|(kind, _)| *kind == StatusKind::Warning)
            .count(),
        2
    );
    assert_eq!(world.requests().await, 0);
}

#[tokio::test]
async fn the_browser_opens_only_when_it_can_help() {
    let world = world().await;
    let keyed = || Scripted::typing(&[KEY_ID], &[SECRET], &[]);

    let terminal = keyed();
    world
        .run(&["auth", "login"], &terminal, &Env::default())
        .await
        .unwrap();
    assert_eq!(terminal.opened(), [page(&world.origin)]);

    let terminal = keyed();
    world
        .run(
            &["auth", "login", "--no-browser"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(terminal.opened(), Vec::<String>::new());

    let terminal = Scripted::piped(&token());
    world
        .run(
            &["auth", "login", "--with-token"],
            &terminal,
            &Env::default(),
        )
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
        let terminal = Scripted::piped(&token());
        let report = world
            .run(&["auth", "login", "--with-token"], &terminal, &environment)
            .await
            .unwrap();
        assert_eq!(report.messages.len(), 2);
        assert_eq!(report.messages[1].0, StatusKind::Warning);
        assert!(report.messages[1].1.contains("CAD_API_KEY"));
    }
    let terminal = Scripted::piped(&token());
    let report = world
        .run(
            &["auth", "login", "--with-token"],
            &terminal,
            &Env::default(),
        )
        .await
        .unwrap();
    assert_eq!(report.messages.len(), 1);
}

#[tokio::test]
async fn logging_in_again_replaces_the_other_place() {
    let world = world().await;
    let piped = || Scripted::piped(&token());
    world
        .run(
            &["auth", "login", "--with-token", "--insecure-storage"],
            &piped(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(world.file.exists());
    world
        .run(
            &["auth", "login", "--with-token"],
            &piped(),
            &Env::default(),
        )
        .await
        .unwrap();
    assert!(
        !world.file.exists(),
        "the file's entry moved to the keyring"
    );
    assert!(world.keyring.text(&world.origin).is_some());
}

// --- status ------------------------------------------------------------------

#[tokio::test]
async fn status_reports_where_the_login_is_from_and_checks_it() {
    let world = world().await;
    world
        .run(
            &["auth", "login", "--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();

    let report = world
        .run(&["auth", "status"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    let object = report.value.as_object().unwrap();
    let mut keys: Vec<_> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "affiliatedCompanyId",
            "keyId",
            "origin",
            "path",
            "resolvedCompanyId",
            "source",
            "storedAt",
            "userId"
        ]
    );
    assert_eq!(object["origin"], world.origin.as_str());
    assert_eq!(object["source"], "file");
    assert_eq!(object["path"], world.file.display().to_string());
    assert_eq!(object["keyId"], KEY_ID);
    assert_eq!(object["userId"], USER_ID);
    assert_eq!(object["affiliatedCompanyId"], COMPANY_ID);
    assert_eq!(object["resolvedCompanyId"], COMPANY_ID);
    assert!(object["storedAt"].as_str().unwrap().ends_with('Z'));
    assert!(!shown(&report).contains(SECRET));
}

#[tokio::test]
async fn status_from_the_keyring_and_from_the_environment() {
    let world = world().await;
    world
        .run(
            &["auth", "login", "--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    let report = world
        .run(&["auth", "status"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(report.value["source"], "keyring");
    assert_eq!(report.value["path"], Value::Null);

    let environment = Env::default()
        .with("CAD_API_KEY", KEY_ID)
        .with("CAD_API_SECRET", SECRET);
    let report = world
        .run(&["auth", "status"], &Scripted::default(), &environment)
        .await
        .unwrap();
    assert_eq!(report.value["source"], "environment");
    assert_eq!(report.value["path"], Value::Null);
    assert_eq!(report.value["storedAt"], Value::Null);
    assert_eq!(report.value["keyId"], KEY_ID);
}

#[tokio::test]
async fn status_without_credentials_exits_two_and_a_rejected_key_exits_three() {
    let world = world().await;
    let error = world
        .run(&["auth", "status"], &Scripted::default(), &Env::default())
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)));
    assert_eq!(exit_code(&error), ExitCode::from(2));
    assert!(error.to_string().contains("cad-cli auth login"));
    assert_eq!(world.requests().await, 0);

    let environment = Env::default()
        .with("CAD_API_KEY", KEY_ID)
        .with("CAD_API_SECRET", "revoked");
    let error = world
        .run(&["auth", "status"], &Scripted::default(), &environment)
        .await
        .unwrap_err();
    assert_eq!(exit_code(&error), ExitCode::from(3));
}

#[tokio::test]
async fn a_login_is_not_used_for_another_origin() {
    let world = world().await;
    world
        .run(
            &["auth", "login", "--with-token"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();

    let elsewhere = format!(
        "http://localhost:{}",
        world.origin.rsplit(':').next().unwrap()
    );
    let cli = Cli::try_parse_from([
        "cad-cli",
        "--server-url",
        &elsewhere,
        "--allow-insecure-http",
        "auth",
        "status",
    ])
    .unwrap();
    let Command::Auth(group) = cli.command else {
        unreachable!()
    };
    let terminal = Scripted::default();
    let context = CommandContext::new(&terminal, false);
    let error = group
        .run(&context, &cli.global, &Env::default(), &world.store)
        .await
        .unwrap_err();
    assert!(matches!(error, CliError::AuthConfig(_)), "{error}");
    assert!(error.to_string().contains(&elsewhere));
    // The key was never sent to the other host: only the login's request.
    assert_eq!(world.requests().await, 1);
}

// --- logout ------------------------------------------------------------------

#[tokio::test]
async fn logout_removes_the_entry_from_both_places_without_connecting() {
    let world = world().await;
    world
        .run(
            &["auth", "login", "--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    world
        .keyring
        .put(&world.origin, &json!({"kind": "apiKey"}).to_string());
    let before = world.requests().await;

    let report = world
        .run(&["auth", "logout"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(
        report.value,
        json!({"origin": world.origin, "removed": ["keyring", "file"]})
    );
    assert_eq!(report.messages.len(), 1);
    assert_eq!(report.messages[0].0, StatusKind::Success);
    let text = &report.messages[0].1;
    assert!(text.contains("stays valid until it is revoked"), "{text}");
    assert!(text.contains(&page(&world.origin)), "{text}");
    assert!(!world.stored());
    assert_eq!(world.requests().await, before, "logout never connects");
}

#[tokio::test]
async fn logout_without_an_entry_warns_and_still_succeeds() {
    let world = world().await;
    let report = world
        .run(&["auth", "logout"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert_eq!(report.value, json!({"origin": world.origin, "removed": []}));
    assert_eq!(report.messages.len(), 1);
    assert_eq!(report.messages[0].0, StatusKind::Warning);
    assert!(report.messages[0].1.contains("nothing to remove"));
}

#[tokio::test]
async fn logout_leaves_other_origins_alone_and_warns_about_the_environment() {
    let world = world().await;
    world
        .run(
            &["auth", "login", "--with-token", "--insecure-storage"],
            &Scripted::piped(&token()),
            &Env::default(),
        )
        .await
        .unwrap();
    // A different origin's entry in the same file.
    let other = "https://staging.carbonaltdelete.eu";
    world
        .store
        .save_in_file(
            other,
            &crate::support::credentials::StoredCredential::api_key(
                Uuid::from_u128(9),
                SecretString::from(SECRET),
                Uuid::from_u128(10),
                chrono::Utc::now(),
            ),
        )
        .unwrap();

    let environment = Env::default().with("CAD_API_SECRET", "x");
    let report = world
        .run(&["auth", "logout"], &Scripted::default(), &environment)
        .await
        .unwrap();
    assert_eq!(report.value["removed"], json!(["file"]));
    assert_eq!(report.messages.len(), 2);
    assert_eq!(report.messages[1].0, StatusKind::Warning);
    assert!(report.messages[1].1.contains("CAD_API_KEY"));
    assert!(world.store.load(other).unwrap().credential.is_some());
}

#[tokio::test]
async fn logout_with_an_unreachable_keyring_says_so() {
    let world = world_with(MemoryBackend::unavailable(KEYRING_DOWN)).await;
    let report = world
        .run(&["auth", "logout"], &Scripted::default(), &Env::default())
        .await
        .unwrap();
    assert!(
        report
            .messages
            .iter()
            .any(|(kind, text)| *kind == StatusKind::Warning && text.contains(KEYRING_DOWN))
    );
}

#[tokio::test]
async fn no_command_needs_a_server_it_was_not_given() {
    let world = world().await;
    for rest in ["login", "status", "logout"] {
        let cli = Cli::try_parse_from(["cad-cli", "auth", rest]).unwrap();
        let Command::Auth(group) = cli.command else {
            unreachable!()
        };
        let terminal = Scripted::piped(&token());
        let context = CommandContext::new(&terminal, false);
        let error = group
            .run(&context, &cli.global, &Env::default(), &world.store)
            .await
            .unwrap_err();
        assert!(matches!(error, CliError::Config { .. }), "{rest}: {error}");
        assert!(
            error.to_string().contains("--server-url"),
            "{rest}: {error}"
        );
    }
}
