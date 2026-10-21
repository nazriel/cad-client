//! Updater contract tests: versions, targets, checksums, archives and the URL
//! policy.
//!
//! The release fixtures are shared with the `update` command's tests.

use std::collections::HashMap;
use std::time::Duration;

use semver::Version;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::archive::{self, expected_digest, sha256_hex};
use super::target::{ArchiveFormat, Target};
use super::{ReleaseSource, URL_VARIABLE, available_message, parse_tag, permitted};
use crate::config::Environment;
use crate::error::CliError;

pub(crate) struct FakeEnvironment(HashMap<String, String>);

impl Environment for FakeEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }
}

pub(crate) fn environment<const N: usize>(pairs: [(&str, &str); N]) -> FakeEnvironment {
    FakeEnvironment(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
    )
}

/// A source that reads the latest release from `server`.
pub(crate) fn source(server: &MockServer) -> ReleaseSource {
    let url = format!("{}/releases/latest", server.uri());
    ReleaseSource::from_environment(
        &environment([(URL_VARIABLE, url.as_str())]),
        Duration::from_secs(5),
    )
    .expect("a loopback override is accepted")
}

pub(crate) fn version(raw: &str) -> Version {
    Version::parse(raw).expect("test versions are semver")
}

/// The archive `mise run dist` would pack for `target`, containing `binary`
/// as the executable.
#[cfg(not(windows))]
pub(crate) fn archive(target: Target, release: &Version, binary: &[u8]) -> Vec<u8> {
    archive_with_entry(
        &format!("{}/{}", target.archive_stem(release), target.binary_name()),
        binary,
    )
}

#[cfg(not(windows))]
fn archive_with_entry(entry: &str, contents: &[u8]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    builder
        .append_data(&mut header, entry, contents)
        .expect("the entry is appended");
    builder
        .into_inner()
        .and_then(flate2::write::GzEncoder::finish)
        .expect("the archive is finished")
}

/// Release notes in the shape `mise run release:notes` writes them, for a
/// release that follows v0.1.0.
pub(crate) const NOTES: &str = "## Changes

