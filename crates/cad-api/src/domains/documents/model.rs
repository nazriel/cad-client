use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

use crate::{
    Error,
    ids::{CompanyId, DocumentId},
};

/// Request used to create a document record and obtain its upload URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentUploadRequest {
    /// Original file name stored with the document.
    pub file_name: String,
    /// Entities to which the document should be attached.
    #[serde(default)]
    pub entity_ids: Vec<Uuid>,
    /// Explicit document-owning company, when applicable.
    #[serde(default)]
    pub company_id: Option<CompanyId>,
    /// Emission factor to which the document should be attached.
    #[serde(default)]
    pub emission_factor_id: Option<Uuid>,
}

impl DocumentUploadRequest {
    /// Creates an unattached document upload request.
    #[must_use]
    pub fn new(file_name: impl Into<String>) -> Self {
        Self {
            file_name: file_name.into(),
            entity_ids: Vec::new(),
            company_id: None,
            emission_factor_id: None,
        }
    }
}

/// A document identifier and short-lived storage URL.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresignedDocument {
    /// Short-lived external storage URL. Never log its query string.
    pub url: Url,
    /// Platform document identifier.
    pub id: DocumentId,
}

impl fmt::Debug for PresignedDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PresignedDocument")
            .field("id", &self.id)
            .field("url", &"<redacted presigned URL>")
            .finish()
    }
}

/// Stage at which a high-level document transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentTransferStage {
    /// Opening or inspecting the local upload source.
    OpenUploadSource,
    /// Creating the document record and presigned upload descriptor.
    PrepareUpload,
    /// Uploading bytes to external storage.
    Upload,
    /// Creating the presigned download descriptor.
    PrepareDownload,
    /// Creating the local download destination.
    CreateDownloadDestination,
    /// Downloading bytes from external storage.
    Download,
}

impl fmt::Display for DocumentTransferStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::OpenUploadSource => "opening upload source",
            Self::PrepareUpload => "preparing upload",
            Self::Upload => "uploading to storage",
            Self::PrepareDownload => "preparing download",
            Self::CreateDownloadDestination => "creating download destination",
            Self::Download => "downloading from storage",
        })
    }
}

/// Failure from a high-level document transfer.
#[derive(Debug, thiserror::Error)]
#[error("document transfer failed while {stage}: {message}")]
pub struct DocumentTransferError {
    stage: DocumentTransferStage,
    document_id: Option<DocumentId>,
    message: String,
}

impl DocumentTransferError {
    pub(super) fn api(
        stage: DocumentTransferStage,
        document_id: Option<DocumentId>,
        error: &Error,
    ) -> Self {
        Self::message(stage, document_id, error)
    }

    pub(super) fn transfer(
        stage: DocumentTransferStage,
        document_id: DocumentId,
        error: &impl fmt::Display,
    ) -> Self {
        Self::message(stage, Some(document_id), error)
    }

    pub(super) fn io(
        stage: DocumentTransferStage,
        document_id: Option<DocumentId>,
        error: &std::io::Error,
    ) -> Self {
        Self::message(stage, document_id, error)
    }

    fn message(
        stage: DocumentTransferStage,
        document_id: Option<DocumentId>,
        error: &impl fmt::Display,
    ) -> Self {
        Self {
            stage,
            document_id,
            message: error.to_string(),
        }
    }

    /// Failed transfer stage.
    #[must_use]
    pub const fn stage(&self) -> DocumentTransferStage {
        self.stage
    }

    /// Document already created by the platform, if known.
    ///
    /// A value is present for storage upload failures so callers can report
    /// the orphaned record without automatically deleting it.
    #[must_use]
    pub const fn document_id(&self) -> Option<DocumentId> {
        self.document_id
    }
}

#[cfg(test)]
mod tests {
    use url::Url;
    use uuid::Uuid;

    use crate::ids::DocumentId;

    use super::PresignedDocument;

    #[test]
    fn debug_redacts_the_entire_presigned_url() {
        let descriptor = PresignedDocument {
            id: DocumentId::from(Uuid::nil()),
            url: Url::parse("https://storage.invalid/object?X-Amz-Signature=secret")
                .expect("valid fixture URL"),
        };
        let debug = format!("{descriptor:?}");

        assert!(!debug.contains("secret"));
        assert!(!debug.contains("X-Amz"));
        assert!(debug.contains("redacted"));
    }
}
