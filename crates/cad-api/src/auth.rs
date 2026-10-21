use std::fmt;

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::ids::{CompanyId, UserId};

/// Credentials accepted by the machine API-key authentication scheme.
pub struct ApiKeyV2Credentials {
    api_key: Uuid,
    secret: SecretString,
}

/// What the secret of an API key looks like, as the API keys page shows it.
const SECRET_PREFIX: &str = "cad_aks_";
const SECRET_BODY_LENGTH: usize = 43;

impl ApiKeyV2Credentials {
    #[must_use]
    pub fn new(api_key: Uuid, secret: impl Into<String>) -> Self {
        Self {
            api_key,
            secret: SecretString::from(secret.into()),
        }
    }

    /// Pairs a Key ID with a secret that is already held as a [`SecretString`].
    #[must_use]
    pub const fn from_secret(api_key: Uuid, secret: SecretString) -> Self {
        Self { api_key, secret }
    }

    /// Checks the Key ID the API keys page shows: a UUID.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] when `value` is not a UUID.
    pub fn parse_key_id(value: &str) -> Result<Uuid> {
        value.trim().parse().map_err(|_| Error::InvalidInput {
            message: "the Key ID must be a UUID, as shown on the API keys page".to_owned(),
        })
    }

    /// Checks the Secret the API keys page shows: `cad_aks_` followed by 43
    /// letters, digits, `-` or `_`. The value is never echoed in the error.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] when `value` has another form.
    pub fn parse_secret(value: &str) -> Result<SecretString> {
        let value = value.trim();
        let well_formed = value.strip_prefix(SECRET_PREFIX).is_some_and(|body| {
            body.len() == SECRET_BODY_LENGTH
                && body
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        });
        if well_formed {
            Ok(SecretString::from(value))
        } else {
            Err(Error::InvalidInput {
                message: format!(
                    "the Secret must be {SECRET_PREFIX} followed by {SECRET_BODY_LENGTH} letters, \
                     digits, '-' or '_', as shown on the API keys page"
                ),
            })
        }
    }

    /// Splits `<Key ID>.<Secret>`, the form of the bearer token, and checks
    /// both halves.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidInput`] when the dot is missing or either half
    /// is malformed. The value is never echoed in the error.
    pub fn parse_token(value: &str) -> Result<(Uuid, SecretString)> {
        let (key_id, secret) = value
            .trim()
            .split_once('.')
            .ok_or_else(|| Error::InvalidInput {
                message: "expected <Key ID>.<Secret>".to_owned(),
            })?;
        Ok((Self::parse_key_id(key_id)?, Self::parse_secret(secret)?))
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
    pub user_id: UserId,
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

    const KEY_ID: &str = "00000000-0000-0000-0000-000000000001";
    const SECRET: &str = "cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";

    #[test]
    fn the_secret_fixture_has_the_documented_length() {
        assert_eq!(SECRET.len(), SECRET_PREFIX.len() + SECRET_BODY_LENGTH);
    }

    #[test]
    fn a_token_is_split_into_key_id_and_secret() {
        let (key_id, secret) =
            ApiKeyV2Credentials::parse_token(&format!("{KEY_ID}.{SECRET}\n")).unwrap();
        assert_eq!(key_id.to_string(), KEY_ID);
        assert_eq!(secret.expose_secret(), SECRET);
    }

    #[test]
    fn surrounding_whitespace_is_ignored_on_every_form() {
        assert!(ApiKeyV2Credentials::parse_token(&format!("  {KEY_ID}.{SECRET}\r\n")).is_ok());
        assert!(ApiKeyV2Credentials::parse_key_id(&format!(" {KEY_ID}\n")).is_ok());
        assert!(ApiKeyV2Credentials::parse_secret(&format!("\t{SECRET} ")).is_ok());
    }

    #[test]
    fn malformed_input_is_rejected_without_echoing_it() {
        let bad_secret = format!("{KEY_ID}.cad_aks_tooshort");
        let cases = [
            String::new(),
            "   \n".to_owned(),
            KEY_ID.to_owned(),
            format!("not-a-uuid.{SECRET}"),
            format!(".{SECRET}"),
            format!("{KEY_ID}."),
            bad_secret.clone(),
            format!("{KEY_ID}.wrong_prefix_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde"),
            format!("{KEY_ID}.{SECRET}x"),
            format!("{KEY_ID}.cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_Ab+/e"),
        ];
        for case in cases {
            let error = ApiKeyV2Credentials::parse_token(&case).expect_err(&case);
            assert!(matches!(error, Error::InvalidInput { .. }), "{case}");
            let message = error.to_string();
            assert!(!message.contains(SECRET), "{message}");
            assert!(!message.contains("tooshort"), "{message}");
        }
        assert!(bad_secret.contains("tooshort"));
    }

    #[test]
    fn a_key_id_and_a_secret_given_apart_are_checked_apart() {
        assert!(ApiKeyV2Credentials::parse_key_id(KEY_ID).is_ok());
        assert!(ApiKeyV2Credentials::parse_key_id("").is_err());
        assert!(ApiKeyV2Credentials::parse_key_id(SECRET).is_err());
        assert!(ApiKeyV2Credentials::parse_secret(SECRET).is_ok());
        assert!(ApiKeyV2Credentials::parse_secret("").is_err());
        assert!(ApiKeyV2Credentials::parse_secret(KEY_ID).is_err());
        // A pasted token is not a secret: the dot is not a secret character.
        assert!(ApiKeyV2Credentials::parse_secret(&format!("{KEY_ID}.{SECRET}")).is_err());
    }

    #[test]
    fn from_secret_builds_the_same_bearer_token_as_new() {
        let secret = ApiKeyV2Credentials::parse_secret(SECRET).unwrap();
        let credentials = ApiKeyV2Credentials::from_secret(Uuid::nil(), secret);
        assert_eq!(
            credentials.bearer_token(),
            ApiKeyV2Credentials::new(Uuid::nil(), SECRET).bearer_token()
        );
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
