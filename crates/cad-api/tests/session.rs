//! Machine API-key authentication behaviour observed through the public client API.

mod support;

use cad_api::ids::CompanyId;
use cad_api::{ApiKeyV1Credentials, Client, ServerConfig};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::header;

const IDENTITY_PATH: &str = "/api/auth/v2/machine/identity";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

fn config(server: &MockServer) -> ServerConfig {
    ServerConfig::insecure_loopback(server.uri()).expect("the mock server is loopback HTTP")
}

fn identity(company_id: Option<CompanyId>) -> serde_json::Value {
    serde_json::json!({
        "principal": {
            "userId": Uuid::nil(),
            "affiliatedCompanyId": Uuid::nil()
        },
        "companySelection": {
            "resolvedCompanyId": company_id,
            "source": "HEADER"
        }
    })
}

#[tokio::test]
async fn machine_key_authenticates_identity_and_resource_requests() {
    let api_key = Uuid::new_v4();
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(None)))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let connected = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect(ApiKeyV1Credentials::new(api_key, "secret-value"))
        .await
        .expect("machine identity succeeds");
    connected
        .companies()
        .await
        .expect("companies request succeeds");

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(
            header(request, "authorization").as_deref(),
            Some(format!("bearer {api_key}.secret-value").as_str())
        );
    }
}

#[tokio::test]
async fn company_scope_uses_header_and_validates_resolution() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(move |request: &wiremock::Request| {
            let selected = request
                .headers
                .get("X-CAD-Company-Id")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value == company_id.to_string());
            ResponseTemplate::new(200).set_body_json(identity(selected.then_some(company_id)))
        })
        .expect(2)
        .mount(&server)
        .await;

    let connected = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect(ApiKeyV1Credentials::new(Uuid::new_v4(), "secret"))
        .await
        .expect("machine identity succeeds");
    let scoped = connected
        .for_company(company_id)
        .await
        .expect("company scope succeeds");
    assert_eq!(scoped.company_id(), company_id);

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(
        header(&requests[1], "x-cad-company-id").as_deref(),
        Some(company_id.to_string().as_str())
    );
}
