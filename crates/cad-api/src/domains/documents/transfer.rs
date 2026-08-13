use std::{fmt, path::Path};

use futures_util::{StreamExt, stream};
use reqwest::header::CONTENT_LENGTH;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

use crate::{error::transport_message, ids::DocumentId};

use super::{
    DocumentTransferError, DocumentTransferStage, DocumentUploadRequest, PresignedDocument,
    control_plane::DocumentControlPlane,
};

const TRANSFER_BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug)]
pub(super) struct StorageError {
    message: String,
}

impl StorageError {
    fn request(error: &reqwest::Error) -> Self {
        Self {
            message: format!("transport failed: {}", transport_message(error)),
        }
    }

    fn io(error: &std::io::Error) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StorageError {}

pub(super) trait ObjectStorage {
    async fn upload(
        &self,
        url: &Url,
        source: tokio::fs::File,
        content_length: u64,
    ) -> Result<(), StorageError>;

    async fn download(
        &self,
        url: &Url,
        destination: &mut tokio::fs::File,
    ) -> Result<(), StorageError>;
}

#[derive(Clone, Copy)]
pub(super) struct ReqwestStorage<'client> {
    client: &'client reqwest::Client,
}

impl<'client> ReqwestStorage<'client> {
    pub(super) const fn new(client: &'client reqwest::Client) -> Self {
        Self { client }
    }
}

impl ObjectStorage for ReqwestStorage<'_> {
    async fn upload(
        &self,
        url: &Url,
        source: tokio::fs::File,
        content_length: u64,
    ) -> Result<(), StorageError> {
        let stream = stream::try_unfold(source, |mut source| async move {
            let mut buffer = vec![0_u8; TRANSFER_BUFFER_SIZE];
            let read = source.read(&mut buffer).await?;
            if read == 0 {
                Ok::<Option<(Vec<u8>, tokio::fs::File)>, std::io::Error>(None)
            } else {
                buffer.truncate(read);
                Ok::<Option<(Vec<u8>, tokio::fs::File)>, std::io::Error>(Some((buffer, source)))
            }
        });
        self.client
            .put(url.clone())
            .header(CONTENT_LENGTH, content_length)
            .body(reqwest::Body::wrap_stream(stream))
            .send()
            .await
            .map_err(|error| StorageError::request(&error))?
            .error_for_status()
            .map_err(|error| StorageError::request(&error))?;
        Ok(())
    }

    async fn download(
        &self,
        url: &Url,
        destination: &mut tokio::fs::File,
    ) -> Result<(), StorageError> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .await
            .map_err(|error| StorageError::request(&error))?
            .error_for_status()
            .map_err(|error| StorageError::request(&error))?;
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|error| StorageError::request(&error))?;
            destination
                .write_all(&chunk)
                .await
                .map_err(|error| StorageError::io(&error))?;
        }
        destination
            .flush()
            .await
            .map_err(|error| StorageError::io(&error))
    }
}

pub(super) async fn upload_file<C, S>(
    control_plane: &C,
    storage: &S,
    input: &DocumentUploadRequest,
    source: &Path,
) -> Result<PresignedDocument, DocumentTransferError>
where
    C: DocumentControlPlane,
    S: ObjectStorage,
{
    let file = tokio::fs::File::open(source).await.map_err(|error| {
        DocumentTransferError::io(DocumentTransferStage::OpenUploadSource, None, &error)
    })?;
    let content_length = file
        .metadata()
        .await
        .map_err(|error| {
            DocumentTransferError::io(DocumentTransferStage::OpenUploadSource, None, &error)
        })?
        .len();
    let descriptor = control_plane.prepare_upload(input).await.map_err(|error| {
        DocumentTransferError::api(DocumentTransferStage::PrepareUpload, None, &error)
    })?;
    storage
        .upload(&descriptor.url, file, content_length)
        .await
        .map_err(|error| {
            DocumentTransferError::transfer(DocumentTransferStage::Upload, descriptor.id, &error)
        })?;
    Ok(descriptor)
}

pub(super) async fn download_file<C, S>(
    control_plane: &C,
    storage: &S,
    id: DocumentId,
    destination: &Path,
) -> Result<PresignedDocument, DocumentTransferError>
where
    C: DocumentControlPlane,
    S: ObjectStorage,
{
    let descriptor = control_plane.prepare_download(id).await.map_err(|error| {
        DocumentTransferError::api(DocumentTransferStage::PrepareDownload, Some(id), &error)
    })?;
    let mut file = tokio::fs::File::create(destination)
        .await
        .map_err(|error| {
            DocumentTransferError::io(
                DocumentTransferStage::CreateDownloadDestination,
                Some(id),
                &error,
            )
        })?;
    storage
        .download(&descriptor.url, &mut file)
        .await
        .map_err(|error| {
            DocumentTransferError::transfer(DocumentTransferStage::Download, id, &error)
        })?;
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    use url::Url;
    use uuid::Uuid;

    use crate::{Result as ApiResult, ids::DocumentId};

    use super::{
        DocumentControlPlane, DocumentTransferStage, DocumentUploadRequest, ObjectStorage,
        PresignedDocument, StorageError, upload_file,
    };

    struct NeverControlPlane;

    impl DocumentControlPlane for NeverControlPlane {
        async fn prepare_upload(
            &self,
            _input: &DocumentUploadRequest,
        ) -> ApiResult<PresignedDocument> {
            panic!("control plane must not run before opening the source")
        }

        async fn prepare_download(&self, _id: DocumentId) -> ApiResult<PresignedDocument> {
            panic!("not used by this test")
        }
    }

    struct NeverStorage;

    impl ObjectStorage for NeverStorage {
        async fn upload(
            &self,
            _url: &Url,
            _source: tokio::fs::File,
            _content_length: u64,
        ) -> Result<(), StorageError> {
            panic!("storage must not run before opening the source")
        }

        async fn download(
            &self,
            _url: &Url,
            _destination: &mut tokio::fs::File,
        ) -> Result<(), StorageError> {
            panic!("not used by this test")
        }
    }

    /// Ordering guarantee expressed through the private seams: the fakes panic
    /// if the transfer reaches them, which no black-box test could observe.
    /// The headers a presigned PUT may carry are covered end to end in
    /// `tests/documents.rs`.
    #[tokio::test]
    async fn upload_opens_source_before_creating_a_remote_document() {
        let missing = std::env::temp_dir().join(format!("cad-missing-{}", Uuid::new_v4()));
        let error = upload_file(
            &NeverControlPlane,
            &NeverStorage,
            &DocumentUploadRequest::new("missing.pdf"),
            &missing,
        )
        .await
        .expect_err("missing source must fail");

        assert_eq!(error.stage(), DocumentTransferStage::OpenUploadSource);
        assert_eq!(error.document_id(), None);
    }
}
