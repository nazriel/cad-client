use std::fmt;

use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::CompanyId;

/// Credentials accepted by the machine API-key authentication scheme.
pub struct ApiKeyV2Credentials {
    api_key: Uuid,
    secret: SecretString,
}

impl ApiKeyV2Credentials {
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

    pub(crate) fn bearer_token(&self) -> String {
        format!("{}.{}", self.api_key, self.secret.expose_secret())
    }
}

impl fmt::Debug for ApiKeyV2Credentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ApiKeyV2Credentials")
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

/// Machine identity and resolved company context returned by the platform.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SessionContext {
    pub principal: MachinePrincipal,
    pub company_selection: MachineCompanySelection,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MachinePrincipal {
    pub user_id: Uuid,
    pub affiliated_company_id: CompanyId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct MachineCompanySelection {
    pub resolved_company_id: Option<CompanyId>,
    pub source: CompanySelectionSource,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum CompanySelectionSource {
    Header,
    LegacyClaim,
    LastUsed,
    Default,
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_never_debug_their_values() {
        let credentials = ApiKeyV2Credentials::new(Uuid::nil(), "very-secret");
        let debug = format!("{credentials:?}");
        assert!(!debug.contains("very-secret"));
        assert!(!debug.contains(&Uuid::nil().to_string()));
    }

    #[test]
    fn bearer_token_joins_key_and_secret() {
        let credentials = ApiKeyV2Credentials::new(Uuid::nil(), "very-secret");
        assert_eq!(
            credentials.bearer_token(),
            format!("{}.very-secret", Uuid::nil())
        );
    }
}
