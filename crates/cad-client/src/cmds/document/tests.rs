//! The download workflow around the transfer itself: where the bytes land, and
//! what is left behind when the transfer fails. Both need the private
//! `DocumentsPort` seam, so they stay in the crate rather than in `tests/`.

use std::path::PathBuf;
use std::sync::Mutex;

use cad_api::domains::documents::{DocumentUploadRequest, PresignedDocument};
use cad_api::ids::DocumentId;
use uuid::Uuid;

use super::download::{Options, run};
use crate::app::context::CommandContext;
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;
use crate::support::terminal::SilentTerminal;

/// Writes what a real transfer would have streamed, so the test can assert
/// where the bytes end up rather than that a mock was called.
struct FakeDocuments {
    payload: Option<&'static [u8]>,
    seen: Mutex<Vec<PathBuf>>,
}

impl FakeDocuments {
    fn writing(payload: &'static [u8]) -> Self {
        Self {
            payload: Some(payload),
            seen: Mutex::new(Vec::new()),
        }
    }

    fn failing() -> Self {
        Self {
            payload: None,
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl DocumentsPort for FakeDocuments {
    async fn upload_document(
        &self,
        _input: &DocumentUploadRequest,
        _source: &std::path::Path,
    ) -> Result<PresignedDocument, CliError> {
        unreachable!("download only")
    }

    async fn download_document(
        &self,
        id: DocumentId,
        destination: &std::path::Path,
    ) -> Result<PresignedDocument, CliError> {
        self.seen
            .lock()
            .expect("seen lock")
            .push(destination.to_path_buf());
        let Some(payload) = self.payload else {
            return Err(CliError::transport(
                "storage refused the transfer".to_owned(),
            ));
        };
        std::fs::write(destination, payload)?;
        Ok(PresignedDocument {
            url: "https://storage.example/object".parse().expect("valid url"),
            id,
        })
    }
}

fn siblings(directory: &std::path::Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .expect("readable directory")
        .map(|entry| entry.expect("directory entry").path())
        .collect()
}

#[tokio::test]
async fn a_successful_download_leaves_only_the_destination() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let out = directory.path().join("report.pdf");
    let port = FakeDocuments::writing(b"payload");
    let terminal = SilentTerminal;

    run(
        Options {
            id: DocumentId::from(Uuid::nil()),
            out: out.clone(),
            force: false,
        },
        &CommandContext::new(&terminal, true),
        &port,
    )
    .await
    .expect("the download succeeds");

    // The SDK is handed a temporary sibling, never the destination itself.
    let streamed_to = port.seen.lock().expect("seen lock").clone();
    assert_eq!(streamed_to.len(), 1);
    assert_ne!(streamed_to[0], out);

    assert_eq!(std::fs::read(&out).expect("destination"), b"payload");
    assert_eq!(siblings(directory.path()), vec![out]);
}

#[tokio::test]
async fn a_failed_download_leaves_the_directory_untouched() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let out = directory.path().join("report.pdf");
    let terminal = SilentTerminal;

    let error = run(
        Options {
            id: DocumentId::from(Uuid::nil()),
            out: out.clone(),
            force: false,
        },
        &CommandContext::new(&terminal, true),
        &FakeDocuments::failing(),
    )
    .await
    .expect_err("the transfer fails");

    assert!(matches!(error, CliError::Transport { .. }));
    assert!(!out.exists());
    assert!(
        siblings(directory.path()).is_empty(),
        "the temporary sibling must be removed on failure"
    );
}

#[tokio::test]
async fn an_existing_target_is_refused_without_force() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let out = directory.path().join("report.pdf");
    std::fs::write(&out, b"previous").expect("existing target");
    let terminal = SilentTerminal;

    let error = run(
        Options {
            id: DocumentId::from(Uuid::nil()),
            out: out.clone(),
            force: false,
        },
        &CommandContext::new(&terminal, true),
        &FakeDocuments::writing(b"payload"),
    )
    .await
    .expect_err("an existing target must not be overwritten");

    assert!(error.to_string().contains("--force"));
    assert_eq!(std::fs::read(&out).expect("destination"), b"previous");
}
