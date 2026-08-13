use cad_api::domains::documents::{
    DocumentTransferError, DocumentUploadRequest, PresignedDocument,
};
use cad_api::ids::DocumentId;

use super::SdkAdapter;
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;

impl DocumentsPort for SdkAdapter {
    async fn upload_document(
        &self,
        input: &DocumentUploadRequest,
        source: &std::path::Path,
    ) -> Result<PresignedDocument, CliError> {
        self.client
            .documents()
            .upload_file(input, source)
            .await
            .map_err(|error| map_transfer_error(&error))
    }

    async fn download_document(
        &self,
        id: DocumentId,
        destination: &std::path::Path,
    ) -> Result<PresignedDocument, CliError> {
        self.client
            .documents()
            .download_file(id, destination)
            .await
            .map_err(|error| map_transfer_error(&error))
    }
}

fn map_transfer_error(error: &DocumentTransferError) -> CliError {
    let document = error
        .document_id()
        .map_or_else(String::new, |id| format!("; documentId={id}"));
    CliError::transport(format!("{error}{document}"))
}
