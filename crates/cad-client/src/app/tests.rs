//! End-to-end coverage of the bootstrap: real argument parsing, real config
//! resolution, a real `cad-api` client and a loopback HTTP fixture. Only the
//! process terminal and the process environment are substituted.
//!
//! These stay beside the code because they drive the private `execute()` seam
//! with fakes. Black-box coverage of the shipped binary lives in `tests/cli.rs`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;

use clap::CommandFactory;
use uuid::Uuid;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::{Cli, FromArgMatches, check_local_inputs, execute};
use crate::cli::OutputFormat;
use crate::config::Environment;
use crate::error::CliError;
use crate::output::TableStyle;
use crate::support::credentials::memory::empty_store;
use crate::support::profiles::{Profile, ProfileName, ProfileSet, ProfileStore};
use crate::support::terminal::{Progress, StatusKind, Terminal, Validator};

const COMPANY_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0042);
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";
const COMPANY_TEMPLATES_PATH: &str = "/api/accounts/v1/template-companies";
const BENCHMARKS_PATH: &str = "/api/benchmarks/v1/benchmarks";
const ACTIVITY_CATEGORIES_PATH: &str = "/api/activities/v1/activity-categories";
const ACTIVITY_GROUPS_PATH: &str = "/api/activities/v1/activity-groups";
const REPORTING_PERIODS_PATH: &str = "/api/reporting-periods/v1/reporting-periods";
const ORGANIZATIONAL_UNITS_PATH: &str = "/api/organizational-units/v1/organizational-units";

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
        // A command that acts on a company is refused without one.
        ("CAD_COMPANY_ID".into(), COMPANY_ID.to_string()),
    ]))
}

#[derive(Default)]
struct FakeTerminal {
    stdout: Mutex<Vec<u8>>,
    status: Mutex<Vec<(StatusKind, String)>>,
    /// Headings of the notes blocks written.
    notes: Mutex<Vec<String>>,
    progress: Mutex<Vec<String>>,
    confirmations: Mutex<Vec<String>>,
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

