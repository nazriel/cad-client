//! Presigned document upload and download.
//!
//! Platform authentication is used only to obtain a presigned descriptor.
//! The file transfer itself uses the core's separate storage client, which
//! does not send the session bearer.

use std::path::Path;

use crate::{Client, CompanyScoped, Result, client::ClientCore, ids::DocumentId};

mod control_plane;
mod model;
mod requests;
mod transfer;

pub use model::{
    DocumentTransferError, DocumentTransferStage, DocumentUploadRequest, PresignedDocument,
};

use control_plane::{ControlPlane, DocumentControlPlane};
use transfer::{ReqwestStorage, download_file, upload_file};

/// Document-transfer operations.
#[derive(Clone, Copy)]
pub struct Documents<'client> {
    control_plane: ControlPlane<'client>,
    storage: ReqwestStorage<'client>,
}

impl<'client> Documents<'client> {
    pub(crate) fn new(core: &'client ClientCore) -> Self {
        Self {
            control_plane: ControlPlane::new(core),
            storage: ReqwestStorage::new(core.storage_client()),
        }
    }

    /// Creates a document record and obtains a presigned upload URL.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn prepare_upload(&self, input: &DocumentUploadRequest) -> Result<PresignedDocument> {
        self.control_plane.prepare_upload(input).await
    }

    /// Obtains a presigned download URL for an existing document.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn prepare_download(&self, id: DocumentId) -> Result<PresignedDocument> {
        self.control_plane.prepare_download(id).await
    }

    /// Streams a local file to unauthenticated external storage.
    ///
    /// The local file is opened before creating the remote document record.
    /// If the storage PUT fails, the returned error contains the created
    /// document identifier and no automatic cleanup is attempted.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentTransferError`] for local I/O, API, and storage failures.
    pub async fn upload_file(
        &self,
        input: &DocumentUploadRequest,
        source: impl AsRef<Path>,
    ) -> std::result::Result<PresignedDocument, DocumentTransferError> {
        upload_file(&self.control_plane, &self.storage, input, source.as_ref()).await
    }

    /// Streams a document from unauthenticated external storage to a local file.
    ///
    /// Callers that require atomic replacement should pass a temporary path and
    /// rename it after this method succeeds.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentTransferError`] for local I/O, API, and storage failures.
    pub async fn download_file(
        &self,
        id: DocumentId,
        destination: impl AsRef<Path>,
    ) -> std::result::Result<PresignedDocument, DocumentTransferError> {
        download_file(&self.control_plane, &self.storage, id, destination.as_ref()).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses presigned document transfer operations.
    #[must_use]
    pub fn documents(&self) -> Documents<'_> {
        Documents::new(self.core())
    }
}