Since [`v0.1.0`](https://github.com/nazriel/cad-client/compare/v0.1.0...v0.2.0).

### Features

- feat: show release notes after an update (abc1234)

## Downloads

- [`SHA256SUMS`](https://github.com/nazriel/cad-client/releases/download/v0.2.0/SHA256SUMS)
";

/// Serves a release `tag` whose assets are `archive` (named for `target`) and
/// a `SHA256SUMS` with the given contents, each listed at its real size.
pub(crate) async fn mount_release(
    server: &MockServer,
    tag: &str,
    target: Target,
    archive: Vec<u8>,
    sums: String,
) {
    let sizes = (archive.len() as u64, sums.len() as u64);
    mount_release_listing(server, tag, target, archive, sums, sizes).await;
}

/// [`mount_release`] with the archive and `SHA256SUMS` sizes the release
/// document advertises.
async fn mount_release_listing(
    server: &MockServer,
    tag: &str,
    target: Target,
    archive: Vec<u8>,
    sums: String,
    (archive_size, sums_size): (u64, u64),
) {
    let release = parse_tag(tag).expect("fixture tags are versions");
    let name = target.archive_name(&release);
    let uri = server.uri();
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": tag,
            "html_url": format!("https://example.invalid/{tag}"),
            "body": NOTES,
            "assets": [
                {"name": name, "browser_download_url": format!("{uri}/download/{name}"), "size": archive_size},
                {"name": "SHA256SUMS", "browser_download_url": format!("{uri}/download/SHA256SUMS"), "size": sums_size},
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

/// A `SHA256SUMS` line for `archive` under the name `target` gives it.
#[cfg(not(windows))]
pub(crate) fn sums_line(target: Target, release: &Version, archive: &[u8]) -> String {
    format!(
        "{}  {}\n",
        sha256_hex(archive),
        target.archive_name(release)
    )
}

// Versions

#[test]
fn release_tags_parse_with_or_without_the_v_prefix() {
    assert_eq!(parse_tag("v0.2.0").expect("tag"), version("0.2.0"));
    assert_eq!(parse_tag("1.10.3").expect("tag"), version("1.10.3"));
    assert!(matches!(parse_tag("latest"), Err(CliError::Update(_))));
    assert!(matches!(parse_tag("v1.2"), Err(CliError::Update(_))));
}

#[test]
fn versions_compare_by_semver_precedence() {
    assert!(version("0.2.0") > version("0.1.0"));
    assert!(version("0.10.0") > version("0.9.9"));
    assert!(version("1.0.0") > version("1.0.0-rc.1"));
    assert!(version("0.1.0") <= version("0.1.0"));
}

#[test]
fn the_notice_names_both_versions_only_when_the_release_is_newer() {
    assert_eq!(
        available_message(&version("0.2.0"), &version("0.1.0")).as_deref(),
        Some("cad-cli 0.2.0 is available (current 0.1.0); run: cad-cli update")
    );
    assert_eq!(
        available_message(&version("0.1.0"), &version("0.1.0")),
        None
    );
    assert_eq!(
        available_message(&version("0.1.0"), &version("0.2.0")),
        None
    );
}

// Targets

#[test]
fn every_published_target_maps_from_os_and_arch() {
    let release = version("0.2.0");
    for (os, arch, archive, binary) in [
        (
            "linux",
            "x86_64",
            "cad-cli-v0.2.0-x86_64-unknown-linux-musl.tar.gz",
            "cad-cli",
        ),
        (
            "linux",
            "aarch64",
            "cad-cli-v0.2.0-aarch64-unknown-linux-musl.tar.gz",
            "cad-cli",
        ),
        (
            "macos",
            "x86_64",
            "cad-cli-v0.2.0-x86_64-apple-darwin.tar.gz",
            "cad-cli",
        ),
        (
            "macos",
            "aarch64",
            "cad-cli-v0.2.0-aarch64-apple-darwin.tar.gz",
            "cad-cli",
        ),
        (
            "windows",
            "x86_64",
            "cad-cli-v0.2.0-x86_64-pc-windows-msvc.zip",
            "cad-cli.exe",
        ),
        (
            "windows",
            "aarch64",
            "cad-cli-v0.2.0-aarch64-pc-windows-msvc.zip",
            "cad-cli.exe",
        ),
    ] {
        let target = Target::from_parts(os, arch).expect("a published target");
        assert_eq!(target.archive_name(&release), archive);
        assert_eq!(target.binary_name(), binary);
        let expected_format = if os == "windows" {
            ArchiveFormat::Zip
        } else {
            ArchiveFormat::TarGz
        };
        assert_eq!(target.format(), expected_format);
    }
}

#[test]
fn unpublished_platforms_have_no_target() {
    for (os, arch) in [
        ("freebsd", "x86_64"),
        ("linux", "x86"),
        ("linux", "riscv64"),
    ] {
        assert_eq!(Target::from_parts(os, arch), None, "{os}/{arch}");
    }
}

#[test]
fn the_running_platform_is_a_published_target() {
    // CI runs the tests on exactly the platforms releases are built for.
    assert!(Target::current().is_some());
}

// Checksums

#[test]
fn sha256_matches_the_reference_vector() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn sums_are_found_in_text_and_binary_mode_lines() {
    let sums = "\
aaaa  cad-cli-v0.2.0-x86_64-apple-darwin.tar.gz
bbbb *cad-cli-v0.2.0-x86_64-pc-windows-msvc.zip
cccc  cad-cli-v0.2.0-x86_64-apple-darwin.tar.gz.sig
";
    assert_eq!(
        expected_digest(sums, "cad-cli-v0.2.0-x86_64-apple-darwin.tar.gz"),
        Some("aaaa")
    );
    assert_eq!(
        expected_digest(sums, "cad-cli-v0.2.0-x86_64-pc-windows-msvc.zip"),
        Some("bbbb")
    );
    assert_eq!(
        expected_digest(sums, "cad-cli-v0.2.0-aarch64-apple-darwin.tar.gz"),
        None
    );
}

#[test]
fn a_matching_digest_verifies_in_either_case() {
    let digest = sha256_hex(b"archive");
    archive::verify(b"archive", "a.tar.gz", &format!("{digest}  a.tar.gz\n"))
        .expect("the digest matches");
    archive::verify(
        b"archive",
        "a.tar.gz",
        &format!("{}  a.tar.gz\n", digest.to_uppercase()),
    )
    .expect("hex case does not matter");
}

#[test]
fn a_checksum_mismatch_is_rejected() {
    let sums = format!("{}  a.tar.gz\n", sha256_hex(b"something else"));
    let error = archive::verify(b"archive", "a.tar.gz", &sums).expect_err("mismatch");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("checksum mismatch")),
        "{error}"
    );
}

#[test]
fn an_archive_missing_from_the_sums_is_rejected() {
    let sums = format!("{}  b.tar.gz\n", sha256_hex(b"archive"));
    let error = archive::verify(b"archive", "a.tar.gz", &sums).expect_err("unlisted");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("not listed")),
        "{error}"
    );
}

