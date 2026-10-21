//! `cad-cli update` parsing and workflow, with the install step (probe and
//! replacement) faked so the test binary is never overwritten.

use std::cell::RefCell;
use std::path::Path;

use clap::Parser;
use semver::Version;
use serde_json::json;
use wiremock::MockServer;

use super::{UPDATE_AVAILABLE_STATUS, UpdateArgs};
use crate::app::context::CommandContext;
use crate::cli::Cli;
use crate::cmds::Command;
use crate::error::CliError;
use crate::exec::update::target::Target;
use crate::exec::update::tests::{mount_release, source, version};
use crate::support::terminal::{SilentTerminal, StatusKind};

fn parse(arguments: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(arguments)
}

fn update_args(arguments: &[&str]) -> UpdateArgs {
    match parse(arguments).expect("update parses").command {
        Command::Update(args) => args,
        other => panic!("expected update, got {other:?}"),
    }
}

#[test]
fn check_and_force_parse_but_exclude_each_other() {
    let plain = update_args(&["cad-cli", "update"]);
    assert!(!plain.check && !plain.force);
    assert!(update_args(&["cad-cli", "update", "--check"]).check);
    assert!(update_args(&["cad-cli", "update", "--force"]).force);
    parse(&["cad-cli", "update", "--check", "--force"]).expect_err("conflicting flags");
}

/// Records what the fake install step was given: the binary's bytes and the
/// version it was asked to install.
#[derive(Default)]
struct Installed(RefCell<Option<(Vec<u8>, Version)>>);

impl Installed {
    fn install(&self) -> impl AsyncFnOnce(&Path, &Version) -> Result<(), CliError> + '_ {
        async |binary: &Path, version: &Version| {
            *self.0.borrow_mut() = Some((std::fs::read(binary)?, version.clone()));
            Ok(())
        }
    }

    fn bytes(&self) -> Option<Vec<u8>> {
        self.0.borrow().as_ref().map(|(bytes, _)| bytes.clone())
    }

    fn version(&self) -> Option<Version> {
        self.0.borrow().as_ref().map(|(_, version)| version.clone())
    }
}

/// A release whose download assets are never fetched by a check.
async fn release_without_archive(server: &MockServer, tag: &str) {
    let target = Target::current().expect("a published target");
    mount_release(server, tag, target, Vec::new(), String::new()).await;
}

#[tokio::test]
async fn check_reports_a_newer_release_with_its_exit_status() {
    let server = MockServer::start().await;
    release_without_archive(&server, "v0.2.0").await;
    let installed = Installed::default();
    let terminal = SilentTerminal;

    let report = update_args(&["cad-cli", "update", "--check"])
        .run_with(
            &CommandContext::new(&terminal, false),
            &source(&server),
            Target::current().expect("a published target"),
            &version("0.1.0"),
            installed.install(),
        )
        .await
        .expect("the check succeeds");

    assert_eq!(
        report.value,
        json!({"current": "0.1.0", "latest": "0.2.0", "updateAvailable": true, "updated": false})
    );
    assert_eq!(report.exit_status, UPDATE_AVAILABLE_STATUS);
    assert_eq!(report.status, StatusKind::Info);
    assert_eq!(
        report.summary,
        "cad-cli 0.2.0 is available (current 0.1.0); run: cad-cli update"
    );
    assert_eq!(report.notes, None, "a check shows no release notes");
    assert_eq!(installed.bytes(), None);
}

#[tokio::test]
async fn an_up_to_date_binary_is_left_alone() {
    let server = MockServer::start().await;
    release_without_archive(&server, "v0.1.0").await;
    let terminal = SilentTerminal;

    for arguments in [
        &["cad-cli", "update", "--check"][..],
        &["cad-cli", "update"],
    ] {
        let installed = Installed::default();
        let report = update_args(arguments)
            .run_with(
                &CommandContext::new(&terminal, false),
                &source(&server),
                Target::current().expect("a published target"),
                &version("0.1.0"),
                installed.install(),
            )
            .await
            .expect("nothing to do");

        assert_eq!(report.exit_status, 0, "{arguments:?}");
        assert_eq!(report.status, StatusKind::Success);
        assert_eq!(report.summary, "cad-cli 0.1.0 is up to date");
        assert_eq!(report.value["updated"], json!(false));
        assert_eq!(report.notes, None);
        assert_eq!(installed.bytes(), None);
    }
}

