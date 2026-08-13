//! Presigned document transfer observed end to end.
//!
//! The point of these is the boundary between the two HTTP clients: the control
//! plane is authenticated, the storage transfer must not be. Driving it through
//! the public `documents()` facade proves the property for the path callers
//! actually take, not just for the internal storage seam.

mod support;

use cad_api::domains::documents::DocumentUploadRequest;
use cad_api::ids::CompanyId;
use cad_api::{ApiKeyV1Credentials, Client, CompanyScoped, ServerConfig};
use chrono::Utc;
use std::io::Write as _;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{header, unsigned_token};

const EXCHANGE_PATH: &str = "/api/keys/v1/api/auth";
const PREPARE_UPLOAD_PATH: &str = "/api/documents/v1/pre-signed-write";
const STORAGE_PATH: &str = "/upload";

async fn connect(server: &MockServer, company_id: CompanyId) -> Client<CompanyScoped> {
    Mock::given(method("POST"))
        .and(path(EXCHANGE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": unsigned_token(Some(company_id), Utc::now().timestamp() + 5 * 60),
            "tokenType": "Bearer",
            "refreshToken": "refresh"
        })))
        .mount(server)
        .await;

    Client::new(ServerConfig::insecure_loopback(server.uri()).expect("loopback HTTP"))
        .expect("valid loopback configuration")
        .connect(ApiKeyV1Credentials::new(Uuid::new_v4(), "secret"))
        .await
        .expect("credential exchange succeeds")
        .scope_current_company()
        .await
        .expect("the token carries a company")
}

#[tokio::test]
async fn presigned_upload_never_carries_platform_credentials_to_storage() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let document_id = Uuid::new_v4();
    let server = MockServer::start().await;
    let client = connect(&server, company_id).await;

    // The storage URL is a bearer credential in query-string form. It is served
    // by the same fixture on a different path so both requests are recorded.
    let storage_url = format!("{}{STORAGE_PATH}?signature=secret", server.uri());
    Mock::given(method("POST"))
        .and(path(PREPARE_UPLOAD_PATH))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"url": storage_url, "id": document_id})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(STORAGE_PATH))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let mut source = tempfile::NamedTempFile::new().expect("create upload source");
    source.write_all(b"abc").expect("write upload source");

    let descriptor = client
        .documents()
        .upload_file(&DocumentUploadRequest::new("evidence.pdf"), source.path())
        .await
        .expect("upload succeeds");
    assert_eq!(descriptor.id.into_uuid(), document_id);

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    let prepare = requests
        .iter()
        .find(|request| request.url.path() == PREPARE_UPLOAD_PATH)
        .expect("the control plane was called");
    assert!(
        header(prepare, "authorization").is_some_and(|value| value.starts_with("bearer ")),
        "the control plane call is authenticated"
    );

    let put = requests
        .iter()
        .find(|request| request.url.path() == STORAGE_PATH)
        .expect("storage received the object");
    assert_eq!(put.method, wiremock::http::Method::PUT);
    assert_eq!(put.url.query(), Some("signature=secret"));
    assert_eq!(header(put, "content-length").as_deref(), Some("3"));
    assert_eq!(put.body, b"abc");
    assert!(
        header(put, "authorization").is_none(),
        "the platform bearer must never reach object storage"
    );
}