// Archives

#[cfg(not(windows))]
#[test]
fn the_executable_is_taken_from_the_archive_directory() {
    let target = Target::current().expect("a published target");
    let release = version("0.2.0");
    let bytes = archive(target, &release, b"new binary");

    let file = archive::extract_binary(&bytes, target, &target.archive_stem(&release), None)
        .expect("the binary is extracted");
    assert_eq!(std::fs::read(&file).expect("readable"), b"new binary");
}

#[cfg(unix)]
#[test]
fn the_executable_is_staged_in_the_given_directory_and_made_executable() {
    use std::os::unix::fs::PermissionsExt as _;

    let target = Target::current().expect("a published target");
    let release = version("0.2.0");
    let bytes = archive(target, &release, b"new binary");
    let directory = tempfile::tempdir().expect("temporary directory");

    let file = archive::extract_binary(
        &bytes,
        target,
        &target.archive_stem(&release),
        Some(directory.path()),
    )
    .expect("the binary is extracted");
    assert_eq!(file.parent(), Some(directory.path()));
    let mode = std::fs::metadata(&file)
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);

    // An unusable directory falls back to the system temporary directory.
    let missing = directory.path().join("missing");
    let file = archive::extract_binary(
        &bytes,
        target,
        &target.archive_stem(&release),
        Some(&missing),
    )
    .expect("the binary is extracted elsewhere");
    assert_ne!(file.parent(), Some(missing.as_path()));
}

#[cfg(not(windows))]
#[test]
fn an_executable_outside_the_archive_directory_is_not_used() {
    let target = Target::current().expect("a published target");
    let release = version("0.2.0");
    let bytes = archive_with_entry("cad-cli", b"stray binary");

    let error = archive::extract_binary(&bytes, target, &target.archive_stem(&release), None)
        .expect_err("the binary is not where install.sh looks");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("does not contain")),
        "{error}"
    );
}

#[cfg(not(windows))]
#[test]
fn a_corrupt_archive_is_rejected() {
    let target = Target::current().expect("a published target");
    let error = archive::extract_binary(b"not gzip", target, "stem", None).expect_err("corrupt");
    assert!(matches!(error, CliError::Update(_)), "{error}");
}

// URL policy

#[test]
fn only_https_or_opted_in_loopback_http_is_fetched() {
    let url = |raw: &str| reqwest::Url::parse(raw).expect("valid URL");
    assert!(permitted(&url("https://github.com/x"), false));
    assert!(!permitted(&url("http://github.com/x"), false));
    assert!(!permitted(&url("http://github.com/x"), true));
    assert!(!permitted(&url("http://127.0.0.1:8080/x"), false));
    assert!(permitted(&url("http://127.0.0.1:8080/x"), true));
    assert!(permitted(&url("http://localhost/x"), true));
    assert!(permitted(&url("http://[::1]/x"), true));
    assert!(!permitted(&url("ftp://127.0.0.1/x"), true));
}

#[test]
fn a_plain_http_override_of_a_remote_host_is_refused() {
    let result = ReleaseSource::from_environment(
        &environment([(URL_VARIABLE, "http://example.com/releases/latest")]),
        Duration::from_secs(1),
    );
    assert!(matches!(result, Err(CliError::Config { .. })));
}

#[tokio::test]
async fn an_asset_url_without_https_is_not_followed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v99.0.0",
            "assets": [{"name": "SHA256SUMS", "browser_download_url": "http://example.com/SHA256SUMS", "size": 64}],
        })))
        .mount(&server)
        .await;
    let target = Target::current().expect("a published target");
    let source = source(&server);
    let mut release = source.latest().await.expect("the release is read");
    release.assets.push(super::Asset {
        name: target.archive_name(&release.version),
        url: format!("{}/archive", server.uri()),
        size: 64,
    });

    let error = source
        .fetch_binary(&release, target, None)
        .await
        .expect_err("plain HTTP to a remote host");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("non-HTTPS")),
        "{error}"
    );
}

// Download sizes

/// Fetches the latest release's binary and returns the error and the paths
/// the server was asked for.
async fn fetch_error(server: &MockServer) -> (CliError, Vec<String>) {
    let target = Target::current().expect("a published target");
    let source = source(server);
    let latest = source.latest().await.expect("the release is read");
    let error = source
        .fetch_binary(&latest, target, None)
        .await
        .expect_err("the download is refused");
    let paths = server
        .received_requests()
        .await
        .expect("requests are recorded")
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect();
    (error, paths)
}

