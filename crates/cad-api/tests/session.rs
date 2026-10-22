//! Machine API-key authentication behaviour observed through the public client API.

mod support;

use cad_api::ids::{CompanyId, UserId};
use cad_api::{ApiKeyV2Credentials, Client, Error, ServerConfig};
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
        .connect(ApiKeyV2Credentials::new(api_key, "secret-value"))
        .await
        .expect("machine identity succeeds");
    connected
        .companies()
        .list()
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
        .connect(ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"))
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

fn company_echoing_identity(
    company_id: CompanyId,
) -> impl Fn(&wiremock::Request) -> ResponseTemplate {
    move |request| {
        let selected = request
            .headers
            .get("X-CAD-Company-Id")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value == company_id.to_string());
        ResponseTemplate::new(200).set_body_json(identity(selected.then_some(company_id)))
    }
}

#[tokio::test]
async fn connecting_for_a_company_sends_the_header_in_one_request() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(company_echoing_identity(company_id))
        .expect(1)
        .mount(&server)
        .await;

    let scoped = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            company_id,
        )
        .await
        .expect("company scope succeeds");
    assert_eq!(scoped.company_id(), company_id);
    assert_eq!(
        scoped.context().company_selection.resolved_company_id,
        Some(company_id)
    );

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        header(&requests[0], "x-cad-company-id").as_deref(),
        Some(company_id.to_string().as_str())
    );
}

#[tokio::test]
async fn connecting_for_a_company_fails_when_another_company_is_resolved() {
    let requested = CompanyId::from_uuid(Uuid::new_v4());
    let resolved = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(identity(Some(resolved))))
        .expect(1)
        .mount(&server)
        .await;

    let error = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            requested,
        )
        .await
        .expect_err("a different resolved company must fail");
    assert!(
        matches!(error, Error::Protocol { .. }),
        "unexpected error: {error:?}"
    );
    assert!(error.to_string().contains(&resolved.to_string()));
    assert!(error.to_string().contains(&requested.to_string()));
}

#[tokio::test]
async fn connecting_for_a_company_with_a_rejected_key_is_an_authentication_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"msg": "no"})))
        .expect(1)
        .mount(&server)
        .await;

    let error = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            CompanyId::from_uuid(Uuid::new_v4()),
        )
        .await
        .expect_err("a rejected key must fail");
    assert!(
        matches!(error, Error::Authentication { .. }),
        "unexpected error: {error:?}"
    );
}

#[tokio::test]
async fn unscoped_drops_the_company_without_a_request() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(company_echoing_identity(company_id))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;

    let scoped = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            company_id,
        )
        .await
        .expect("company scope succeeds");
    let connected = scoped.unscoped();
    assert_eq!(
        connected.context().company_selection.resolved_company_id,
        Some(company_id),
        "the context is the one the scoped connection received"
    );
    connected
        .companies()
        .list()
        .await
        .expect("companies request succeeds");

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(requests.len(), 2, "unscoped() makes no request");
    assert_eq!(header(&requests[1], "x-cad-company-id"), None);
}

#[tokio::test]
async fn users_are_read_from_the_user_resource_in_either_state() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let user_id = UserId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(company_echoing_identity(company_id))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/accounts/v1/users/{user_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": user_id,
            "email": "ada@example.com",
            "firstName": "Ada",
            "lastName": "Lovelace",
            "locale": "en"
        })))
        .expect(2)
        .mount(&server)
        .await;

    let scoped = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            company_id,
        )
        .await
        .expect("company scope succeeds");
    let from_scoped = scoped.users().get(user_id).await.expect("scoped read");
    let from_connected = scoped
        .unscoped()
        .users()
        .get(user_id)
        .await
        .expect("connected read");
    assert_eq!(from_scoped.email.as_deref(), Some("ada@example.com"));
    assert_eq!(from_connected.id, user_id);
}

#[tokio::test]
async fn connecting_for_a_company_the_key_cannot_use_stays_an_api_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(IDENTITY_PATH))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({"msg": "no"})))
        .expect(1)
        .mount(&server)
        .await;

    let error = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect_for_company(
            ApiKeyV2Credentials::new(Uuid::new_v4(), "secret"),
            CompanyId::from_uuid(Uuid::new_v4()),
        )
        .await
        .expect_err("a company the key cannot use must fail");
    assert!(
        matches!(error, Error::Api { status: 403, .. }),
        "unexpected error: {error:?}"
    );
}
