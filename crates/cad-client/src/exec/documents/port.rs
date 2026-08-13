use std::path::Path;

use cad_api::domains::documents::{DocumentUploadRequest, PresignedDocument};
use cad_api::ids::DocumentId;

use crate::error::CliError;

pub(crate) trait DocumentsPort: Send + Sync {
    async fn upload_document(
        &self,
        input: &DocumentUploadRequest,
        source: &Path,
    ) -> Result<PresignedDocument, CliError>;

    async fn download_document(
        &self,
        id: DocumentId,
        destination: &Path,
    ) -> Result<PresignedDocument, CliError>;
}
