//! End-to-end coverage of the bootstrap: real argument parsing, real config
//! resolution, a real `cad-api` client and a loopback HTTP fixture. Only the
//! process terminal and the process environment are substituted.
//!
//! These stay beside the code because they drive the private `execute()` seam
//! with fakes. Black-box coverage of the shipped binary lives in `tests/cli.rs`.

use std::collections::HashMap;
use std::sync::Mutex;

use clap::CommandFactory;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{Cli, FromArgMatches, check_local_inputs, execute};
use crate::cli::OutputFormat;
use crate::config::Environment;
use crate::error::CliError;
use crate::output::TableStyle;
use crate::support::terminal::{Progress, StatusKind, Terminal};

const COMPANY_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0042);
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

#[derive(Default)]
struct FakeEnvironment(HashMap<String, String>);

impl Environment for FakeEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

fn credentials() -> FakeEnvironment {
    FakeEnvironment(HashMap::from([
        ("CAD_API_KEY".into(), Uuid::nil().to_string()),
        ("CAD_API_SECRET".into(), "secret".into()),
    ]))
}

#[derive(Default)]
struct FakeTerminal {
    stdout: Mutex<Vec<u8>>,
    status: Mutex<Vec<(StatusKind, String)>>,
    progress: Mutex<Vec<String>>,
    interactive: bool,
}

impl FakeTerminal {
    fn stdout(&self) -> String {
        String::from_utf8(self.stdout.lock().expect("stdout lock").clone())
            .expect("stdout is UTF-8")
    }

    fn statuses(&self) -> Vec<(StatusKind, String)> {
        self.status.lock().expect("status lock").clone()
    }
}

/// Records that an operation announced itself; there is nothing to draw.
struct RecordedProgress;

impl Progress for RecordedProgress {}

impl Terminal for FakeTerminal {
    fn status(&self, kind: StatusKind, message: &str) -> Result<(), CliError> {
        self.status
            .lock()
            .expect("status lock")
            .push((kind, message.to_owned()));
        Ok(())
    }

    fn confirm(&self, _prompt: &str, assume_yes: bool) -> Result<(), CliError> {
        if assume_yes {
            return Ok(());
        }
        if self.interactive {
            return Ok(());
        }
        Err(CliError::Input(
            "destructive operations outside a TTY require --yes".to_owned(),
        ))
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(Vec::new())
    }

    fn write_output(
        &self,
        value: &serde_json::Value,
        format: OutputFormat,
    ) -> Result<(), CliError> {
        let mut stdout = self.stdout.lock().expect("stdout lock");
        crate::output::write_styled(value, format, &mut *stdout, TableStyle::PLAIN)
    }

    fn write_bytes(&self, bytes: &[u8]) -> Result<(), CliError> {
        self.stdout
            .lock()
            .expect("stdout lock")
            .extend_from_slice(bytes);
        Ok(())
    }

    fn progress(&self, message: &str) -> Box<dyn Progress> {
        self.progress
            .lock()
            .expect("progress lock")
            .push(message.to_owned());
        Box::new(RecordedProgress)
    }
}

/// A loopback fixture that answers the machine identity endpoint and nothing else.
async fn mock_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": {
                "userId": Uuid::nil(),
                "affiliatedCompanyId": COMPANY_ID
            },
            "companySelection": {
                "resolvedCompanyId": COMPANY_ID,
                "source": "DEFAULT"
            }
        })))
        .mount(&server)
        .await;
    server
}

async fn request_paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .expect("the mock server records requests")
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

async fn run_cli(
    arguments: &[&str],
    environment: &FakeEnvironment,
    terminal: &FakeTerminal,
) -> Result<(), CliError> {
    let matches = Cli::command()
        .try_get_matches_from(arguments)
        .expect("arguments parse");
    let cli = Cli::from_arg_matches(&matches).expect("arguments map to Cli");
    execute(cli, &matches, environment, terminal).await
}

#[test]
fn local_input_check_walks_nested_subcommands() {
    let matches = |arguments: &[&str]| {
        Cli::command()
            .try_get_matches_from(arguments)
            .expect("arguments parse")
    };

    let error = check_local_inputs(&matches(&[
        "cad-cli",
        "report",
        "create",
        "--input",
        "definitely-missing.json",
    ]))
    .expect_err("a missing input file must fail");
    assert!(error.to_string().contains("definitely-missing.json"));

    let error = check_local_inputs(&matches(&[
        "cad-cli",
        "document",
        "upload",
        "--file",
        "missing-evidence.pdf",
    ]))
    .expect_err("a missing upload source must fail");
    assert!(error.to_string().contains("missing-evidence.pdf"));

    check_local_inputs(&matches(&["cad-cli", "report", "create", "--input", "-"]))
        .expect("stdin is always readable");
    check_local_inputs(&matches(&["cad-cli", "company", "list"]))
        .expect("a command without inputs has nothing to check");
}

#[tokio::test]
async fn company_list_renders_the_server_payload_as_json() {
    let server = mock_server().await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": COMPANY_ID,
                "name": "Acme",
                "displayName": "Acme NV",
                "createdAt": "2026-01-01T00:00:00Z",
                "isConsultingCompany": false,
                "isDemoCompany": false,
                "availableClientCompanies": null,
                "companyStatus": "ACTIVE"
            }])),
        )
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "company",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("company list should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {COMPANIES_PATH}")
        ]
    );

    let rendered: serde_json::Value =
        serde_json::from_str(&terminal.stdout()).expect("stdout is JSON");
    assert_eq!(rendered[0]["displayName"], "Acme NV");

    // The confirmation belongs to stderr; stdout above is still pure JSON.
    assert_eq!(
        terminal.statuses(),
        [(StatusKind::Success, "1 result".to_owned())]
    );
}

#[tokio::test]
async fn an_empty_collection_is_reported_as_a_warning() {
    let server = mock_server().await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "company",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("an empty list is not a failure");

    assert_eq!(
        terminal.statuses(),
        [(StatusKind::Warning, "no results".to_owned())]
    );
    assert_eq!(terminal.stdout().trim(), "[]");
}

#[tokio::test]
async fn missing_credentials_fail_before_any_request() {
    let terminal = FakeTerminal::default();
    let error = run_cli(
        &["cad-cli", "--environment", "staging", "company", "list"],
        &FakeEnvironment::default(),
        &terminal,
    )
    .await
    .expect_err("missing credentials must fail");

    assert!(matches!(error, CliError::AuthConfig(_)));
    assert!(terminal.stdout().is_empty());
}

#[tokio::test]
async fn delete_outside_a_tty_refuses_without_yes() {
    let server = mock_server().await;
    let terminal = FakeTerminal::default();

    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "custom-field",
            "delete",
            &Uuid::nil().to_string(),
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("a non-interactive delete must be refused");

    assert!(matches!(error, CliError::Input(_)));
    assert_eq!(
        request_paths(&server).await,
        [format!("GET {IDENTITY_PATH}")],
        "only the machine identity check may reach the server"
    );
    assert!(terminal.stdout().is_empty());
}

#[tokio::test]
async fn unreadable_input_fails_before_the_credential_exchange() {
    let terminal = FakeTerminal::default();
    let error = run_cli(
        &[
            "cad-cli",
            "--environment",
            "staging",
            "report",
            "create",
            "--input",
            "definitely-missing.json",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("a missing input file must fail");

    // No fixture server exists, so reaching the network would hang or connect-fail
    // instead of producing this error.
    assert!(matches!(error, CliError::Input(_)));
    assert!(error.to_string().contains("definitely-missing.json"));
}