#[tokio::test]
async fn an_older_latest_release_is_never_installed_without_force() {
    let server = MockServer::start().await;
    release_without_archive(&server, "v0.1.0").await;
    let installed = Installed::default();
    let terminal = SilentTerminal;

    let report = update_args(&["cad-cli", "update"])
        .run_with(
            &CommandContext::new(&terminal, false),
            &source(&server),
            Target::current().expect("a published target"),
            &version("0.2.0-dev.1"),
            installed.install(),
        )
        .await
        .expect("nothing to do");

    assert_eq!(report.value["updateAvailable"], json!(false));
    assert_eq!(installed.bytes(), None);
}

#[cfg(not(windows))]
mod install {
    use super::*;
    use crate::exec::update::tests::{archive, sums_line};

    async fn run(
        arguments: &[&str],
        current: &str,
        sums_for: &[u8],
    ) -> (Result<super::super::Report, CliError>, Installed) {
        let server = MockServer::start().await;
        let target = Target::current().expect("a published target");
        let release = version("0.2.0");
        let bytes = archive(target, &release, b"new binary");
        let sums = sums_line(
            target,
            &release,
            if sums_for.is_empty() {
                &bytes
            } else {
                sums_for
            },
        );
        mount_release(&server, "v0.2.0", target, bytes, sums).await;
        let installed = Installed::default();
        let terminal = SilentTerminal;

        let result = update_args(arguments)
            .run_with(
                &CommandContext::new(&terminal, false),
                &source(&server),
                target,
                &version(current),
                installed.install(),
            )
            .await;
        (result, installed)
    }

    #[tokio::test]
    async fn a_newer_release_is_verified_and_installed() {
        let (result, installed) = run(&["cad-cli", "update"], "0.1.0", b"").await;
        let report = result.expect("the update succeeds");

        assert_eq!(
            report.value,
            json!({"current": "0.1.0", "latest": "0.2.0", "updateAvailable": true, "updated": true})
        );
        assert_eq!(report.exit_status, 0);
        assert_eq!(report.summary, "updated cad-cli 0.1.0 to 0.2.0");
        assert_eq!(installed.bytes().as_deref(), Some(&b"new binary"[..]));
        assert_eq!(installed.version(), Some(version("0.2.0")));
        let notes = report.notes.expect("an update shows the release notes");
        assert_eq!(notes.heading, "Changes in cad-cli 0.2.0");
        assert!(notes.body.contains("show release notes after an update"));
        assert!(!notes.body.contains("Downloads"));
    }

    #[tokio::test]
    async fn a_failed_install_step_reports_no_update() {
        let server = MockServer::start().await;
        let target = Target::current().expect("a published target");
        let release = version("0.2.0");
        let bytes = archive(target, &release, b"new binary");
        let sums = sums_line(target, &release, &bytes);
        mount_release(&server, "v0.2.0", target, bytes, sums).await;
        let terminal = SilentTerminal;

        let error = update_args(&["cad-cli", "update"])
            .run_with(
                &CommandContext::new(&terminal, false),
                &source(&server),
                target,
                &version("0.1.0"),
                async |_: &Path, _: &Version| Err(CliError::Update("probe failed".to_owned())),
            )
            .await
            .expect_err("the install step fails");
        assert_eq!(error.to_string(), "update failed: probe failed");
    }

    #[tokio::test]
    async fn force_reinstalls_the_current_release() {
        let (result, installed) = run(&["cad-cli", "update", "--force"], "0.2.0", b"").await;
        let report = result.expect("the reinstall succeeds");

        assert_eq!(report.value["updateAvailable"], json!(false));
        assert_eq!(report.value["updated"], json!(true));
        assert_eq!(installed.bytes().as_deref(), Some(&b"new binary"[..]));
    }

    #[tokio::test]
    async fn a_checksum_mismatch_installs_nothing() {
        let (result, installed) =
            run(&["cad-cli", "update"], "0.1.0", b"a different archive").await;

        let error = result.expect_err("the digest differs");
        assert!(matches!(error, CliError::Update(_)), "{error}");
        assert_eq!(installed.bytes(), None);
    }
}