fn update_message(error: &CliError) -> &str {
    match error {
        CliError::Update(message) => message,
        other => panic!("expected an update error, got {other}"),
    }
}

#[tokio::test]
async fn an_archive_listed_above_the_cap_is_not_downloaded() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let sums = "digest  archive\n".to_owned();
    let sizes = (super::MAX_ARCHIVE_BYTES + 1, sums.len() as u64);
    mount_release_listing(&server, "v99.0.0", target, Vec::new(), sums, sizes).await;

    let (error, paths) = fetch_error(&server).await;
    assert!(update_message(&error).contains("limit"), "{error}");
    let archive_path = format!("/download/{}", target.archive_name(&version("99.0.0")));
    assert!(!paths.contains(&archive_path), "{paths:?}");
}

#[tokio::test]
async fn sums_listed_above_the_cap_are_not_downloaded() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let sizes = (16, super::MAX_SUMS_BYTES + 1);
    mount_release_listing(
        &server,
        "v99.0.0",
        target,
        vec![0; 16],
        String::new(),
        sizes,
    )
    .await;

    let (error, paths) = fetch_error(&server).await;
    assert!(
        update_message(&error).contains("SHA256SUMS is listed at"),
        "{error}"
    );
    assert!(
        !paths.contains(&"/download/SHA256SUMS".to_owned()),
        "{paths:?}"
    );
}

#[tokio::test]
async fn a_body_longer_than_advertised_is_rejected() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let sums = "digest  archive\n".to_owned();
    let sizes = (50, sums.len() as u64);
    mount_release_listing(&server, "v99.0.0", target, vec![0; 100], sums, sizes).await;

    let (error, _) = fetch_error(&server).await;
    assert!(
        update_message(&error).contains("sent more than the 50 bytes"),
        "{error}"
    );
}

#[tokio::test]
async fn a_body_shorter_than_advertised_is_rejected() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let sums = "digest  archive\n".to_owned();
    let sizes = (150, sums.len() as u64);
    mount_release_listing(&server, "v99.0.0", target, vec![0; 100], sums, sizes).await;

    let (error, _) = fetch_error(&server).await;
    assert!(
        update_message(&error).contains("ended after 100 of its 150 bytes"),
        "{error}"
    );
}

#[tokio::test]
async fn an_oversized_release_document_is_rejected() {
    let server = MockServer::start().await;
    let padding = "x".repeat(usize::try_from(super::MAX_DOCUMENT_BYTES).expect("fits"));
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v99.0.0",
            "body": padding,
            "assets": [],
        })))
        .mount(&server)
        .await;

    let error = source(&server).latest().await.expect_err("too large");
    assert!(
        update_message(&error).contains("release document is larger"),
        "{error}"
    );
}

// Probing the new binary

/// Writes an executable shell script into `directory`.
#[cfg(unix)]
fn script(directory: &std::path::Path, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    let path = directory.join("cad-cli-probe");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("writable");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

#[cfg(unix)]
#[tokio::test]
async fn a_binary_reporting_the_release_version_passes_the_probe() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let binary = script(directory.path(), "echo 'cad-cli 99.0.0'");

    super::install::probe(&binary, &version("99.0.0"))
        .await
        .expect("the probe passes");
}

#[cfg(unix)]
#[tokio::test]
async fn a_binary_reporting_another_version_fails_the_probe() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let binary = script(directory.path(), "echo 'cad-cli 98.0.0'");

    let error = super::install::probe(&binary, &version("99.0.0"))
        .await
        .expect_err("wrong version");
    assert_eq!(
        update_message(&error),
        "the downloaded binary reports `cad-cli 98.0.0` instead of `cad-cli 99.0.0`; \
         nothing was installed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_binary_that_fails_or_cannot_run_fails_the_probe() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let failing = script(directory.path(), "echo 'cad-cli 99.0.0'; exit 3");
    let error = super::install::probe(&failing, &version("99.0.0"))
        .await
        .expect_err("non-zero exit");
    assert!(
        update_message(&error).contains("does not run on this machine"),
        "{error}"
    );

    let missing = directory.path().join("missing");
    let error = super::install::probe(&missing, &version("99.0.0"))
        .await
        .expect_err("cannot start");
    assert!(
        update_message(&error).contains("does not run on this machine"),
        "{error}"
    );
}

// Release notes

