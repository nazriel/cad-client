use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result, ids::CompanyId};

/// Credentials accepted by the API-key exchange endpoint.
pub struct ApiKeyV1Credentials {
    api_key: Uuid,
    secret: SecretString,
}

impl ApiKeyV1Credentials {
    #[must_use]
    pub fn new(api_key: Uuid, secret: impl Into<String>) -> Self {
        Self {
            api_key,
            secret: SecretString::from(secret.into()),
        }
    }

    #[must_use]
    pub const fn api_key(&self) -> Uuid {
        self.api_key
    }

    pub(crate) fn exposed_secret(&self) -> &str {
        self.secret.expose_secret()
    }
}

impl fmt::Debug for ApiKeyV1Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiKeyV1Credentials")
            .field("api_key", &"[REDACTED]")
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

/// A company visible to the authenticated user.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Company {
    pub id: CompanyId,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_consulting_company: bool,
    pub is_demo_company: bool,
    pub available_client_companies: Option<u64>,
    #[serde(rename = "companyStatus")]
    pub status: String,
}

/// Non-sensitive context decoded from a server-issued access token.
///
/// Claims are used only for client-side state and refresh timing. Server-side
/// authorization remains authoritative because the SDK does not verify the JWT
/// signature.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
#[allow(clippy::struct_excessive_bools)]
pub struct SessionContext {
    pub subject: Option<String>,
    pub user_id: Option<Uuid>,
    pub company_id: Option<CompanyId>,
    pub company_name: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub is_admin: bool,
    pub is_expert: bool,
    pub is_readonly: bool,
    pub is_readonly_limited: bool,
}

#[derive(Deserialize)]
#[allow(clippy::struct_excessive_bools)]
struct JwtClaims {
    exp: i64,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default, rename = "userId")]
    user_id: Option<Uuid>,
    #[serde(default)]
    com: Option<CompanyId>,
    #[serde(default, rename = "clientCompanyId")]
    client_company_id: Option<CompanyId>,
    #[serde(default, rename = "cna")]
    company_name: Option<String>,
    #[serde(default, rename = "isAdmin")]
    is_admin: bool,
    #[serde(default, rename = "isExpert")]
    is_expert: bool,
    #[serde(default, rename = "isReadonly")]
    is_readonly: bool,
    #[serde(default, rename = "isReadonlyLimited")]
    is_readonly_limited: bool,
}

pub(crate) struct TokenSet {
    access_token: SecretString,
    refresh_token: SecretString,
    context: SessionContext,
    expected_company: Option<CompanyId>,
}

impl TokenSet {
    pub(crate) fn from_pair(
        access_token: String,
        refresh_token: String,
        expected_company: Option<CompanyId>,
    ) -> Result<Self> {
        let context = decode_context(&access_token)?;
        validate_company(&context, expected_company)?;

        Ok(Self {
            access_token: SecretString::from(access_token),
            refresh_token: SecretString::from(refresh_token),
            context,
            expected_company,
        })
    }

    pub(crate) fn access_token(&self) -> &str {
        self.access_token.expose_secret()
    }

    pub(crate) fn refresh_token(&self) -> &str {
        self.refresh_token.expose_secret()
    }

    pub(crate) fn context(&self) -> SessionContext {
        self.context.clone()
    }

    pub(crate) fn should_refresh(&self) -> bool {
        self.context.expires_at.timestamp() <= Utc::now().timestamp() + 30
    }

    pub(crate) fn replace_access(&mut self, access_token: String) -> Result<()> {
        let context = decode_context(&access_token)?;
        validate_company(&context, self.expected_company)?;
        self.access_token = SecretString::from(access_token);
        self.context = context;
        Ok(())
    }

    pub(crate) fn bind_company(&mut self, company_id: CompanyId) -> Result<()> {
        validate_company(&self.context, Some(company_id))?;
        self.expected_company = Some(company_id);
        Ok(())
    }
}

fn decode_context(token: &str) -> Result<SessionContext> {
    let payload = token
        .split('.')
        .nth(1)
        .ok_or_else(|| Error::protocol("access token is not a JWT"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| Error::protocol("access token contains an invalid JWT payload"))?;
    let claims: JwtClaims = serde_json::from_slice(&bytes)
        .map_err(|_| Error::protocol("access token contains invalid JWT claims"))?;

    let company_id = match (claims.com, claims.client_company_id) {
        (Some(short), Some(long)) if short != long => {
            return Err(Error::protocol(
                "access token contains conflicting company claims",
            ));
        }
        (Some(value), _) | (_, Some(value)) => Some(value),
        (None, None) => None,
    };
    let expires_at = DateTime::from_timestamp(claims.exp, 0)
        .ok_or_else(|| Error::protocol("access token expiration is out of range"))?;

    Ok(SessionContext {
        subject: claims.sub,
        user_id: claims.user_id,
        company_id,
        company_name: claims.company_name,
        expires_at,
        is_admin: claims.is_admin,
        is_expert: claims.is_expert,
        is_readonly: claims.is_readonly,
        is_readonly_limited: claims.is_readonly_limited,
    })
}

fn validate_company(context: &SessionContext, expected: Option<CompanyId>) -> Result<()> {
    if let Some(expected) = expected
        && context.company_id != Some(expected)
    {
        return Err(Error::protocol(format!(
            "server-issued token is scoped to {:?}, expected {expected}",
            context.company_id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsigned_token(company_id: Option<CompanyId>, expires_at: i64) -> String {
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

    #[test]
    fn credentials_never_debug_their_values() {
        let credentials = ApiKeyV1Credentials::new(Uuid::nil(), "very-secret");
        let debug = format!("{credentials:?}");
        assert!(!debug.contains("very-secret"));
        assert!(!debug.contains(&Uuid::nil().to_string()));
    }

    #[test]
    fn decodes_company_context_without_trusting_it_for_authorization() {
        let company_id = CompanyId::from_uuid(Uuid::new_v4());
        let context = decode_context(&unsigned_token(
            Some(company_id),
            Utc::now().timestamp() + 300,
        ))
        .unwrap();
        assert_eq!(context.company_id, Some(company_id));
    }
}
