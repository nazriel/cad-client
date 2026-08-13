use crate::{Result, client::ClientCore, ids::DocumentId};

use super::{DocumentUploadRequest, PresignedDocument, requests};

pub(super) trait DocumentControlPlane {
    async fn prepare_upload(&self, input: &DocumentUploadRequest) -> Result<PresignedDocument>;

    async fn prepare_download(&self, id: DocumentId) -> Result<PresignedDocument>;
}

#[derive(Clone, Copy)]
pub(super) struct ControlPlane<'client> {
    core: &'client ClientCore,
}

impl<'client> ControlPlane<'client> {
    pub(super) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }
}

impl DocumentControlPlane for ControlPlane<'_> {
    async fn prepare_upload(&self, input: &DocumentUploadRequest) -> Result<PresignedDocument> {
        self.core.execute(requests::prepare_upload(input)).await
    }

    async fn prepare_download(&self, id: DocumentId) -> Result<PresignedDocument> {
        self.core.execute(requests::prepare_download(id)).await
    }
}
