//! Black-box coverage of `cad-cli update` and the background release check,
//! against a fake GitHub release served from loopback.

use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CURRENT: &str = env!("CARGO_PKG_VERSION");
const LATEST_PATH: &str = "/releases/latest";

/// A `cad-cli` invocation that reads releases from `server` and keeps its
/// cache in `cache`, with the ambient CAD environment scrubbed.
fn cad_cli(program: &Path, server: &MockServer, cache: &Path) -> Command {
    let mut command = Command::new(program);
    for key in [
        "CAD_API_KEY",
        "CAD_API_SECRET",
        "CAD_SERVER_URL",
        "CAD_ENV",
        "CAD_COMPANY_ID",
        "CAD_CLI_NO_UPDATE_CHECK",
        "CI",
        "RUST_LOG",
    ] {
        command.env_remove(key);
    }
    command
        .env(
            "CAD_CLI_UPDATE_URL",
            format!("{}{LATEST_PATH}", server.uri()),
        )
        .env("HOME", cache)
        .env("XDG_CACHE_HOME", cache)
        .env("LOCALAPPDATA", cache);
    command
}

fn built_binary() -> std::path::PathBuf {
    assert_cmd::cargo::cargo_bin("cad-cli")
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

async fn mount_tag(server: &MockServer, tag: &str) {
    Mock::given(method("GET"))
        .and(path(LATEST_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": tag,
            "assets": [],
        })))
        .mount(server)
        .await;
}

fn stdout_json(assert: &assert_cmd::assert::Assert) -> Value {
    serde_json::from_slice(&assert.get_output().stdout).expect("stdout is JSON")
}

#[tokio::test]
async fn check_exits_ten_when_a_newer_release_exists() {
    let server = MockServer::start().await;
    mount_tag(&server, "v99.0.0").await;
    let cache = tempfile::tempdir().expect("temporary directory");
    let mut command = cad_cli(&built_binary(), &server, cache.path());
    command.args(["--output", "json", "update", "--check"]);

    let assert = run(command)
        .await
        .code(10)
        .stderr(predicate::str::contains(format!(
            "cad-cli 99.0.0 is available (current {CURRENT}); run: cad-cli update"
        )));
    assert_eq!(
        stdout_json(&assert),
        json!({"current": CURRENT, "latest": "99.0.0", "updateAvailable": true, "updated": false})
    );
}

#[tokio::test]
async fn check_exits_zero_when_up_to_date() {
    let server = MockServer::start().await;
    mount_tag(&server, &format!("v{CURRENT}")).await;
    let cache = tempfile::tempdir().expect("temporary directory");
    let mut command = cad_cli(&built_binary(), &server, cache.path());
    command.args(["--output", "json", "update", "--check"]);

    let assert = run(command)
        .await
        .success()
        .stderr(predicate::str::contains("is up to date"));
    assert_eq!(stdout_json(&assert)["updateAvailable"], false);
}

#[tokio::test]
async fn an_unreachable_release_server_is_a_transport_failure() {
    let server = MockServer::start().await;
    let cache = tempfile::tempdir().expect("temporary directory");
    let mut command = cad_cli(&built_binary(), &server, cache.path());
    command
        .args(["update", "--check"])
        .env("CAD_CLI_UPDATE_URL", "http://127.0.0.1:9/releases/latest");

    run(command)
        .await
        .code(5)
        .stdout(predicate::str::is_empty());
}

#[tokio::test]
async fn the_background_check_stays_off_when_stderr_is_not_a_terminal() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(LATEST_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v99.0.0",
            "assets": [],
        })))
        .expect(0)
        .mount(&server)
        .await;
    let cache = tempfile::tempdir().expect("temporary directory");
    let mut command = cad_cli(&built_binary(), &server, cache.path());
    // Fails on missing credentials, after the point the check would start.
    command.args(["context", "show"]);

    run(command)
        .await
        .code(2)
        .stderr(predicate::str::contains("is available").not());
    assert!(
        std::fs::read_dir(cache.path())
            .expect("readable")
            .next()
            .is_none(),
        "no state file is written"
    );
}

/// Full updates replace a copy of the built binary, never the binary itself.
#[cfg(unix)]
mod replace {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    const NEW_BINARY: &[u8] = b"#!/bin/sh\necho 'cad-cli 99.0.0'\n";