    fn confirmations(&self) -> Vec<String> {
        self.confirmations
            .lock()
            .expect("confirmation lock")
            .clone()
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

    fn notes(&self, heading: &str, _body: &str) -> Result<(), CliError> {
        self.notes
            .lock()
            .expect("notes lock")
            .push(heading.to_owned());
        Ok(())
    }

    fn confirm(&self, prompt: &str, assume_yes: bool) -> Result<(), CliError> {
        self.confirmations
            .lock()
            .expect("confirmation lock")
            .push(prompt.to_owned());
        if assume_yes {
            return Ok(());
        }
        if self.interactive {
            return Ok(());
        }
        Err(CliError::Input(
            "operations that require confirmation outside a TTY require --yes".to_owned(),
        ))
    }

    fn read_stdin(&self) -> Result<Vec<u8>, CliError> {
        Ok(Vec::new())
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }

    fn prompt(&self, _label: &str, _validate: Validator) -> Result<String, CliError> {
        Err(CliError::Input(
            "this fake terminal answers no prompts".to_owned(),
        ))
    }

    fn prompt_secret(
        &self,
        _label: &str,
        _validate: Validator,
    ) -> Result<secrecy::SecretString, CliError> {
        Err(CliError::Input(
            "this fake terminal answers no prompts".to_owned(),
        ))
    }

    fn select(&self, _label: &str, _options: &[String]) -> Result<usize, CliError> {
        Err(CliError::Input(
            "this fake terminal answers no prompts".to_owned(),
        ))
    }

    fn ask(&self, _question: &str) -> Result<bool, CliError> {
        Ok(false)
    }

    fn open_url(&self, _url: &str) -> bool {
        false
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
    run_cli_status(arguments, environment, terminal)
        .await
        .map(|status| assert_eq!(status, 0, "a platform command exits 0"))
}

/// Like [`run_cli`], for commands whose successful exit status varies.
async fn run_cli_status(
    arguments: &[&str],
    environment: &FakeEnvironment,
    terminal: &FakeTerminal,
) -> Result<u8, CliError> {
    let matches = Cli::command()
        .try_get_matches_from(arguments)
        .expect("arguments parse");
    let cli = Cli::from_arg_matches(&matches).expect("arguments map to Cli");
    execute(
        cli,
        &matches,
        environment,
        &empty_store(),
        &ProfileStore::new(None),
        terminal,
    )
    .await
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
async fn company_create_confirms_the_invoice_impact_before_posting() {
    let server = mock_server().await;
    let template_id = Uuid::from_u128(0x43);
    Mock::given(method("POST"))
        .and(path(COMPANIES_PATH))
        .and(body_json(serde_json::json!({
            "name": "Acme GmbH",
            "template": "TEMPLATE_COMPANY",
            "companyTemplateId": template_id
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": COMPANY_ID,
            "name": "Acme GmbH",
            "displayName": "Acme GmbH",
            "createdAt": "2026-01-01T00:00:00Z",
            "isConsultingCompany": false,
            "isDemoCompany": false,
            "availableClientCompanies": null,
            "companyStatus": "ACTIVE"
        })))
        .mount(&server)
        .await;
    let mut input = tempfile::NamedTempFile::new().expect("temporary company input");
    writeln!(
        input,
        "{{\"name\":\"Acme GmbH\",\"template\":\"TEMPLATE_COMPANY\",\"companyTemplateId\":\"{template_id}\"}}"
    )
    .expect("write company input");
    let input_path = input.path().to_str().expect("UTF-8 input path");
    let terminal = FakeTerminal {
        interactive: true,
        ..Default::default()
    };

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "company",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("confirmed company creation should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("POST {COMPANIES_PATH}")
        ]
    );
    assert_eq!(
        terminal.confirmations(),
        ["Creating a company may affect your next invoice. Continue?"]
    );
    let rendered: serde_json::Value =
        serde_json::from_str(&terminal.stdout()).expect("stdout is JSON");
    assert_eq!(rendered["name"], "Acme GmbH");
}

#[tokio::test]
async fn company_create_outside_a_tty_requires_yes_before_posting() {
    let server = mock_server().await;
    let mut input = tempfile::NamedTempFile::new().expect("temporary company input");
    writeln!(input, "{{\"name\":\"Acme GmbH\"}}").expect("write company input");
    let input_path = input.path().to_str().expect("UTF-8 input path");
    let terminal = FakeTerminal::default();

    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "company",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("non-interactive company creation needs --yes");

    assert!(error.to_string().contains("require confirmation"));
    assert_eq!(
        request_paths(&server).await,
        [format!("GET {IDENTITY_PATH}")]
    );
}

#[tokio::test]
async fn company_create_with_yes_posts_without_an_interactive_terminal() {
    let server = mock_server().await;
    Mock::given(method("POST"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": COMPANY_ID,
            "name": "Acme GmbH",
            "displayName": "Acme GmbH",
            "createdAt": "2026-01-01T00:00:00Z",
            "isConsultingCompany": false,
            "isDemoCompany": false,
            "availableClientCompanies": null,
            "companyStatus": "ACTIVE"
        })))
        .mount(&server)
        .await;
    let mut input = tempfile::NamedTempFile::new().expect("temporary company input");
    writeln!(input, "{{\"name\":\"Acme GmbH\"}}").expect("write company input");
    let input_path = input.path().to_str().expect("UTF-8 input path");
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--yes",
            "company",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("--yes should permit non-interactive company creation");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("POST {COMPANIES_PATH}")
        ]
    );
}

