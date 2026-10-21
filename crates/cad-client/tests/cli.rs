//! Black-box coverage of the shipped `cad-cli` binary.
//!
//! Everything here is asserted from outside the process: exit codes, the split
//! between stdout and stderr, and the bytes a machine-readable run produces.
//! That is the contract users and scripts depend on, and it is the one thing an
//! in-process test of `execute()` cannot check.

use std::path::Path;
use std::sync::LazyLock;

use assert_cmd::Command;
use predicates::prelude::*;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COMPANY_ID: Uuid = Uuid::from_u128(0x0000_0000_0000_0000_0000_0000_0000_0042);
const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

/// The configuration directory of every test that does not bring its own: a
/// fresh, empty one, so a developer's stored logins cannot leak into a test
/// and no test can write to the real one.
static SCRATCH_CONFIG_HOME: LazyLock<tempfile::TempDir> =
    LazyLock::new(|| tempfile::tempdir().expect("a scratch configuration directory"));

/// A `cad-cli` invocation with the ambient CAD environment scrubbed, so a
/// developer's own credentials cannot change what the test observes. It never
/// touches the system keyring and keeps its configuration in a scratch
/// directory.
fn cad_cli() -> Command {
    cad_cli_in(SCRATCH_CONFIG_HOME.path())
}

/// Like [`cad_cli`], with `config_home` as the configuration directory.
fn cad_cli_in(config_home: &Path) -> Command {
    let mut command = Command::cargo_bin("cad-cli").expect("the binary is built for tests");
    for key in [
        "CAD_API_KEY",
        "CAD_API_SECRET",
        "CAD_SERVER_URL",
        "CAD_ENV",
        "CAD_COMPANY_ID",
        "CAD_CLI_COMPLETE_OFFLINE",
        "COMPLETE",
        "_CLAP_COMPLETE_INDEX",
        "RUST_LOG",
    ] {
        command.env_remove(key);
    }
    command
        .env("CAD_CLI_KEYRING", "0")
        .env("XDG_CONFIG_HOME", config_home);
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
        .and(predicate::str::contains("company-template"))
        .and(predicate::str::contains("activity-category"))
        .and(predicate::str::contains("activity-group"))
        .and(predicate::str::contains("reporting-period"))
        .and(predicate::str::contains("inventory-line"))
        .and(predicate::str::contains("organizational-unit"))
        .and(predicate::str::contains("document"))
        .and(predicate::str::contains("custom-field"))
        .and(predicate::str::contains("custom-option"))
        .and(predicate::str::contains("report"))
        .and(predicate::str::contains("schema"))
        .and(predicate::str::contains("context"))
        .and(predicate::str::contains("auth"));
    assert.stdout(stdout);
}

#[test]
fn version_prints_the_build_version_of_this_package() {
    let version = env!("CAD_CLI_BUILD_VERSION");
    assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
    cad_cli()
        .arg("--version")
        .assert()
        .success()
        .stdout(format!("cad-cli {version}\n"));
}

