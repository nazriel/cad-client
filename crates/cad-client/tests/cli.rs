//! Black-box coverage of the shipped `cad-cli` binary.
//!
//! Everything here is asserted from outside the process: exit codes, the split
//! between stdout and stderr, and the bytes a machine-readable run produces.
//! That is the contract users and scripts depend on, and it is the one thing an
//! in-process test of `execute()` cannot check.

use assert_cmd::Command;
use predicates::prelude::*;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COMPANY_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0042);
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

/// A `cad-cli` invocation with the ambient CAD environment scrubbed, so a
/// developer's own credentials cannot change what the test observes.
fn cad_cli() -> Command {
    let mut command = Command::cargo_bin("cad-cli").expect("the binary is built for tests");
    for key in [
        "CAD_API_KEY",
        "CAD_API_SECRET",
        "CAD_SERVER_URL",
        "CAD_ENV",
        "CAD_COMPANY_ID",
        "RUST_LOG",
    ] {
        command.env_remove(key);
    }
    command
}

fn with_credentials() -> Command {
    let mut command = cad_cli();
    command
        .env("CAD_API_KEY", Uuid::nil().to_string())
        .env("CAD_API_SECRET", "secret");
    command
}

async fn mock_identity(server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Runs the binary off the async runtime so the mock server keeps serving.
async fn run(command: Command) -> assert_cmd::assert::Assert {
    tokio::task::spawn_blocking(move || {
        let mut command = command;
        command.assert()
    })
    .await
    .expect("the child process ran to completion")
}

#[test]
fn help_lists_every_command_group_and_exits_zero() {
    let assert = cad_cli().arg("--help").assert().success();
    let stdout = predicate::str::contains("company")
        .and(predicate::str::contains("activity-group"))
        .and(predicate::str::contains("reporting-period"))
        .and(predicate::str::contains("inventory-line"))
        .and(predicate::str::contains("organizational-unit"))
        .and(predicate::str::contains("document"))
        .and(predicate::str::contains("custom-field"))
        .and(predicate::str::contains("custom-option"))
        .and(predicate::str::contains("report"))
        .and(predicate::str::contains("schema"))
        .and(predicate::str::contains("context"));
    assert.stdout(stdout);
}

#[test]
fn generated_artefacts_need_no_credentials_or_server() {
    // Both are derived from the command definition, so they must run before any
    // configuration is resolved — an unconfigured machine can still install them.
    cad_cli()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cad-cli").and(predicate::str::contains("company")));

    cad_cli()
        .arg("man")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(".ie \\n(.g"));

    let output = cad_cli()
        .args(["schema", "activity-group"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "activity-group");
    assert_eq!(
        schema["commands"]["update"]["inputTransport"]["argument"],
        "--input"
    );

    let output = cad_cli()
        .args(["schema", "benchmark"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "benchmark");
    assert_eq!(schema["serverManagedFields"], serde_json::json!(["id"]));

    let output = cad_cli()
        .args(["schema", "reporting-period"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "reporting-period");
    assert_eq!(
        schema["commands"]["create"]["inputTransport"]["argument"],
        "--input"
    );

    let output = cad_cli()
        .args(["schema", "organizational-unit"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "organizational-unit");
    assert_eq!(
        schema["commands"]["duplicate"]["options"][0]["name"],
        "--include-children"
    );
}

#[test]
fn missing_credentials_name_the_variable_and_exit_two() {
    cad_cli()
        .args(["--environment", "staging", "company", "list"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("CAD_API_KEY"));
}

#[test]
fn an_unknown_flag_is_a_usage_error() {
    cad_cli()
        .args(["--definitely-not-a-flag", "company", "list"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn company_list_keeps_debug_logs_out_of_json_stdout() {
    let server = MockServer::start().await;
    mock_identity(
        &server,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": {
                "userId": Uuid::nil(),
                "affiliatedCompanyId": COMPANY_ID
            },
            "companySelection": {
                "resolvedCompanyId": COMPANY_ID,
                "source": "DEFAULT"
            }
        })),
    )
    .await;
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

    // This is exactly what the insecure-loopback opt-in exists for.
    let mut command = with_credentials();
    command.env("RUST_LOG", "debug");
    command.args([
        "--server-url",
        &server.uri(),
        "--allow-insecure-http",
        "--output",
        "json",
        "company",
        "list",
    ]);

    let output = run(command).await.success().get_output().clone();
    let rendered: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is nothing but JSON");
    assert_eq!(rendered[0]["displayName"], "Acme NV");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejected_credentials_exit_three() {
    let server = MockServer::start().await;
    // A rejected key is an authentication failure whatever client-error
    // status the API chose, not only 401.
    mock_identity(
        &server,
        ResponseTemplate::new(404)
            .set_body_json(serde_json::json!({"msg": "example key rejection"})),
    )
    .await;

    let mut command = with_credentials();
    command.args([
        "--server-url",
        &server.uri(),
        "--allow-insecure-http",
        "company",
        "list",
    ]);

    run(command)
        .await
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("example key rejection"));
}