#[tokio::test]
async fn company_template_list_renders_the_server_payload_as_json() {
    let server = mock_server().await;
    let template_id = Uuid::from_u128(0x43);
    Mock::given(method("GET"))
        .and(path(COMPANY_TEMPLATES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": template_id,
                "name": "Consulting template",
                "displayName": "Consulting template",
                "createdAt": "2026-01-01T00:00:00Z",
                "isConsultingCompany": true,
                "isDemoCompany": false,
                "availableClientCompanies": null,
                "companyStatus": "ACTIVE",
                "templateDescription": "Prepared consulting configuration"
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
            "company-template",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("company template list should succeed");

    let rendered: serde_json::Value =
        serde_json::from_str(&terminal.stdout()).expect("stdout is JSON");
    assert_eq!(rendered[0]["id"], template_id.to_string());
}

#[tokio::test]
async fn benchmark_read_hides_the_singleton_collection_wrapper() {
    let server = mock_server().await;
    let benchmark_id = Uuid::from_u128(7);
    let reporting_period_id = Uuid::from_u128(8);
    let revenue_unit_id = Uuid::from_u128(9);
    Mock::given(method("GET"))
        .and(path(BENCHMARKS_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": benchmark_id,
                "reportingPeriodId": reporting_period_id,
                "fullTimeEmployees": 42.0,
                "grossAnnualRevenue": null,
                "grossAnnualRevenueUnitId": revenue_unit_id,
                "highLevelSectorCode": "C",
                "lowLevelSectorCode": null,
                "location": "DENMARK",
                "sectorGroup": "INDUSTRY"
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
            "benchmark",
            "read",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("benchmark read should succeed");

    let rendered: serde_json::Value =
        serde_json::from_str(&terminal.stdout()).expect("stdout is JSON");
    assert!(rendered.is_object(), "the CLI must not emit the API array");
    assert_eq!(rendered["id"], benchmark_id.to_string());
    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {BENCHMARKS_PATH}")
        ]
    );
    assert_eq!(
        terminal.statuses(),
        [(StatusKind::Success, "1 result".to_owned())]
    );
}

#[tokio::test]
async fn benchmark_update_uses_the_singleton_id_in_its_url_and_body() {
    let server = mock_server().await;
    let benchmark_id = Uuid::from_u128(7);
    let reporting_period_id = Uuid::from_u128(8);
    let revenue_unit_id = Uuid::from_u128(9);
    let benchmark = serde_json::json!({
        "id": benchmark_id,
        "reportingPeriodId": reporting_period_id,
        "fullTimeEmployees": 42.0,
        "grossAnnualRevenue": null,
        "grossAnnualRevenueUnitId": revenue_unit_id,
        "highLevelSectorCode": "C",
        "lowLevelSectorCode": null,
        "location": "DENMARK",
        "sectorGroup": "INDUSTRY"
    });
    Mock::given(method("GET"))
        .and(path(BENCHMARKS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([benchmark])))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{BENCHMARKS_PATH}/{benchmark_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": benchmark_id,
            "reportingPeriodId": reporting_period_id,
            "fullTimeEmployees": 43.0,
            "grossAnnualRevenue": null,
            "grossAnnualRevenueUnitId": revenue_unit_id,
            "highLevelSectorCode": "C",
            "lowLevelSectorCode": null,
            "location": "DENMARK",
            "sectorGroup": "INDUSTRY"
        })))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("temporary input directory");
    let input_path = directory.path().join("benchmark.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(&serde_json::json!({
            "reportingPeriodId": reporting_period_id,
            "fullTimeEmployees": 43.0,
            "grossAnnualRevenue": null,
            "grossAnnualRevenueUnitId": revenue_unit_id,
            "highLevelSectorCode": "C",
            "lowLevelSectorCode": null,
            "location": "DENMARK"
        }))
        .expect("serializable benchmark input"),
    )
    .expect("benchmark input written");
    let input_path = input_path.to_str().expect("UTF-8 temporary path");
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "benchmark",
            "update",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("benchmark update should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {BENCHMARKS_PATH}"),
            format!("PUT {BENCHMARKS_PATH}/{benchmark_id}")
        ]
    );
    let requests = server
        .received_requests()
        .await
        .expect("mock server records requests");
    let put = requests
        .iter()
        .find(|request| request.url.path() == format!("{BENCHMARKS_PATH}/{benchmark_id}"))
        .expect("benchmark update request");
    let body: serde_json::Value =
        serde_json::from_slice(&put.body).expect("benchmark update body is JSON");
    assert_eq!(body["id"], benchmark_id.to_string());
    assert_eq!(body["fullTimeEmployees"], 43.0);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal.stdout()).expect("stdout is JSON")["id"],
        benchmark_id.to_string()
    );
}

#[tokio::test]
async fn reporting_period_create_serializes_input_and_returns_the_created_period() {
    let server = mock_server().await;
    let reporting_period_id = Uuid::from_u128(9);
    Mock::given(method("POST"))
        .and(path(REPORTING_PERIODS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": reporting_period_id,
            "description": "Financial year 2026",
            "shortName": "FY26",
            "startDate": "2026-01-01",
            "endDate": "2026-12-31",
            "isLocked": false
        })))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("temporary input directory");
    let input_path = directory.path().join("reporting-period.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(&serde_json::json!({
            "description": "Financial year 2026",
            "shortName": "FY26",
            "startDate": "2026-01-01",
            "endDate": "2026-12-31",
            "isLocked": false
        }))
        .expect("serializable reporting-period input"),
    )
    .expect("reporting-period input written");
    let input_path = input_path.to_str().expect("UTF-8 temporary path");
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "reporting-period",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("reporting-period creation should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("POST {REPORTING_PERIODS_PATH}")
        ]
    );
    let requests = server
        .received_requests()
        .await
        .expect("mock server records requests");
    let post = requests
        .iter()
        .find(|request| request.url.path() == REPORTING_PERIODS_PATH)
        .expect("reporting-period create request");
    let body: serde_json::Value = serde_json::from_slice(&post.body).expect("create body is JSON");
    assert_eq!(body["shortName"], "FY26");
    assert!(body.get("id").is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal.stdout()).expect("stdout is JSON")["id"],
        reporting_period_id.to_string()
    );
}

