use serde::{Deserialize, Serialize};

use crate::{ApiKeyV1Credentials, Error, Result};

use super::RequestSpec;

const API_KEY_EXCHANGE_PATH: &str = "/api/keys/v1/api/auth";
const ACCESS_TOKEN_REFRESH_PATH: &str = "/api/auth/token/refresh";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ApiKeyExchangeRequest<'credentials> {
    api_key: uuid::Uuid,
    secret: &'credentials str,
}

impl<'credentials> From<&'credentials ApiKeyV1Credentials> for ApiKeyExchangeRequest<'credentials> {
    fn from(credentials: &'credentials ApiKeyV1Credentials) -> Self {
        Self {
            api_key: credentials.api_key(),
            secret: credentials.exposed_secret(),
        }
    }
}

pub(crate) fn api_key_exchange(
    credentials: &ApiKeyV1Credentials,
) -> RequestSpec<ApiKeyExchangeRequest<'_>, ExchangeTokenResponse> {
    RequestSpec::new(
        reqwest::Method::POST,
        API_KEY_EXCHANGE_PATH,
        None,
        Some(ApiKeyExchangeRequest::from(credentials)),
    )
}

pub(crate) fn access_token_refresh() -> RequestSpec<(), AccessTokenResponse> {
    RequestSpec::new(reqwest::Method::GET, ACCESS_TOKEN_REFRESH_PATH, None, None)
}

/// Response of the API-key exchange, which answers in `camelCase`.
///
/// This type is field-for-field identical to [`TokenPairResponse`] and must
/// stay separate: the two endpoints disagree about wire casing, so merging them
/// would silently break one of them. The unit test named after the casing
/// asymmetry pins the difference.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExchangeTokenResponse {
    pub(crate) access_token: String,
    token_type: String,
    pub(crate) refresh_token: String,
}

impl ExchangeTokenResponse {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_bearer(&self.token_type)
    }
}

#[derive(Deserialize)]
pub(crate) struct AccessTokenResponse {
    pub(crate) access_token: String,
    token_type: String,
}

impl AccessTokenResponse {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_bearer(&self.token_type)
    }
}

/// Response of the company-switch endpoint, which answers in `snake_case` even
/// though its request body is camelCase.
///
/// See [`ExchangeTokenResponse`] for why the two look duplicated.
#[derive(Deserialize)]
pub(crate) struct TokenPairResponse {
    pub(crate) access_token: String,
    token_type: String,
    pub(crate) refresh_token: String,
}

impl TokenPairResponse {
    pub(crate) fn validate(&self) -> Result<()> {
        validate_bearer(&self.token_type)
    }
}

fn validate_bearer(token_type: &str) -> Result<()> {
    if token_type.eq_ignore_ascii_case("bearer") {
        Ok(())
    } else {
        Err(Error::protocol(format!(
            "unsupported token type `{token_type}`"
        )))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use uuid::Uuid;

    use crate::ApiKeyV1Credentials;

    use super::{
        ACCESS_TOKEN_REFRESH_PATH, API_KEY_EXCHANGE_PATH, ExchangeTokenResponse, TokenPairResponse,
        access_token_refresh, api_key_exchange,
    };

    #[test]
    fn exchange_request_uses_the_camel_case_contract() {
        let credentials = ApiKeyV1Credentials::new(Uuid::nil(), "secret-value");
        let exchange = api_key_exchange(&credentials);
        assert_eq!(exchange.method, reqwest::Method::POST);
        assert_eq!(exchange.path, API_KEY_EXCHANGE_PATH);
        assert_eq!(
            serde_json::to_value(exchange.body).unwrap(),
            json!({"apiKey": Uuid::nil(), "secret": "secret-value"})
        );

        let refresh = access_token_refresh();
        assert_eq!(refresh.method, reqwest::Method::GET);
        assert_eq!(refresh.path, ACCESS_TOKEN_REFRESH_PATH);
        assert!(refresh.body.is_none());
    }

    /// The two token-pair DTOs have identical fields but different wire casing.
    /// Collapsing them into one type would break whichever endpoint lost its
    /// casing, so the asymmetry is pinned here rather than left to a comment.
    #[test]
    fn casing_asymmetry_between_exchange_and_switch_is_pinned() {
        let camel = json!({
            "accessToken": "a",
            "tokenType": "Bearer",
            "refreshToken": "r"
        });
        let snake = json!({
            "access_token": "a",
            "token_type": "Bearer",
            "refresh_token": "r"
        });

        assert!(serde_json::from_value::<ExchangeTokenResponse>(camel.clone()).is_ok());
        assert!(serde_json::from_value::<ExchangeTokenResponse>(snake.clone()).is_err());
        assert!(serde_json::from_value::<TokenPairResponse>(snake).is_ok());
        assert!(serde_json::from_value::<TokenPairResponse>(camel).is_err());
    }
}
