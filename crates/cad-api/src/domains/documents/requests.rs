use reqwest::Method;

use crate::{ids::DocumentId, protocol::RequestSpec};

use super::{DocumentUploadRequest, PresignedDocument};

const PREPARE_UPLOAD_PATH: &str = "/api/documents/v1/pre-signed-write";
const DOCUMENTS_PATH: &str = "/api/documents/v1/documents";

pub(super) fn prepare_upload(
    input: &DocumentUploadRequest,
) -> RequestSpec<&DocumentUploadRequest, PresignedDocument> {
    RequestSpec::new(Method::POST, PREPARE_UPLOAD_PATH, None, Some(input))
}

pub(super) fn prepare_download(id: DocumentId) -> RequestSpec<(), PresignedDocument> {
    RequestSpec::new(
        Method::GET,
        format!("{DOCUMENTS_PATH}/{id}/pre-signed-read"),
        None,
        None,
    )
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use crate::ids::DocumentId;

    use super::{DOCUMENTS_PATH, PREPARE_UPLOAD_PATH, prepare_download, prepare_upload};
    use crate::domains::documents::DocumentUploadRequest;

    #[test]
    fn recipes_preserve_paths_methods_and_upload_casing() {
        let input = DocumentUploadRequest::new("evidence.pdf");
        let upload = prepare_upload(&input);
        assert_eq!(upload.method, Method::POST);
        assert_eq!(upload.path, PREPARE_UPLOAD_PATH);
        assert_eq!(
            serde_json::to_value(upload.body).expect("serializable request"),
            json!({
                "fileName": "evidence.pdf",
                "entityIds": [],
                "companyId": null,
                "emissionFactorId": null
            })
        );

        let id = DocumentId::from(Uuid::nil());
        let download = prepare_download(id);
        assert_eq!(download.method, Method::GET);
        assert_eq!(
            download.path,
            format!("{DOCUMENTS_PATH}/{id}/pre-signed-read")
        );
        assert!(download.body.is_none());
    }
}