#[tokio::test]
async fn activity_category_update_serializes_the_path_id_and_returns_the_updated_category() {
    let server = mock_server().await;
    let activity_category_id = Uuid::from_u128(10);
    let activity_group_id = Uuid::from_u128(11);
    Mock::given(method("PUT"))
        .and(path(format!(
            "{ACTIVITY_CATEGORIES_PATH}/{activity_category_id}"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": activity_category_id,
            "name": "Purchased Electricity",
            "description": "Electricity purchased for operations.",
            "inclusionCriteria": "Includes purchased electricity.",
            "exclusionCriteria": "Excludes onsite generation.",
            "activityCategoryType": "PURCHASED_ELECTRICITY",
            "activityGroupId": activity_group_id,
            "position": 0,
            "isIncluded": true,
            "isDefault": false,
            "ghgScopeType": "SCOPE_2",
            "countMeasurements": 0,
            "countActiveMeasurements": 0,
            "countDraftMeasurements": 0,
            "ghg_protocol_reference": "Scope 2",
            "iso_14064_reference": "6.3.1"
        })))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("temporary input directory");
    let input_path = directory.path().join("activity-category.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(&serde_json::json!({
            "name": "Purchased Electricity",
            "description": "Electricity purchased for operations.",
            "inclusionCriteria": "Includes purchased electricity.",
            "exclusionCriteria": "Excludes onsite generation.",
            "activityCategoryType": "PURCHASED_ELECTRICITY",
            "activityGroupName": "Scope 2",
            "activityGroupId": activity_group_id,
            "position": 0,
            "isIncluded": true
        }))
        .expect("serializable activity-category input"),
    )
    .expect("activity-category input written");
    let input_path = input_path.to_str().expect("UTF-8 temporary path");
    let terminal = FakeTerminal::default();
    let category_id = activity_category_id.to_string();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "activity-category",
            "update",
            &category_id,
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("activity-category update should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("PUT {ACTIVITY_CATEGORIES_PATH}/{activity_category_id}")
        ]
    );
    let requests = server
        .received_requests()
        .await
        .expect("mock server records requests");
    let update = requests
        .iter()
        .find(|request| {
            request.url.path() == format!("{ACTIVITY_CATEGORIES_PATH}/{activity_category_id}")
        })
        .expect("activity-category update request");
    let body: serde_json::Value =
        serde_json::from_slice(&update.body).expect("update body is JSON");
    assert_eq!(body["id"], activity_category_id.to_string());
    assert_eq!(body["activityCategoryType"], "PURCHASED_ELECTRICITY");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal.stdout()).expect("stdout is JSON")["id"],
        activity_category_id.to_string()
    );
}

#[tokio::test]
async fn activity_group_create_serializes_input_and_returns_the_created_group() {
    let server = mock_server().await;
    let activity_group_id = Uuid::from_u128(10);
    Mock::given(method("POST"))
        .and(path(ACTIVITY_GROUPS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": activity_group_id,
            "position": 0,
            "name": "Scope 1",
            "description": "Direct emissions"
        })))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("temporary input directory");
    let input_path = directory.path().join("activity-group.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(&serde_json::json!({
            "position": 0,
            "name": "Scope 1",
            "description": "Direct emissions"
        }))
        .expect("serializable activity-group input"),
    )
    .expect("activity-group input written");
    let input_path = input_path.to_str().expect("UTF-8 temporary path");
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "activity-group",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("activity-group creation should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("POST {ACTIVITY_GROUPS_PATH}")
        ]
    );
    let requests = server
        .received_requests()
        .await
        .expect("mock server records requests");
    let post = requests
        .iter()
        .find(|request| request.url.path() == ACTIVITY_GROUPS_PATH)
        .expect("activity-group create request");
    let body: serde_json::Value = serde_json::from_slice(&post.body).expect("create body is JSON");
    assert_eq!(body["name"], "Scope 1");
    assert!(body.get("id").is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal.stdout()).expect("stdout is JSON")["id"],
        activity_group_id.to_string()
    );
}

#[tokio::test]
async fn organizational_unit_create_serializes_input_and_returns_the_created_unit() {
    let server = mock_server().await;
    let parent_id = Uuid::from_u128(10);
    let organizational_unit_id = Uuid::from_u128(11);
    Mock::given(method("POST"))
        .and(path(ORGANIZATIONAL_UNITS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": organizational_unit_id,
            "name": "Operations",
            "position": 0,
            "parentId": parent_id
        })))
        .mount(&server)
        .await;
    let directory = tempfile::tempdir().expect("temporary input directory");
    let input_path = directory.path().join("organizational-unit.json");
    std::fs::write(
        &input_path,
        serde_json::to_vec(&serde_json::json!({
            "name": "Operations",
            "position": 0,
            "parentId": parent_id
        }))
        .expect("serializable organizational-unit input"),
    )
    .expect("organizational-unit input written");
    let input_path = input_path.to_str().expect("UTF-8 temporary path");
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "organizational-unit",
            "create",
            "--input",
            input_path,
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("organizational-unit creation should succeed");

    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("POST {ORGANIZATIONAL_UNITS_PATH}")
        ]
    );
    let requests = server
        .received_requests()
        .await
        .expect("mock server records requests");
    let post = requests
        .iter()
        .find(|request| request.url.path() == ORGANIZATIONAL_UNITS_PATH)
        .expect("organizational-unit create request");
    let body: serde_json::Value = serde_json::from_slice(&post.body).expect("create body is JSON");
    assert_eq!(body["parentId"], parent_id.to_string());
    assert!(body.get("id").is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&terminal.stdout()).expect("stdout is JSON")["id"],
        organizational_unit_id.to_string()
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

