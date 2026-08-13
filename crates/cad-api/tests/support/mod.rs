//! Fixtures shared by the integration tests.
//!
//! Integration tests see only the crate's public API, so the `cfg(test)` helper
//! in `src/auth.rs` is out of reach and is reproduced here instead.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use cad_api::ids::CompanyId;
use uuid::Uuid;

/// An access token the SDK accepts: unsigned, carrying the claims it reads.
///
/// The SDK deliberately does not verify the signature — the server remains
/// authoritative for authorization — so an unsigned token is enough to drive
/// session state in a test.
pub fn unsigned_token(company_id: Option<CompanyId>, expires_at: i64) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        serde_json::json!({
            "exp": expires_at,
            "userId": Uuid::nil(),
            "com": company_id,
            "clientCompanyId": company_id,
            "isAdmin": false,
            "isExpert": true,
            "isReadonly": false,
            "isReadonlyLimited": false
        })
        .to_string(),
    );
    format!("{header}.{payload}.")
}

/// Header value of `name` on `request`, lowercased for case-insensitive checks.
pub fn header(request: &wiremock::Request, name: &str) -> Option<String> {
    request
        .headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase)
}
