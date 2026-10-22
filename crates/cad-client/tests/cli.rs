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
const USER_ROLES_PATH: &str = "/api/accounts/v1/user-roles";

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
        "CAD_PROFILE",
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
        .and(predicate::str::contains("use"))
        .and(predicate::str::contains("context").not());
    assert.stdout(stdout);
}

#[test]
fn context_show_is_gone_and_use_check_replaces_it() {
    cad_cli()
        .args(["context", "show"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "unrecognized subcommand 'context'",
        ));
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
        .args([
            "--server-url",
            "https://staging.carbonaltdelete.eu",
            "company",
            "list",
        ])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("CAD_API_KEY")
                .and(predicate::str::contains("cad-cli use profile <name> --url")),
        );
}

#[test]
fn without_a_profile_or_a_server_a_command_names_the_fix() {
    cad_cli()
        .args(["company", "list"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("no profile is selected")
                .and(predicate::str::contains("cad-cli use profile <name>")),
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
        // No IDs without credentials; `-`, which picks one, needs none.
        .stdout("-:Select interactively")
        .stderr(predicate::str::is_empty());

    cad_cli()
        .env("COMPLETE", "zsh")
        .env("XDG_CACHE_HOME", cache.path())
        .env("_CLAP_COMPLETE_INDEX", "2")
        .env("RUST_LOG", "debug")
        .args(["--", "cad-cli", "--company-id", "0"])
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
    Mock::given(method("GET"))
        .and(path(USER_ROLES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
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
            .stdout(format!("-:Select interactively\n{COMPANY_ID}:Acme"))
            .stderr(predicate::str::is_empty());
    }
}

// --- use | use profile ------------------------------------------------------

const KEY_ID: &str = "00000000-0000-0000-0000-0000000000aa";
const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";
const USER_ID: &str = "00000000-0000-0000-0000-0000000000bb";
const USER_PATH: &str = "/api/accounts/v1/users/00000000-0000-0000-0000-0000000000bb";
const EMAIL: &str = "jane@example.test";

fn token() -> String {
    format!("{KEY_ID}.{SECRET}")
}

/// A platform that accepts exactly the key above and rejects any other, and
/// knows the user it belongs to.
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
    Mock::given(method("GET"))
        .and(path(USER_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id": USER_ID, "email": EMAIL})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    server
}

fn credentials_file(config_home: &Path) -> std::path::PathBuf {
    config_home.join("cad-cli").join("credentials.json")
}

fn profiles_file(config_home: &Path) -> std::path::PathBuf {
    config_home.join("cad-cli").join("profiles.json")
}

fn stored_profiles(config_home: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(profiles_file(config_home)).unwrap()).unwrap()
}

/// `use profile <name> --url <server> --allow-insecure-http`, for a server on
/// loopback.
fn use_profile(config_home: &Path, name: &str, server: &MockServer) -> Command {
    let mut command = cad_cli_in(config_home);
    command.args([
        "--output",
        "json",
        "use",
        "profile",
        name,
        "--url",
        &server.uri(),
        "--allow-insecure-http",
    ]);
    command
}

/// Writes a profiles file by hand, with `active` and each `(name, url)`.
fn write_profiles(config_home: &Path, active: &str, profiles: &[(&str, &str, bool)]) {
    let profiles: serde_json::Map<_, _> = profiles
        .iter()
        .map(|(name, url, http)| {
            (
                (*name).to_owned(),
                serde_json::json!({"url": url, "allowInsecureHttp": http}),
            )
        })
        .collect();
    let file = profiles_file(config_home);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        file,
        serde_json::json!({"version": 1, "active": active, "profiles": profiles}).to_string(),
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_new_profile_logs_in_and_later_commands_use_its_server_and_login() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();

    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(format!("{}\n", token()));
    let output = run(login).await.success().get_output().clone();

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let result: serde_json::Value = serde_json::from_str(&stdout).expect("JSON on stdout");
    assert_eq!(result["previous"], serde_json::Value::Null);
    let row = &result;
    assert_eq!(row["name"], "local");
    assert_eq!(row["url"], server.uri());
    assert_eq!(row["active"], true);
    assert_eq!(row["selected"], true);
    assert_eq!(row["keyId"], KEY_ID);
    assert_eq!(row["userId"], USER_ID);
    assert_eq!(row["email"], EMAIL);
    assert_eq!(row["storage"], "file");
    assert!(stderr.contains("logged in to"), "{stderr}");
    assert!(stderr.contains(EMAIL), "{stderr}");
    assert!(
        stderr.contains(&format!("now using local ({}) as {EMAIL}", server.uri())),
        "{stderr}"
    );
    assert!(!stdout.contains(SECRET) && !stderr.contains(SECRET));

    // The metadata is in profiles.json and the secret only in the credentials
    // file, under the profile's name and with the origin it was made for.
    let profiles = stored_profiles(home.path());
    assert_eq!(profiles["version"], 1);
    assert_eq!(profiles["active"], "local");
    assert_eq!(profiles["profiles"]["local"]["url"], server.uri());
    assert_eq!(profiles["profiles"]["local"]["allowInsecureHttp"], true);
    assert_eq!(profiles["profiles"]["local"]["login"]["email"], EMAIL);
    let file = credentials_file(home.path());
    assert!(
        !std::fs::read_to_string(profiles_file(home.path()))
            .unwrap()
            .contains(SECRET)
    );
    let credentials: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(credentials["credentials"]["local"]["secret"], SECRET);
    assert_eq!(credentials["credentials"]["local"]["origin"], server.uri());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        for written in [&file, &profiles_file(home.path())] {
            assert_eq!(mode(written), 0o600, "{}", written.display());
            assert_eq!(mode(written.parent().unwrap()), 0o700);
        }
    }

    // A command with no server, profile or credentials of its own: the active
    // profile's server, with its stored login.
    let before = server.received_requests().await.unwrap().len();
    let mut companies = cad_cli_in(home.path());
    companies.args(["--output", "json", "company", "list"]);
    run(companies).await.success().stdout("[]\n");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), before + 2, "the identity and the list");
    assert!(requests[before..].iter().all(|request| {
        request
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            == Some(format!("Bearer {}", token()).as_str())
    }));

    // `use` lists it, and never connects.
    let before = server.received_requests().await.unwrap().len();
    let mut list = cad_cli_in(home.path());
    list.args(["--output", "json", "use"]);
    let output = run(list).await.success().get_output().clone();
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["name"], "local");
    assert_eq!(rows[0]["active"], true);
    assert_eq!(rows[0]["email"], EMAIL);
    assert!(!String::from_utf8_lossy(&output.stdout).contains(SECRET));
    assert_eq!(server.received_requests().await.unwrap().len(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn another_server_gets_no_key_and_forgetting_removes_the_login_and_the_profile() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(token());
    run(login).await.success();
    let file = credentials_file(home.path());

    // Another port of the same host is not this profile's server: its key is
    // not sent there.
    let mut elsewhere = cad_cli_in(home.path());
    elsewhere.args([
        "--server-url",
        "http://localhost:1",
        "--allow-insecure-http",
        "use",
        "--check",
    ]);
    run(elsewhere)
        .await
        .code(2)
        .stderr(predicate::str::contains(
            "no credentials for http://localhost:1",
        ));

    // Forgetting removes the login and the profile.
    let mut forget = cad_cli_in(home.path());
    forget.args(["--output", "json", "use", "profile", "local", "--forget"]);
    let output = run(forget).await.success().get_output().clone();
    let forgotten: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        forgotten,
        serde_json::json!({"profile": "local", "removed": ["file"], "wasActive": true})
    );
    assert!(!file.exists());
    assert_eq!(
        stored_profiles(home.path())["profiles"],
        serde_json::json!({})
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("stays valid until it is revoked"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut after = cad_cli_in(home.path());
    after.args(["company", "list"]);
    run(after)
        .await
        .code(2)
        .stderr(predicate::str::contains("no profile is selected"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_users_request_still_logs_in_with_no_email() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
            "companySelection": {"resolvedCompanyId": COMPANY_ID, "source": "DEFAULT"}
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({"detail": "x"})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();

    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(token());
    let output = run(login).await.success().get_output().clone();

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["email"], serde_json::Value::Null);
    assert_eq!(result["userId"], USER_ID);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("could not read the e-mail"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stored_profiles(home.path())["profiles"]["local"]["login"]["email"],
        serde_json::Value::Null
    );
    assert!(credentials_file(home.path()).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dash_without_a_terminal_names_what_to_pass_and_makes_no_request() {
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    write_profiles(home.path(), "local", &[("local", &server.uri(), true)]);

    for (arguments, message) in [
        (
            vec!["use", "profile", "-"],
            "`-` picks a profile at a terminal, and there is none; name the profile",
        ),
        (
            vec!["use", "company", "-"],
            "`-` picks a company at a terminal, and there is none; pass its ID or name",
        ),
        (
            vec!["--company-id", "-", "reporting-period", "list"],
            "`-` picks a company at a terminal, and there is none; pass --company-id <ID> or set CAD_COMPANY_ID",
        ),
    ] {
        let mut command = cad_cli_in(home.path());
        command
            .env("CAD_API_KEY", KEY_ID)
            .env("CAD_API_SECRET", SECRET);
        command.args(&arguments);
        run(command)
            .await
            .code(2)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(message));
    }
    assert_eq!(stored_profiles(home.path())["active"], "local");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn switching_to_a_logged_in_profile_makes_no_request() {
    // The server would fail the test if anything reached it.
    let server = MockServer::start().await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        "staging",
        &[
            ("local", &server.uri(), true),
            ("staging", "https://staging.carbonaltdelete.eu", false),
        ],
    );
    // `local` has logged in before: its profiles.json entry says so.
    let mut document = stored_profiles(home.path());
    document["profiles"]["local"]["login"] = serde_json::json!({
        "keyId": KEY_ID, "userId": USER_ID, "email": EMAIL,
        "storage": "keyring", "storedAt": "2026-10-21T12:00:00Z"
    });
    std::fs::write(profiles_file(home.path()), document.to_string()).unwrap();

    let mut switch = cad_cli_in(home.path());
    switch.args(["--output", "json", "use", "profile", "local"]);
    let output = run(switch).await.success().get_output().clone();

    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["previous"], "staging");
    assert_eq!(result["email"], EMAIL);
    assert_eq!(stored_profiles(home.path())["active"], "local");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("as {EMAIL} (was staging)")),
        "{stderr}"
    );
    // `expect(0)` is verified when the server is dropped.
}

#[test]
fn a_known_name_without_a_terminal_or_a_url_takes_its_suggestion_and_says_so() {
    let home = tempfile::tempdir().unwrap();
    let output = cad_cli_in(home.path())
        .args(["--output", "json", "use", "profile", "staging"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains(
                "using https://staging.carbonaltdelete.eu for profile staging",
            )
            .and(predicate::str::contains("--with-token"))
            .and(predicate::str::contains("CAD_API_KEY")),
        )
        .get_output()
        .stdout
        .clone();
    let result: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(result["url"], "https://staging.carbonaltdelete.eu");
    assert_eq!(result["keyId"], serde_json::Value::Null);
    let profiles = stored_profiles(home.path());
    assert_eq!(profiles["active"], "staging");
    assert_eq!(
        profiles["profiles"]["staging"]["url"],
        "https://staging.carbonaltdelete.eu"
    );
}

#[test]
fn another_name_without_a_url_fails_naming_the_flag_and_stores_nothing() {
    let home = tempfile::tempdir().unwrap();
    cad_cli_in(home.path())
        .args(["use", "profile", "acme"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("--url"));
    assert!(!profiles_file(home.path()).exists());
}

#[test]
fn use_profile_refuses_names_flags_and_options_that_do_not_apply() {
    let home = tempfile::tempdir().unwrap();
    for (args, expected) in [
        (vec!["use", "profile", "Bad Name"], "1 to 32 characters"),
        (
            vec![
                "--profile",
                "x",
                "use",
                "profile",
                "acme",
                "--url",
                "https://x.test",
            ],
            "--profile does not apply to `cad-cli use profile`",
        ),
        (
            vec!["--server-url", "https://x.test", "use", "profile", "acme"],
            "--server-url does not apply to `cad-cli use profile`; use --url",
        ),
        (
            vec!["use", "profile", "acme", "--forget", "--login"],
            "cannot be used with",
        ),
    ] {
        cad_cli_in(home.path())
            .args(&args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(expected));
    }
    assert!(!profiles_file(home.path()).exists());
}

#[test]
fn use_without_profiles_lists_nothing_and_says_how_to_start() {
    let home = tempfile::tempdir().unwrap();
    cad_cli_in(home.path())
        .args(["--output", "json", "use"])
        .assert()
        .success()
        .stdout("[]\n")
        .stderr(predicate::str::contains(
            "no profile yet; run `cad-cli use profile <name>`",
        ));
    assert!(!profiles_file(home.path()).exists());
}

#[test]
fn use_marks_the_selected_profile_apart_from_the_active_one() {
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        "staging",
        &[
            ("production", "https://app.carbonaltdelete.eu", false),
            ("staging", "https://staging.carbonaltdelete.eu", false),
        ],
    );
    let list = |variable: Option<&str>| {
        let mut command = cad_cli_in(home.path());
        command.args(["--output", "json", "use"]);
        if let Some(variable) = variable {
            command.env("CAD_PROFILE", variable);
        }
        let output = command.assert().success().get_output().clone();
        let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        (rows, String::from_utf8(output.stderr).unwrap())
    };

    let (rows, stderr) = list(None);
    assert_eq!(rows[0]["name"], "production");
    assert_eq!(
        (rows[0]["active"].as_bool(), rows[0]["selected"].as_bool()),
        (Some(false), Some(false))
    );
    assert_eq!(
        (rows[1]["active"].as_bool(), rows[1]["selected"].as_bool()),
        (Some(true), Some(true))
    );
    assert!(
        stderr.contains("using profile staging (the active profile)"),
        "{stderr}"
    );

    let (rows, stderr) = list(Some("production"));
    assert_eq!(
        (rows[0]["active"].as_bool(), rows[0]["selected"].as_bool()),
        (Some(false), Some(true))
    );
    assert_eq!(
        (rows[1]["active"].as_bool(), rows[1]["selected"].as_bool()),
        (Some(true), Some(false))
    );
    assert!(
        stderr.contains("using profile production (selected by CAD_PROFILE)"),
        "{stderr}"
    );
}

#[test]
fn use_renders_a_table_with_the_account_in_its_columns() {
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        "staging",
        &[("staging", "https://staging.carbonaltdelete.eu", false)],
    );
    let mut document = stored_profiles(home.path());
    document["profiles"]["staging"]["login"] = serde_json::json!({
        "keyId": KEY_ID, "userId": USER_ID, "email": EMAIL,
        "storage": "keyring", "storedAt": "2026-10-21T12:00:00Z"
    });
    std::fs::write(profiles_file(home.path()), document.to_string()).unwrap();

    cad_cli_in(home.path())
        .args(["--output", "table", "use"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(EMAIL)
                .and(predicate::str::contains("staging"))
                .and(predicate::str::contains("email")),
        );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_key_exits_three_and_stores_nothing() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(token().replace("AbCdEf", "ZzZzZz"));
    run(login)
        .await
        .code(3)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ZzZzZz").not());
    assert!(!credentials_file(home.path()).exists());
    assert!(!profiles_file(home.path()).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_insecure_storage_and_a_keyring_login_exits_two_and_stores_nothing() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = use_profile(home.path(), "local", &server);
    login.args(["--with-token"]).write_stdin(token());
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
    assert!(!profiles_file(home.path()).exists());
}

#[test]
fn login_without_a_terminal_or_a_token_names_the_flag() {
    let home = tempfile::tempdir().unwrap();
    cad_cli_in(home.path())
        .args(["use", "profile", "staging", "--login", "--no-browser"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(
            predicate::str::contains("--with-token").and(predicate::str::contains(
                "https://staging.carbonaltdelete.eu/app/user-account-settings/api-keys",
            )),
        );
    assert!(!profiles_file(home.path()).exists());
}

#[cfg(unix)]
#[test]
fn a_credentials_file_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        "staging",
        &[("staging", "https://staging.carbonaltdelete.eu", false)],
    );
    let file = credentials_file(home.path());
    std::fs::write(&file, r#"{"version":1,"credentials":{}}"#).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();

    cad_cli_in(home.path())
        .args(["use", "--check"])
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
fn a_stored_entry_of_an_unknown_kind_is_refused_naming_kind_and_profile() {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        "staging",
        &[("staging", "https://staging.carbonaltdelete.eu", false)],
    );
    let file = credentials_file(home.path());
    let document = serde_json::json!({
        "version": 1,
        "credentials": {"staging": {"kind": "oauth", "accessToken": "never-shown"}}
    });
    std::fs::write(&file, document.to_string()).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();

    cad_cli_in(home.path())
        .args(["use", "--check"])
        .assert()
        .code(2)
        .stderr(
            predicate::str::contains("oauth")
                .and(predicate::str::contains("profile staging"))
                .and(predicate::str::contains("never-shown").not()),
        );
}

#[test]
fn a_damaged_profiles_file_is_refused_naming_it_and_left_alone() {
    let home = tempfile::tempdir().unwrap();
    let file = profiles_file(home.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{not json").unwrap();

    for args in [
        vec!["use"],
        vec!["use", "profile", "staging"],
        vec!["company", "list"],
    ] {
        cad_cli_in(home.path())
            .args(&args)
            .assert()
            .code(2)
            .stderr(predicate::str::contains(file.display().to_string()));
    }
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{not json");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_environment_beats_a_stored_login() {
    let server = platform_for_the_key().await;
    let home = tempfile::tempdir().unwrap();
    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(token());
    run(login).await.success();

    // The stored login would be accepted; the environment's key is not.
    let mut context = cad_cli_in(home.path());
    context
        .args(["use", "--check"])
        .env("CAD_API_KEY", Uuid::nil().to_string())
        .env("CAD_API_SECRET", "not-the-stored-secret");
    run(context).await.code(3);
}

const ACME_ID: &str = "00000000-0000-0000-0000-0000000000a1";
const OTHER_ID: &str = "00000000-0000-0000-0000-0000000000a2";
const REPORTING_PERIODS_PATH: &str = "/api/reporting-periods/v1/reporting-periods";

fn listed_company(id: &str, name: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "displayName": name,
        "createdAt": "2026-01-01T00:00:00Z",
        "isConsultingCompany": false,
        "isDemoCompany": false,
        "availableClientCompanies": null,
        "companyStatus": "ACTIVE"
    })
}

/// A platform for the key above with two companies, whose identity endpoint
/// resolves the company named by the `X-CAD-Company-Id` header.
async fn platform_with_two_companies() -> MockServer {
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
                return ResponseTemplate::new(401)
                    .set_body_json(serde_json::json!({"detail": "no"}));
            }
            let requested = request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok());
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "principal": {"userId": USER_ID, "affiliatedCompanyId": ACME_ID},
                "companySelection": {
                    "resolvedCompanyId": requested.unwrap_or(ACME_ID),
                    "source": if requested.is_some() { "HEADER" } else { "DEFAULT" }
                }
            }))
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"id": USER_ID, "email": EMAIL})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            listed_company(ACME_ID, "Acme GmbH"),
            listed_company(OTHER_ID, "Other AG"),
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(USER_ROLES_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                "id": Uuid::from_u128(1),
                "userId": USER_ID,
                "companyId": OTHER_ID,
                "userRole": "EXPERT"
            }])),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(REPORTING_PERIODS_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;
    server
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_company_scoped_command_needs_a_company_and_use_company_supplies_it() {
    let server = platform_with_two_companies().await;
    let home = tempfile::tempdir().unwrap();

    // Two companies and no terminal: the login leaves the company unset.
    let mut login = use_profile(home.path(), "local", &server);
    login
        .args(["--with-token", "--insecure-storage"])
        .write_stdin(token());
    let output = run(login).await.success().get_output().clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no company is selected; run `cad-cli use company <name|id>`"),
        "{stderr}"
    );
    let after_login = server.received_requests().await.unwrap().len();

    // The refusal comes before any request, and the platform's own choice of
    // company for the key (Acme's) is not used.
    let mut refused = cad_cli_in(home.path());
    refused.args(["reporting-period", "list"]);
    run(refused).await.code(2).stderr(predicate::str::contains(
        "no company is selected for profile local; run `cad-cli use company <name|id>`, or \
         pass --company-id or set CAD_COMPANY_ID",
    ));
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        after_login,
        "no request was sent"
    );

    // A session-level command needs no company.
    let mut list = cad_cli_in(home.path());
    list.args(["--output", "json", "company", "list"]);
    run(list).await.success();

    // Choosing one by name.
    let mut choose = cad_cli_in(home.path());
    choose.args(["--output", "json", "use", "company", "other ag"]);
    let output = run(choose).await.success().get_output().clone();
    let chosen: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        chosen,
        serde_json::json!({
            "profile": "local",
            "companyId": OTHER_ID,
            "companyName": "Other AG",
            "previousCompanyId": null,
            "previousCompanyName": null
        })
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!(
        "Using company Other AG on local ({}) as {EMAIL}",
        server.uri()
    )));
    assert_eq!(
        stored_profiles(home.path())["profiles"]["local"]["company"],
        serde_json::json!({"id": OTHER_ID, "name": "Other AG"})
    );

    // Now the command runs: one identity request carrying the company, and
    // the command's own.
    let before = server.received_requests().await.unwrap().len();
    let mut periods = cad_cli_in(home.path());
    periods.args(["--output", "json", "reporting-period", "list"]);
    run(periods).await.success();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), before + 2);
    assert_eq!(requests[before].url.path(), IDENTITY_PATH);
    assert_eq!(requests[before + 1].url.path(), REPORTING_PERIODS_PATH);
    for request in &requests[before..] {
        assert_eq!(
            request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok()),
            Some(OTHER_ID)
        );
    }

    // `use --check` shows where it comes from.
    let mut check = cad_cli_in(home.path());
    check.args(["--output", "json", "use", "--check"]);
    let output = run(check).await.success().get_output().clone();
    let checked: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        checked,
        serde_json::json!({
            "profile": "local",
            "url": server.uri(),
            "credentialsSource": "file",
            "keyId": KEY_ID,
            "userId": USER_ID,
            "email": EMAIL,
            "companyId": OTHER_ID,
            "companyName": "Other AG",
            "companySource": "profile",
            "resolvedCompanyId": OTHER_ID,
            "resolvedCompanySource": "HEADER"
        })
    );
}

