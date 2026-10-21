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
    execute(cli, &matches, environment, &empty_store(), terminal).await
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

/// A store whose credentials file holds a login for `origin`, in `dir`.
fn store_with_login(
    origin: &str,
    dir: &std::path::Path,
) -> crate::support::credentials::CredentialStore {
    use crate::support::credentials::{CredentialStore, FileBackend, StoredCredential};

    let store = CredentialStore::new(
        None,
        Some(FileBackend::new(
            dir.join("cad-cli").join("credentials.json"),
        )),
    );
    store
        .save_in_file(
            origin,
            &StoredCredential::api_key(
                Uuid::from_u128(0xaa),
                secrecy::SecretString::from("cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde"),
                Uuid::nil(),
                chrono::Utc::now(),
            ),
        )
        .unwrap();
    store
}

async fn run_with_store(
    arguments: &[&str],
    store: &crate::support::credentials::CredentialStore,
    terminal: &FakeTerminal,
) -> Result<u8, CliError> {
    let matches = Cli::command()
        .try_get_matches_from(arguments)
        .expect("arguments parse");
    let cli = Cli::from_arg_matches(&matches).expect("arguments map to Cli");
    execute(cli, &matches, &FakeEnvironment::default(), store, terminal).await
}

#[tokio::test]
async fn an_online_command_authenticates_with_the_stored_login() {
    let server = mock_server().await;
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_login(&server.uri(), dir.path());
    let terminal = FakeTerminal::default();

    let status = run_with_store(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "--output",
            "json",
            "context",
            "show",
        ],
        &store,
        &terminal,
    )
    .await
    .expect("the stored login is enough");

    assert_eq!(status, 0);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        format!(
            "Bearer {}.cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde",
            Uuid::from_u128(0xaa)
        )
    );
}

#[tokio::test]
async fn an_online_command_without_any_login_fails_before_a_request() {
    let server = mock_server().await;
    let store = crate::support::credentials::memory::empty_store();
    let terminal = FakeTerminal::default();

    let error = run_with_store(
        &[
            "cad-cli",
            "--server-url",
            &server.uri(),
            "--allow-insecure-http",
            "context",
            "show",
        ],
        &store,
        &terminal,
    )
    .await
    .expect_err("no credentials anywhere");

    assert!(matches!(error, CliError::AuthConfig(_)));
    assert!(error.to_string().contains("cad-cli auth login"));
    assert_eq!(request_paths(&server).await, Vec::<String>::new());
}

#[tokio::test]
async fn auth_commands_write_their_data_to_stdout_and_their_lines_to_stderr() {
    let dir = tempfile::tempdir().unwrap();
    let origin = "http://127.0.0.1:1";
    let store = store_with_login(origin, dir.path());
    let terminal = FakeTerminal::default();

    let status = run_with_store(
        &[
            "cad-cli",
            "--server-url",
            origin,
            "--allow-insecure-http",
            "--output",
            "json",
            "auth",
            "logout",
        ],
        &store,
        &terminal,
    )
    .await
    .expect("logout needs no server");

    assert_eq!(status, 0);
    let stdout: serde_json::Value = serde_json::from_str(&terminal.stdout()).unwrap();
    assert_eq!(
        stdout,
        serde_json::json!({"origin": origin, "removed": ["file"]})
    );
    let statuses = terminal.statuses();
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].0, StatusKind::Success);
    assert!(statuses[0].1.contains("logged out of"));
    assert!(!dir.path().join("cad-cli").join("credentials.json").exists());
}