/// Mounts an identity endpoint that resolves the company named by
/// `X-CAD-Company-Id`, as the platform does for an accessible company.
async fn company_selecting_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(|request: &wiremock::Request| {
            let selected = request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok())
                .map_or_else(|| Uuid::from_u128(1).to_string(), str::to_owned);
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "principal": {
                    "userId": Uuid::nil(),
                    "affiliatedCompanyId": COMPANY_ID
                },
                "companySelection": {
                    "resolvedCompanyId": selected,
                    "source": "HEADER"
                }
            }))
        })
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_company_scoped_command_with_company_id_makes_one_identity_request() {
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
                "source": "HEADER"
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ACTIVITY_GROUPS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--company-id",
            &COMPANY_ID.to_string(),
            "--output",
            "json",
            "activity-group",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect("a scoped command with --company-id should succeed");

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok()),
            Some(COMPANY_ID.to_string().as_str()),
            "{} {}",
            request.method,
            request.url.path()
        );
    }
}

#[tokio::test]
async fn a_session_level_command_with_company_id_sends_no_company_header_on_its_own_request() {
    let server = company_selecting_server().await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--company-id",
            &COMPANY_ID.to_string(),
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

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {COMPANIES_PATH}")
        ]
    );
    assert!(requests[0].headers.contains_key("X-CAD-Company-Id"));
    assert!(
        !requests[1].headers.contains_key("X-CAD-Company-Id"),
        "the session-level request carries no company"
    );
}

#[tokio::test]
async fn a_rejected_key_with_company_id_exits_with_the_authentication_code() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"msg": "no"})))
        .expect(1)
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--company-id",
            &COMPANY_ID.to_string(),
            "activity-group",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("a rejected key must fail");

    assert!(
        matches!(error, CliError::Authentication { .. }),
        "{error:?}"
    );
    assert_eq!(error.exit_code(), std::process::ExitCode::from(3));
}

#[tokio::test]
async fn a_company_refused_with_a_403_with_company_id_exits_with_the_api_code() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({"msg": "no"})))
        .expect(1)
        .mount(&server)
        .await;
    let terminal = FakeTerminal::default();

    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--company-id",
            &COMPANY_ID.to_string(),
            "activity-group",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("a refused company must fail");

    assert!(matches!(error, CliError::Api { .. }), "{error:?}");
    assert_eq!(error.exit_code(), std::process::ExitCode::from(4));
    assert!(error.to_string().contains("HTTP 403"));
}

#[tokio::test]
async fn a_company_the_platform_resolves_differently_fails_as_an_api_error() {
    // The identity answers for its own company, whatever was requested.
    let server = mock_server().await;
    let other = Uuid::from_u128(0x99);
    let terminal = FakeTerminal::default();

    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--company-id",
            &other.to_string(),
            "activity-group",
            "list",
        ],
        &credentials(),
        &terminal,
    )
    .await
    .expect_err("an unresolved company must fail");

    assert!(matches!(error, CliError::Api { .. }), "{error:?}");
    assert_eq!(error.exit_code(), std::process::ExitCode::from(4));
    assert!(error.to_string().contains(&other.to_string()));
    assert_eq!(
        request_paths(&server).await,
        [format!("GET {IDENTITY_PATH}")]
    );
}

#[tokio::test]
async fn missing_credentials_fail_before_any_request() {
    let terminal = FakeTerminal::default();
    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            "https://staging.carbonaltdelete.eu",
            "company",
            "list",
        ],
        &FakeEnvironment::default(),
        &terminal,
    )
    .await
    .expect_err("missing credentials must fail");

    assert!(matches!(error, CliError::AuthConfig { .. }));
    assert!(
        error
            .to_string()
            .contains("no credentials for https://staging.carbonaltdelete.eu"),
        "{error}"
    );
    assert_eq!(terminal.stdout(), "");
}

