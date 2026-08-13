//! Session behaviour observed through the public client API.
//!
//! These exercise the credential exchange, the company switch and the proactive
//! refresh against a mock HTTP server, so they belong here rather than beside
//! the code: nothing in them needs to see a private item.

mod support;

use cad_api::ids::CompanyId;
use cad_api::{ApiKeyV1Credentials, Client, ServerConfig};
use chrono::Utc;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use support::{header, unsigned_token};

const EXCHANGE_PATH: &str = "/api/keys/v1/api/auth";
const SWITCH_PATH: &str = "/api/auth/token/switch";
const REFRESH_PATH: &str = "/api/auth/token/refresh";
const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

fn config(server: &MockServer) -> ServerConfig {
    ServerConfig::insecure_loopback(server.uri()).expect("the mock server is loopback HTTP")
}

fn body_of(request: &wiremock::Request) -> String {
    String::from_utf8(request.body.clone()).expect("request body is UTF-8")
}

#[tokio::test]
async fn exchange_and_switch_preserve_wire_casing_and_session_independence() {
    let initial_company = CompanyId::from_uuid(Uuid::new_v4());
    let switched_company = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(EXCHANGE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": unsigned_token(Some(initial_company), Utc::now().timestamp() + 5 * 60),
            "tokenType": "Bearer",
            "refreshToken": "initial-refresh"
        })))
        .expect(1)
        .mount(&server)
        .await;

    // The platform answers the switch in snake_case while the exchange answers
    // in camelCase. Both spellings must decode, or a company switch breaks.
    Mock::given(method("POST"))
        .and(path(SWITCH_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": unsigned_token(Some(switched_company), Utc::now().timestamp() + 5 * 60),
            "token_type": "Bearer",
            "refresh_token": "switched-refresh"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let api_key = Uuid::new_v4();
    let connected = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect(ApiKeyV1Credentials::new(api_key, "secret-value"))
        .await
        .expect("credential exchange succeeds");
    let switched = connected
        .for_company(switched_company)
        .await
        .expect("company switch succeeds");

    // Switching must mint an independent session rather than mutate the first.
    assert_eq!(connected.context().await.company_id, Some(initial_company));
    assert_eq!(switched.context().await.company_id, Some(switched_company));

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    assert_eq!(requests.len(), 2);

    let exchange = &requests[0];
    assert_eq!(exchange.url.path(), EXCHANGE_PATH);
    let exchange_body = body_of(exchange);
    assert!(exchange_body.contains(&format!(r#""apiKey":"{api_key}""#)));
    assert!(exchange_body.contains(r#""secret":"secret-value""#));
    assert!(
        header(exchange, "authorization").is_none(),
        "the key exchange has no bearer to send"
    );

    let switch = &requests[1];
    assert_eq!(switch.url.path(), SWITCH_PATH);
    assert!(body_of(switch).contains(&format!(r#""companyId":"{switched_company}""#)));
    assert!(
        header(switch, "authorization")
            .expect("the switch is authenticated")
            .starts_with("bearer ")
    );
}

#[tokio::test]
async fn concurrent_requests_share_one_proactive_refresh() {
    let company_id = CompanyId::from_uuid(Uuid::new_v4());
    let server = MockServer::start().await;

    // An access token this close to expiry makes both callers want a refresh.
    Mock::given(method("POST"))
        .and(path(EXCHANGE_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accessToken": unsigned_token(Some(company_id), Utc::now().timestamp() + 5),
            "tokenType": "Bearer",
            "refreshToken": "the-refresh-token"
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(REFRESH_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": unsigned_token(Some(company_id), Utc::now().timestamp() + 5 * 60),
            "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path(COMPANIES_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(2)
        .mount(&server)
        .await;

    let connected = Client::new(config(&server))
        .expect("valid loopback configuration")
        .connect(ApiKeyV1Credentials::new(Uuid::new_v4(), "secret"))
        .await
        .expect("credential exchange succeeds");
    let other = connected.clone();

    let (first, second) = tokio::join!(connected.companies(), other.companies());
    assert!(first.is_ok());
    assert!(second.is_ok());

    let requests = server
        .received_requests()
        .await
        .expect("the mock server records requests");
    let refreshes: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path() == REFRESH_PATH)
        .collect();
    assert_eq!(
        refreshes.len(),
        1,
        "two concurrent callers must not each refresh"
    );
    assert_eq!(
        header(refreshes[0], "authorization").as_deref(),
        Some("bearer the-refresh-token"),
        "refresh authenticates with the refresh token, not the access token"
    );
}