#[test]
fn a_company_scoped_command_without_a_profile_names_the_flag_and_the_variable() {
    // A key and a server in the environment, but no profile: nothing connects.
    with_credentials()
        .env("CAD_SERVER_URL", "https://no-profile.example.test")
        .args(["reporting-period", "list"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "no company is selected; pass --company-id or set CAD_COMPANY_ID",
        ));
}

/// Parses stderr that must be exactly one JSON line, and returns it.
fn error_line(stderr: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stderr);
    assert_eq!(text.lines().count(), 1, "one line on stderr: {text}");
    assert!(text.ends_with('\n'), "{text:?}");
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"))
}

#[test]
fn a_json_error_is_one_object_on_stderr_and_stdout_stays_empty() {
    for format in ["json", "jsonl"] {
        let output = cad_cli()
            .args(["--output", format, "company", "list"])
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty())
            .get_output()
            .clone();
        let error = error_line(&output.stderr);
        let error = &error["error"];
        assert_eq!(error["kind"], "config", "{format}");
        assert_eq!(error["message"], "no profile is selected", "{format}");
        assert!(
            error["hint"]
                .as_str()
                .is_some_and(|hint| hint.contains("cad-cli use profile <name>")),
            "{format}: {error}"
        );
        assert_eq!(error["exitCode"], 2, "{format}");
        assert_eq!(error["status"], serde_json::Value::Null, "{format}");
        assert_eq!(error["details"], serde_json::Value::Null, "{format}");
    }
}