#[tokio::test]
async fn without_a_profile_or_a_server_a_command_says_how_to_choose_one() {
    let terminal = FakeTerminal::default();
    let error = run_cli(&["cad-cli", "company", "list"], &credentials(), &terminal)
        .await
        .expect_err("no profile and no server must fail");

    assert!(matches!(error, CliError::Config { .. }), "{error:?}");
    assert_eq!(error.exit_code(), std::process::ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        "configuration error: no profile is selected; run `cad-cli use profile <name>` \
         (production, staging, dev, or a name of your own), pass --profile, or set \
         CAD_SERVER_URL with CAD_API_KEY and CAD_API_SECRET"
    );
    assert_eq!(terminal.stdout(), "");
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
    assert_eq!(terminal.stdout(), "");
}

#[tokio::test]
async fn unreadable_input_fails_before_the_identity_request() {
    let terminal = FakeTerminal::default();
    let error = run_cli(
        &[
            "cad-cli",
            "--server-url",
            "https://staging.carbonaltdelete.eu",
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

#[tokio::test]
async fn update_check_needs_no_platform_configuration_and_renders_its_result() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "tag_name": "v99.0.0",
            "assets": [],
        })))
        .expect(1)
        .mount(&server)
        .await;
    // Deliberately no CAD credentials or server: `update` must not resolve them.
    let environment = FakeEnvironment(HashMap::from([(
        crate::exec::update::URL_VARIABLE.to_owned(),
        format!("{}/releases/latest", server.uri()),
    )]));
    let terminal = FakeTerminal::default();

    let status = run_cli_status(
        &["cad-cli", "--output", "json", "update", "--check"],
        &environment,
        &terminal,
    )
    .await
    .expect("the check succeeds");

    assert_eq!(status, crate::cmds::update::UPDATE_AVAILABLE_STATUS);
    let stdout: serde_json::Value =
        serde_json::from_str(&terminal.stdout()).expect("stdout is JSON");
    assert_eq!(stdout["latest"], "99.0.0");
    assert_eq!(stdout["current"], env!("CARGO_PKG_VERSION"));
    assert_eq!(stdout["updateAvailable"], true);
    assert_eq!(stdout["updated"], false);
    let statuses = terminal.statuses();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].0, StatusKind::Info);
    assert!(statuses[0].1.starts_with("cad-cli 99.0.0 is available"));
}

/// A credentials file holding a login for the profile `work`, which is the
/// active profile and whose server is `origin`, all in `dir`.
fn store_with_login(
    origin: &str,
    dir: &std::path::Path,
) -> (crate::support::credentials::CredentialStore, ProfileStore) {
    use crate::support::credentials::{CredentialStore, FileBackend, StoredCredential};

    let store = CredentialStore::new(
        None,
        Some(FileBackend::new(
            dir.join("cad-cli").join("credentials.json"),
        )),
    );
    let work: ProfileName = "work".parse().unwrap();
    store
        .save_in_file(
            &work,
            &StoredCredential::api_key(
                origin.to_owned(),
                Uuid::from_u128(0xaa),
                secrecy::SecretString::from("cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde"),
                Uuid::nil().into(),
                chrono::Utc::now(),
            ),
        )
        .unwrap();
    let profiles = ProfileStore::new(Some(dir.join("cad-cli").join("profiles.json")));
    let mut set = ProfileSet::default();
    set.insert(
        work.clone(),
        Profile {
            url: origin.to_owned(),
            allow_insecure_http: true,
            login: None,
            company: None,
        },
    );
    set.activate(work);
    profiles.save(&set).unwrap();
    (store, profiles)
}

async fn run_with_store(
    arguments: &[&str],
    environment: &FakeEnvironment,
    store: &crate::support::credentials::CredentialStore,
    profiles: &ProfileStore,
    terminal: &FakeTerminal,
) -> Result<u8, CliError> {
    let matches = Cli::command()
        .try_get_matches_from(arguments)
        .expect("arguments parse");
    let cli = Cli::from_arg_matches(&matches).expect("arguments map to Cli");
    execute(cli, &matches, environment, store, profiles, terminal).await
}

fn bearer(request: &wiremock::Request) -> String {
    request
        .headers
        .get("authorization")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

const STORED_BEARER: &str = "Bearer 00000000-0000-0000-0000-0000000000aa.cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";

#[tokio::test]
async fn a_command_runs_on_the_active_profiles_server_with_its_stored_login() {
    let server = mock_server().await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());
    let terminal = FakeTerminal::default();

    // No server, no profile and no credentials on the command line or in the
    // environment: the active profile supplies all three.
    let status = run_with_store(
        &["cad-cli", "--output", "json", "company", "list"],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &terminal,
    )
    .await
    .expect("the active profile is enough");

    assert_eq!(status, 0);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {COMPANIES_PATH}")
        ]
    );
    assert!(
        requests
            .iter()
            .all(|request| bearer(request) == STORED_BEARER)
    );
}