fn release_with(notes: Option<&str>, page: Option<&str>) -> super::Release {
    super::Release {
        version: version("0.2.0"),
        notes: notes.map(ToOwned::to_owned),
        page: page.map(ToOwned::to_owned),
        assets: Vec::new(),
    }
}

#[test]
fn notes_show_the_changes_and_the_release_page_but_not_the_downloads() {
    let release = release_with(Some(NOTES), Some("https://example.invalid/v0.2.0"));
    let notes = super::notes::after_update(&release, &version("0.1.0"));

    assert_eq!(notes.heading, "Changes in cad-cli 0.2.0");
    assert_eq!(
        notes.body,
        "Since [`v0.1.0`](https://github.com/nazriel/cad-client/compare/v0.1.0...v0.2.0).

### Features

- feat: show release notes after an update (abc1234)

Release notes: https://example.invalid/v0.2.0"
    );
}

#[test]
fn skipping_releases_adds_a_compare_link() {
    let release = release_with(Some(NOTES), None);
    let notes = super::notes::after_update(&release, &version("0.0.9"));

    assert!(
        notes.body.ends_with(
            "\n\nEvery change since 0.0.9: \
             https://github.com/nazriel/cad-client/compare/v0.0.9...v0.2.0"
        ),
        "{}",
        notes.body
    );
}

#[test]
fn notes_that_do_not_name_their_predecessor_get_the_compare_link() {
    let notes = super::notes::after_update(&release_with(None, None), &version("0.1.0"));
    assert_eq!(
        notes.body,
        "Every change since 0.1.0: https://github.com/nazriel/cad-client/compare/v0.1.0...v0.2.0"
    );
    // A reinstall of the same release links nothing to compare.
    let reinstall = super::Release {
        version: version("0.1.0"),
        ..release_with(None, None)
    };
    assert_eq!(
        super::notes::after_update(&reinstall, &version("0.1.0")).body,
        ""
    );
}

#[test]
fn long_notes_are_capped_and_control_characters_removed() {
    let commits = (0..100)
        .map(|index| format!("- commit {index}\x1b[2J\r"))
        .collect::<Vec<_>>()
        .join("\n");
    let release = release_with(Some(&format!("## Changes\n\n{commits}")), None);
    let notes = super::notes::after_update(&release, &version("0.0.9"));

    let lines: Vec<&str> = notes.body.lines().collect();
    assert_eq!(lines[0], "- commit 0[2J");
    assert_eq!(lines[super::notes::MAX_NOTE_LINES - 1], "- commit 39[2J");
    assert_eq!(
        lines[super::notes::MAX_NOTE_LINES],
        "… 60 more lines in the release notes"
    );
    assert!(!notes.body.contains('\x1b') && !notes.body.contains('\r'));
}

// Release lookup and download

#[tokio::test]
async fn a_missing_release_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/releases/latest"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let error = source(&server).latest().await.expect_err("no release");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("404")),
        "{error}"
    );
}

#[cfg(not(windows))]
#[tokio::test]
async fn a_verified_archive_yields_the_new_executable() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let release = version("99.0.0");
    let bytes = archive(target, &release, b"new binary");
    let sums = sums_line(target, &release, &bytes);
    mount_release(&server, "v99.0.0", target, bytes, sums).await;

    let source = source(&server);
    let latest = source.latest().await.expect("the release is read");
    assert_eq!(latest.version, release);
    assert_eq!(
        latest.page.as_deref(),
        Some("https://example.invalid/v99.0.0")
    );
    assert!(
        latest
            .notes
            .as_deref()
            .is_some_and(|notes| notes.starts_with("## Changes"))
    );
    let file = source
        .fetch_binary(&latest, target, None)
        .await
        .expect("the archive verifies");
    assert_eq!(std::fs::read(&file).expect("readable"), b"new binary");
}

#[cfg(not(windows))]
#[tokio::test]
async fn a_tampered_archive_is_rejected_before_extraction() {
    let server = MockServer::start().await;
    let target = Target::current().expect("a published target");
    let release = version("99.0.0");
    let bytes = archive(target, &release, b"tampered binary");
    let sums = sums_line(target, &release, b"the archive that was published");
    mount_release(&server, "v99.0.0", target, bytes, sums).await;

    let source = source(&server);
    let latest = source.latest().await.expect("the release is read");
    let error = source
        .fetch_binary(&latest, target, None)
        .await
        .expect_err("the digest differs");
    assert!(
        matches!(&error, CliError::Update(message) if message.contains("checksum mismatch")),
        "{error}"
    );
}