    fn target() -> &'static str {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "x86_64-unknown-linux-musl",
            ("linux", "aarch64") => "aarch64-unknown-linux-musl",
            ("macos", "x86_64") => "x86_64-apple-darwin",
            ("macos", "aarch64") => "aarch64-apple-darwin",
            (os, arch) => panic!("no release is published for {os}/{arch}"),
        }
    }

    /// The `.tar.gz` that `mise run dist` packs, holding `binary` as cad-cli.
    fn release_archive(stem: &str, binary: &[u8]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(binary.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("{stem}/cad-cli"), binary)
            .expect("the entry is appended");
        builder
            .into_inner()
            .and_then(flate2::write::GzEncoder::finish)
            .expect("the archive is finished")
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;

        ring::digest::digest(&ring::digest::SHA256, bytes)
            .as_ref()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            })
    }

    /// Serves release v99.0.0; `SHA256SUMS` lists the digest of `listed`.
    async fn mount_release(server: &MockServer, archive: Vec<u8>, listed: &[u8]) {
        let name = format!("cad-cli-v99.0.0-{}.tar.gz", target());
        let sums = format!("{}  {name}\n", sha256_hex(listed));
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path(LATEST_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "tag_name": "v99.0.0",
                "html_url": "https://example.invalid/v99.0.0",
                "body": "## Changes\n\nSince [`v0.1.0`](https://example.invalid).\n",
                "assets": [
                    {"name": name, "browser_download_url": format!("{uri}/download/{name}"), "size": archive.len()},
                    {"name": "SHA256SUMS", "browser_download_url": format!("{uri}/download/SHA256SUMS"), "size": sums.len()},
                ],
            })))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/download/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(archive))
            .mount(server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/SHA256SUMS"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sums))
            .mount(server)
            .await;
    }

    /// Names in `directory` other than the installed copy: staging leftovers.
    fn leftovers(directory: &Path) -> Vec<String> {
        std::fs::read_dir(directory)
            .expect("readable")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|name| name != "cad-cli")
            .collect()
    }

    /// Copies the built binary into `directory`, where it may be replaced.
    fn installed_copy(directory: &Path) -> std::path::PathBuf {
        let copy = directory.join("cad-cli");
        std::fs::copy(built_binary(), &copy).expect("the binary is copied");
        copy
    }

    #[tokio::test]
    async fn update_replaces_the_running_executable_with_the_verified_release() {
        let server = MockServer::start().await;
        let archive = release_archive(&format!("cad-cli-v99.0.0-{}", target()), NEW_BINARY);
        mount_release(&server, archive.clone(), &archive).await;
        let directory = tempfile::tempdir().expect("temporary directory");
        let copy = installed_copy(directory.path());
        let mut command = cad_cli(&copy, &server, directory.path());
        command.args(["--output", "json", "update"]);

        let assert = run(command)
            .await
            .success()
            .stderr(predicate::str::contains(format!(
                "updated cad-cli {CURRENT} to 99.0.0"
            )));
        assert_eq!(
            stdout_json(&assert),
            json!({"current": CURRENT, "latest": "99.0.0", "updateAvailable": true, "updated": true})
        );
        assert_eq!(std::fs::read(&copy).expect("readable"), NEW_BINARY);
        let mode = std::fs::metadata(&copy)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the new binary is executable");
        Command::new(&copy)
            .assert()
            .success()
            .stdout("cad-cli 99.0.0\n");
        assert_eq!(leftovers(directory.path()), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_binary_that_fails_the_probe_is_not_installed() {
        let server = MockServer::start().await;
        let wrong = b"#!/bin/sh\necho 'cad-cli 98.0.0'\n";
        let archive = release_archive(&format!("cad-cli-v99.0.0-{}", target()), wrong);
        mount_release(&server, archive.clone(), &archive).await;
        let directory = tempfile::tempdir().expect("temporary directory");
        let copy = installed_copy(directory.path());
        let before = std::fs::read(&copy).expect("readable");
        let mut command = cad_cli(&copy, &server, directory.path());
        command.arg("update");

        run(command)
            .await
            .code(4)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains(
                "the downloaded binary reports `cad-cli 98.0.0` instead of `cad-cli 99.0.0`",
            ));
        assert_eq!(std::fs::read(&copy).expect("readable"), before);
        assert_eq!(leftovers(directory.path()), Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_checksum_mismatch_leaves_the_executable_untouched() {
        let server = MockServer::start().await;
        let archive = release_archive(&format!("cad-cli-v99.0.0-{}", target()), NEW_BINARY);
        mount_release(&server, archive, b"the archive that was published").await;
        let directory = tempfile::tempdir().expect("temporary directory");
        let copy = installed_copy(directory.path());
        let before = std::fs::read(&copy).expect("readable");
        let mut command = cad_cli(&copy, &server, directory.path());
        command.arg("update");

        run(command)
            .await
            .code(4)
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::contains("checksum mismatch"));
        assert_eq!(std::fs::read(&copy).expect("readable"), before);
    }
}