#[test]
fn generated_artefacts_need_no_credentials_or_server() {
    // Both are derived from the command definition, so they must run before any
    // configuration is resolved — an unconfigured machine can still install them.
    cad_cli()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("COMPLETE=\"bash\"").and(predicate::str::contains("cad-cli")),
        );

    cad_cli()
        .arg("man")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(".ie \\n(.g"));

    let output = cad_cli()
        .args(["schema", "activity-category"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "activity-category");
    assert_eq!(
        schema["commands"]["update"]["input"]["properties"]["activityCategoryType"]["$ref"],
        "#/$defs/ActivityCategoryType"
    );

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
        .args(["schema", "company"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "company");
    assert_eq!(schema["commands"]["create"]["confirmationRequired"], true);

    let output = cad_cli()
        .args(["schema", "company-template"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let schema: serde_json::Value = serde_json::from_slice(&output).expect("schema output is JSON");
    assert_eq!(schema["resource"], "company-template");

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
        .stderr(
            predicate::str::contains("CAD_API_KEY")
                .and(predicate::str::contains("cad-cli auth login")),
        );
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

#[test]
fn completions_print_the_same_registration_as_complete_env() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let subcommand = cad_cli()
            .args(["completions", shell])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let environment = cad_cli()
            .env("COMPLETE", shell)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        assert!(!subcommand.is_empty(), "{shell}");
        assert_eq!(subcommand, environment, "{shell}");
    }
}

#[test]
fn completion_without_credentials_still_completes_commands_quietly() {
    let cache = tempfile::tempdir().expect("temporary cache directory");
    cad_cli()
        .env("COMPLETE", "zsh")
        .env("XDG_CACHE_HOME", cache.path())
        .env("_CLAP_COMPLETE_INDEX", "2")
        .env("RUST_LOG", "debug")
        .args(["--", "cad-cli", "--company-id", ""])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());

    cad_cli()
        .env("COMPLETE", "zsh")
        .env("XDG_CACHE_HOME", cache.path())
        .env("_CLAP_COMPLETE_INDEX", "1")
        .args(["--", "cad-cli", "reporting-p"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("reporting-period:"))
        .stderr(predicate::str::is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn completion_lists_company_ids_from_the_platform_on_stdout_only() {
    let server = MockServer::start().await;
    mock_identity(
        &server,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": { "userId": Uuid::nil(), "affiliatedCompanyId": COMPANY_ID },
            "companySelection": { "resolvedCompanyId": COMPANY_ID, "source": "DEFAULT" }
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
        .expect(1)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().expect("temporary cache directory");
    let completion = |cache: &std::path::Path| {
        let mut command = with_credentials();
        command
            .env("COMPLETE", "zsh")
            .env("XDG_CACHE_HOME", cache)
            .env("_CLAP_COMPLETE_INDEX", "5")
            .env("RUST_LOG", "debug")
            .args([
                "--",
                "cad-cli",
                "--server-url",
                &server.uri(),
                "--allow-insecure-http",
                "--company-id",
                "",
            ]);
        command
    };

    // The first request goes live; the second is answered from the cache,
    // which `expect(1)` verifies when the server is dropped.
    for _ in 0..2 {
        run(completion(cache.path()))
            .await
            .success()
            .stdout(format!("{COMPANY_ID}:Acme NV"))
            .stderr(predicate::str::is_empty());
    }
}

// --- auth login | status | logout ---------------------------------------------

const KEY_ID: &str = "00000000-0000-0000-0000-0000000000aa";
const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";
const USER_ID: &str = "00000000-0000-0000-0000-0000000000bb";

fn token() -> String {
    format!("{KEY_ID}.{SECRET}")
}

/// A platform that accepts exactly the key above and rejects any other.
async fn platform_for_the_key() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
            "companySelection": {"resolvedCompanyId": COMPANY_ID, "source": "DEFAULT"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"detail": "no"})))
        .mount(&server)
        .await;
    server
}

fn credentials_file(config_home: &Path) -> std::path::PathBuf {
    config_home.join("cad-cli").join("credentials.json")
}

fn against(command: &mut Command, server: &MockServer) {
    command.args(["--server-url", &server.uri(), "--allow-insecure-http"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_with_a_token_stores_in_the_file_and_later_commands_use_it() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();

    let mut login = cad_cli_in(home.path());
    against(&mut login, &server);
    login
        .args([
            "--output",
            "json",
            "auth",
            "login",
            "--with-token",
            "--insecure-storage",
        ])
        .write_stdin(format!("{}\n", token()));
    let output = run(login).await.success().get_output().clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("JSON on stdout");
    let file = credentials_file(home.path());
    assert_eq!(result["origin"], server.uri());
    assert_eq!(result["keyId"], KEY_ID);
    assert_eq!(result["userId"], USER_ID);
    assert_eq!(result["affiliatedCompanyId"], COMPANY_ID.to_string());
    assert_eq!(result["storage"], "file");
    assert_eq!(result["path"], file.display().to_string());
    assert!(stderr.contains("logged in to"), "{stderr}");
    assert!(stderr.contains(&server.uri()), "{stderr}");
    assert!(!stdout.contains(SECRET) && !stderr.contains(SECRET));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(file.parent().unwrap()), 0o700);
    }

    // Another command, with no environment credentials, uses the stored login.
    let mut context = cad_cli_in(home.path());
    against(&mut context, &server);
    context.args(["--output", "json", "context", "show"]);
    run(context)
        .await
        .success()
        .stdout(predicate::str::contains(USER_ID));

    let mut status = cad_cli_in(home.path());
    against(&mut status, &server);
    status.args(["--output", "json", "auth", "status"]);
    let output = run(status).await.success().get_output().clone();
    let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(status["source"], "file");
    assert_eq!(status["keyId"], KEY_ID);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(SECRET));

    // Another port of the same host has no login: the key is never sent there.
    let mut elsewhere = cad_cli_in(home.path());
    elsewhere.args([
        "--server-url",
        "http://localhost:1",
        "--allow-insecure-http",
        "context",
        "show",
    ]);
    run(elsewhere)
        .await
        .code(2)
        .stderr(predicate::str::contains(
            "no credentials for http://localhost:1",
        ));

    let mut logout = cad_cli_in(home.path());
    against(&mut logout, &server);
    logout.args(["--output", "json", "auth", "logout"]);
    let output = run(logout).await.success().get_output().clone();
    let removed: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(removed["removed"], serde_json::json!(["file"]));
    assert!(!file.exists());

    let mut after = cad_cli_in(home.path());
    against(&mut after, &server);
    after.args(["context", "show"]);
    run(after).await.code(2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_key_exits_three_and_stores_nothing() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = cad_cli_in(home.path());
    against(&mut login, &server);
    login
        .args(["auth", "login", "--with-token", "--insecure-storage"])
        .write_stdin(token().replace("AbCdEf", "ZzZzZz"));
    run(login)
        .await
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ZzZzZz").not());
    assert!(!credentials_file(home.path()).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_insecure_storage_and_a_keyring_login_exits_two_and_stores_nothing() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = cad_cli_in(home.path());
    against(&mut login, &server);
    login
        .args(["auth", "login", "--with-token"])
        .write_stdin(token());
    run(login)
        .await
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("--insecure-storage")
                .and(predicate::str::contains("CAD_API_KEY"))
                .and(predicate::str::contains(SECRET).not()),
        );
    assert!(!credentials_file(home.path()).exists());
}

#[test]
fn login_without_a_terminal_or_a_token_names_the_flag() {
    let home = tempfile::tempdir().unwrap();
    cad_cli_in(home.path())
        .args(["--environment", "staging", "auth", "login", "--no-browser"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("--with-token").and(predicate::str::contains(
                "https://staging.carbonaltdelete.eu/app/user-account-settings/api-keys",
            )),
        );
}

#[test]
fn auth_needs_a_server_like_every_command() {
    for rest in ["login", "status", "logout"] {
        cad_cli()
            .args(["auth", rest])
            .assert()
            .code(2)
            .stderr(predicate::str::contains("--server-url"));
    }
}

#[cfg(unix)]
#[test]
fn a_credentials_file_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let file = credentials_file(home.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, r#"{"version":1,"credentials":{}}"#).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

    cad_cli_in(home.path())
        .args(["--environment", "staging", "context", "show"])
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains(file.display().to_string())
                .and(predicate::str::contains("644"))
                .and(predicate::str::contains("chmod 600")),
        );
}

#[cfg(unix)]
#[test]
fn a_stored_entry_of_an_unknown_kind_is_refused_naming_kind_and_origin() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    let file = credentials_file(home.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let origin = "https://staging.carbonaltdelete.eu";
    let document = serde_json::json!({
        "version": 1,
        "credentials": {origin: {"kind": "oauth", "accessToken": "never-shown"}}
    });
    std::fs::write(&file, document.to_string()).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();

    cad_cli_in(home.path())
        .args(["--environment", "staging", "context", "show"])
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("oauth")
                .and(predicate::str::contains(origin))
                .and(predicate::str::contains("never-shown").not()),
        );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_environment_beats_a_stored_login() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = cad_cli_in(home.path());
    against(&mut login, &server);
    login
        .args(["auth", "login", "--with-token", "--insecure-storage"])
        .write_stdin(token());
    run(login).await.success();

    // The stored login would be accepted; the environment's key is not.
    let mut context = cad_cli_in(home.path());
    against(&mut context, &server);
    context
        .args(["context", "show"])
        .env("CAD_API_KEY", Uuid::nil().to_string())
        .env("CAD_API_SECRET", "not-the-stored-secret");
    run(context).await.code(3);
}