#[test]
fn a_table_error_stays_the_text_line() {
    cad_cli()
        .args(["--output", "table", "company", "list"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::starts_with(
            "configuration error: no profile is selected; run `cad-cli use profile <name>`",
        ))
        .stderr(predicate::str::contains("{").not());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rejected_request_reports_its_status_and_body_as_json() {
    let server = MockServer::start().await;
    mock_identity(
        &server,
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "principal": {"userId": USER_ID, "affiliatedCompanyId": COMPANY_ID},
            "companySelection": {"resolvedCompanyId": COMPANY_ID, "source": "HEADER"}
        })),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(REPORTING_PERIODS_PATH))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(serde_json::json!({"msg": "not your company"})),
        )
        .mount(&server)
        .await;

    let mut command = with_credentials();
    command.env("CAD_COMPANY_ID", COMPANY_ID.to_string()).args([
        "--server-url",
        &server.uri(),
        "--allow-insecure-http",
        "--output",
        "jsonl",
        "reporting-period",
        "list",
    ]);
    let output = run(command)
        .await
        .code(4)
        .stdout(predicate::str::is_empty())
        .get_output()
        .clone();
    let error = error_line(&output.stderr);
    let error = &error["error"];
    assert_eq!(error["kind"], "api");
    assert_eq!(error["status"], 403);
    assert_eq!(error["exitCode"], 4);
    assert_eq!(error["hint"], serde_json::Value::Null);
    assert_eq!(
        error["details"],
        serde_json::json!({"msg": "not your company"})
    );
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("HTTP 403")),
        "{error}"
    );
}