#[tokio::test]
async fn the_environment_beats_the_stored_login() {
    let server = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());

    run_with_store(
        &["cad-cli", "--output", "json", "use", "--check"],
        &credentials(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect("the environment's key is used");

    // The identity request, then the account's e-mail.
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(bearer(request), format!("Bearer {}.secret", Uuid::nil()));
    }
}

#[tokio::test]
async fn a_server_given_by_flag_selects_the_profile_whose_server_it_is() {
    let server = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());

    // Make another profile the active one: only the server picks `work`.
    let mut set = profiles.load().unwrap();
    set.insert(
        "other".parse().unwrap(),
        Profile {
            url: "https://other.example.test".to_owned(),
            allow_insecure_http: false,
            login: None,
            company: None,
        },
    );
    set.activate("other".parse().unwrap());
    profiles.save(&set).unwrap();

    run_with_store(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--output",
            "json",
            "use",
            "--check",
        ],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect("the profile whose server it is supplies the login and the opt-in");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| bearer(request) == STORED_BEARER)
    );
}

#[tokio::test]
async fn a_foreign_server_never_receives_the_active_profiles_key() {
    let server = mock_server().await;
    let foreign = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());

    // `CAD_SERVER_URL` names another server while `work` is active.
    let environment = FakeEnvironment(HashMap::from([(
        "CAD_SERVER_URL".to_owned(),
        foreign.uri(),
    )]));
    let error = run_with_store(
        &["cad-cli", "--allow-insecure-http", "use", "--check"],
        &environment,
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect_err("no stored login may be used for a server that is not the profile's");
    assert!(matches!(error, CliError::AuthConfig { .. }), "{error}");
    assert!(
        error.to_string().contains(&format!(
            "no credentials for {}; set CAD_API_KEY and CAD_API_SECRET, or create a profile \
             with `cad-cli use profile <name> --url {}`",
            foreign.uri(),
            foreign.uri()
        )),
        "{error}"
    );
    assert!(foreign.received_requests().await.unwrap().is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());

    // An explicit profile together with a foreign server fails outright.
    let error = run_with_store(
        &[
            "cad-cli",
            "--profile",
            "work",
            "--server-url",
            &foreign.uri(),
            "--allow-insecure-http",
            "use",
            "--check",
        ],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect_err("an explicit profile logs in to its own server only");
    assert_eq!(
        error.to_string(),
        format!(
            "configuration error: profile work logs in to {}, not {}",
            server.uri(),
            foreign.uri()
        )
    );
    assert!(foreign.received_requests().await.unwrap().is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_profile_without_a_login_fails_before_a_request_naming_the_way_out() {
    let server = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (_, profiles) = store_with_login(&server.uri(), dir.path());
    let store = crate::support::credentials::memory::empty_store();

    let error = run_with_store(
        &["cad-cli", "use", "--check"],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect_err("no credentials anywhere");

    assert!(matches!(error, CliError::AuthConfig { .. }));
    assert_eq!(error.exit_code(), std::process::ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        "authentication configuration error: profile work has no login; run `cad-cli use \
         profile work --login`, or set CAD_API_KEY and CAD_API_SECRET"
    );
    assert_eq!(request_paths(&server).await, Vec::<String>::new());
}

/// Gives the profile `work` a company.
fn choose_company(profiles: &ProfileStore, company: Uuid) {
    let mut set = profiles.load().unwrap();
    let work: ProfileName = "work".parse().unwrap();
    let mut profile = set.get(&work).unwrap().clone();
    profile.company = Some(crate::support::profiles::Company {
        id: company.into(),
        name: Some("Acme GmbH".to_owned()),
    });
    set.insert(work, profile);
    profiles.save(&set).unwrap();
}

#[tokio::test]
async fn a_company_scoped_command_without_a_company_is_refused_before_any_request() {
    let server = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());

    // With a profile in use, the error names it.
    let error = run_with_store(
        &["cad-cli", "activity-group", "list"],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect_err("the platform's own choice of company is not used");
    assert!(matches!(error, CliError::Config { .. }), "{error:?}");
    assert_eq!(error.exit_code(), std::process::ExitCode::from(2));
    assert_eq!(
        error.to_string(),
        "configuration error: no company is selected for profile work; run `cad-cli use \
         company <name|id>`, or pass --company-id or set CAD_COMPANY_ID"
    );

    // With no profile, the error names the flag and the variable only.
    let environment = FakeEnvironment(HashMap::from([
        ("CAD_API_KEY".into(), Uuid::nil().to_string()),
        ("CAD_API_SECRET".into(), "secret".into()),
    ]));
    let error = run_with_store(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--profile",
            "work",
            "reporting-period",
            "list",
        ],
        &environment,
        &store,
        &ProfileStore::new(None),
        &FakeTerminal::default(),
    )
    .await
    .expect_err("there is no profile in a store that has none");
    assert!(error.to_string().contains("there is no profile work"));
    let error = run_with_store(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "reporting-period",
            "list",
        ],
        &environment,
        &store,
        &ProfileStore::new(None),
        &FakeTerminal::default(),
    )
    .await
    .expect_err("a server without a profile has no company either");
    assert_eq!(
        error.to_string(),
        "configuration error: no company is selected; pass --company-id or set CAD_COMPANY_ID"
    );

    assert_eq!(
        request_paths(&server).await,
        Vec::<String>::new(),
        "every refusal happens before the connection"
    );
}

#[tokio::test]
async fn a_session_level_command_needs_no_company() {
    let server = mock_server().await;
    for (path_, group) in [
        (COMPANIES_PATH, "company"),
        (COMPANY_TEMPLATES_PATH, "company-template"),
    ] {
        Mock::given(method("GET"))
            .and(path(path_))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let (store, profiles) = store_with_login(&server.uri(), dir.path());
        run_with_store(
            &["cad-cli", "--output", "json", group, "list"],
            &FakeEnvironment::default(),
            &store,
            &profiles,
            &FakeTerminal::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("{group} list: {error}"));
    }
    for request in server.received_requests().await.unwrap() {
        assert!(
            !request.headers.contains_key("X-CAD-Company-Id"),
            "{}",
            request.url.path()
        );
    }
}

#[tokio::test]
async fn the_profiles_company_is_sent_in_the_one_identity_request_and_the_flag_beats_it() {
    let server = company_selecting_server().await;
    Mock::given(method("GET"))
        .and(path(ACTIVITY_GROUPS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login(&server.uri(), dir.path());
    choose_company(&profiles, COMPANY_ID);

    run_with_store(
        &["cad-cli", "--output", "json", "activity-group", "list"],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect("the profile's company is enough");
    assert_eq!(
        request_paths(&server).await,
        [
            format!("GET {IDENTITY_PATH}"),
            format!("GET {ACTIVITY_GROUPS_PATH}")
        ]
    );
    for request in server.received_requests().await.unwrap() {
        assert_eq!(
            request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok()),
            Some(COMPANY_ID.to_string().as_str())
        );
    }

    let other = Uuid::from_u128(0x77);
    run_with_store(
        &[
            "cad-cli",
            "--company-id",
            &other.to_string(),
            "--output",
            "json",
            "activity-group",
            "list",
        ],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect("the flag beats the profile");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 4);
    for request in &requests[2..] {
        assert_eq!(
            request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok()),
            Some(other.to_string().as_str())
        );
    }
}

#[tokio::test]
async fn use_writes_its_data_to_stdout_and_its_lines_to_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let origin = "http://127.0.0.1:1";
    let (store, profiles) = store_with_login(origin, dir.path());
    let terminal = FakeTerminal::default();

    let status = run_with_store(
        &[
            "cad-cli", "--output", "json", "use", "profile", "work", "--forget",
        ],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &terminal,
    )
    .await
    .expect("forgetting needs no server");

    assert_eq!(status, 0);
    let stdout: serde_json::Value = serde_json::from_str(&terminal.stdout()).unwrap();
    assert_eq!(
        stdout,
        serde_json::json!({"profile": "work", "removed": ["file"], "wasActive": true})
    );
    let statuses = terminal.statuses();
    assert_eq!(statuses[0].0, StatusKind::Success);
    assert!(statuses[0].1.contains("forgot profile work"));
    assert!(!dir.path().join("cad-cli").join("credentials.json").exists());
    assert!(profiles.load().unwrap().is_empty());
}

#[tokio::test]
async fn a_profile_not_in_the_file_fails_naming_the_command_that_creates_it() {
    let dir = tempfile::tempdir().unwrap();
    let (store, profiles) = store_with_login("http://127.0.0.1:1", dir.path());
    let error = run_with_store(
        &["cad-cli", "--profile", "nope", "company", "list"],
        &FakeEnvironment::default(),
        &store,
        &profiles,
        &FakeTerminal::default(),
    )
    .await
    .expect_err("the profile does not exist");
    assert_eq!(
        error.to_string(),
        "configuration error: there is no profile nope; create it with `cad-cli use profile nope`"
    );
}
