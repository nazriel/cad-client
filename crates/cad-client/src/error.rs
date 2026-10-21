use std::process::ExitCode;

use std::borrow::Cow;

use serde_json::Value;
use strum::EnumDiscriminants;
use thiserror::Error;

/// Failure surfaced by the CLI.
///
/// The enum is deliberately typed rather than an `anyhow`-style opaque error:
/// it is the carrier of the documented exit-code contract, and every variant
/// maps to exactly one code in [`CliError::exit_code`].
///
/// Variants that can originate in the SDK keep the [`cad_api::Error`] they were
/// converted from. The rendered message is unchanged, but `details` from a
/// rejected API call and the whole source chain below it stay reachable through
/// [`CliError::api_error`] instead of being flattened away at the boundary.
///
/// `Config` and `AuthConfig` can carry a `hint`: how to fix the failure, kept
/// apart from what went wrong so a program can show or act on either. The text
/// rendering joins them as `<message>; <hint>`.
#[derive(Debug, Error, EnumDiscriminants)]
#[strum_discriminants(name(ErrorKind), derive(serde::Serialize))]
#[strum_discriminants(serde(rename_all = "camelCase"))]
#[strum_discriminants(doc = "What kind of failure a [`CliError`] is, as named in its JSON form.")]
pub enum CliError {
    #[error("configuration error: {}", Hinted(message, hint))]
    Config {
        message: String,
        hint: Option<String>,
        #[source]
        source: Option<cad_api::Error>,
    },
    #[error("authentication configuration error: {}", Hinted(message, hint))]
    AuthConfig {
        message: String,
        hint: Option<String>,
    },
    #[error("authentication failed: {message}")]
    Authentication {
        message: String,
        #[source]
        source: Option<cad_api::Error>,
    },
    #[error("invalid input: {0}")]
    Input(String),
    #[error("API operation failed: {message}")]
    Api {
        message: String,
        #[source]
        source: Option<cad_api::Error>,
    },
    #[error("transport failed: {message}")]
    Transport {
        message: String,
        #[source]
        source: Option<cad_api::Error>,
    },
    #[error("I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("operation timed out: {0}")]
    Timeout(String),
    #[error("operation cancelled")]
    Cancelled,
    /// A release could not be installed: its metadata, checksum list or
    /// archive was not what `cad-cli update` requires.
    #[error("update failed: {0}")]
    Update(String),
}

/// A message with the hint that says how to fix it, as one line of text.
struct Hinted<'a>(&'a str, &'a Option<String>);

impl std::fmt::Display for Hinted<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.1 {
            Some(hint) => write!(f, "{}; {hint}", self.0),
            None => f.write_str(self.0),
        }
    }
}

impl CliError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            hint: None,
            source: None,
        }
    }

    /// A configuration failure that says how to fix it.
    pub fn config_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            hint: Some(hint.into()),
            source: None,
        }
    }

    pub fn auth_config(message: impl Into<String>) -> Self {
        Self::AuthConfig {
            message: message.into(),
            hint: None,
        }
    }

    /// A credentials failure that says how to fix it.
    pub fn auth_config_with_hint(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::AuthConfig {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    #[must_use]
    pub fn kind(&self) -> ErrorKind {
        ErrorKind::from(self)
    }

    /// What went wrong, without the kind's prefix and without the hint.
    #[must_use]
    pub fn message(&self) -> Cow<'_, str> {
        match self {
            Self::Config { message, .. }
            | Self::AuthConfig { message, .. }
            | Self::Authentication { message, .. }
            | Self::Api { message, .. }
            | Self::Transport { message, .. } => Cow::Borrowed(message),
            Self::Input(message) | Self::Timeout(message) | Self::Update(message) => {
                Cow::Borrowed(message)
            }
            Self::Io(error) => Cow::Owned(error.to_string()),
            Self::Cancelled => Cow::Borrowed("operation cancelled"),
        }
    }

    /// How to fix the failure, when the error says so.
    #[must_use]
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Config { hint, .. } | Self::AuthConfig { hint, .. } => hint.as_deref(),
            _ => None,
        }
    }

    /// The HTTP status the platform answered with, when the failure carries one.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self.api_error() {
            Some(cad_api::Error::Api { status, .. }) => Some(*status),
            Some(cad_api::Error::Authentication { status, .. }) => *status,
            _ => None,
        }
    }

    /// Reports a failure the CLI itself diagnosed from an API result, such as a
    /// remote job that finished in a failed state.
    pub fn api(message: impl Into<String>) -> Self {
        Self::Api {
            message: message.into(),
            source: None,
        }
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self::Transport {
            message: message.into(),
            source: None,
        }
    }

    /// Returns the SDK error this failure was converted from, if any.
    #[must_use]
    pub fn api_error(&self) -> Option<&cad_api::Error> {
        match self {
            Self::Config { source, .. }
            | Self::Authentication { source, .. }
            | Self::Api { source, .. }
            | Self::Transport { source, .. } => source.as_ref(),
            Self::AuthConfig { .. }
            | Self::Input(_)
            | Self::Io(_)
            | Self::Timeout(_)
            | Self::Cancelled
            | Self::Update(_) => None,
        }
    }

    /// Returns the structured error body the platform sent, if the failure came
    /// from a rejected API call that carried one.
    #[must_use]
    pub fn details(&self) -> Option<&Value> {
        match self.api_error() {
            Some(cad_api::Error::Api { details, .. }) => details.as_ref(),
            _ => None,
        }
    }

    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        ExitCode::from(self.exit_status())
    }

    /// The number [`CliError::exit_code`] makes the process exit with.
    #[must_use]
    pub fn exit_status(&self) -> u8 {
        match self {
            Self::Config { .. } | Self::AuthConfig { .. } | Self::Input(_) => 2,
            Self::Authentication { .. } => 3,
            // A release that cannot be installed is a remote service
            // returning something unusable, like a failed API operation.
            Self::Api { .. } | Self::Timeout(_) | Self::Update(_) => 4,
            Self::Transport { .. } | Self::Io(_) => 5,
            Self::Cancelled => 130,
        }
    }
}

impl From<cad_api::Error> for CliError {
    fn from(error: cad_api::Error) -> Self {
        // The wording of every message is what it was before sources were kept;
        // only the discarded cause is new.
        match &error {
            cad_api::Error::Configuration { message }
            | cad_api::Error::InvalidInput { message } => {
                let message = message.clone();
                Self::Config {
                    message,
                    hint: None,
                    source: Some(error),
                }
            }
            cad_api::Error::Authentication { message, .. } => {
                let message = message.clone();
                Self::Authentication {
                    message,
                    source: Some(error),
                }
            }
            cad_api::Error::Api {
                status, message, ..
            } => {
                let message = format!("HTTP {status}: {message}");
                Self::Api {
                    message,
                    source: Some(error),
                }
            }
            cad_api::Error::Transport { message, .. } | cad_api::Error::Io { message, .. } => {
                let message = message.clone();
                Self::Transport {
                    message,
                    source: Some(error),
                }
            }
            cad_api::Error::Protocol { message } => {
                let message = format!("invalid server response: {message}");
                Self::Api {
                    message,
                    source: Some(error),
                }
            }
            _ => {
                let message = error.to_string();
                Self::Api {
                    message,
                    source: Some(error),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CliError, ErrorKind};

    #[test]
    fn api_failures_keep_their_details_and_source() {
        let error = CliError::from(cad_api::Error::Api {
            status: 422,
            message: "validation failed".to_owned(),
            details: Some(serde_json::json!({"msg": "validation failed"})),
        });

        assert_eq!(
            error.to_string(),
            "API operation failed: HTTP 422: validation failed"
        );
        assert!(matches!(
            error.api_error(),
            Some(cad_api::Error::Api { status: 422, .. })
        ));
        assert_eq!(
            error.details().and_then(|details| details.get("msg")),
            Some(&serde_json::json!("validation failed"))
        );
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn locally_diagnosed_failures_have_no_sdk_source() {
        let error = CliError::api("report 7 finished with status FAILED");
        assert!(error.api_error().is_none());
        assert!(error.details().is_none());
        assert!(std::error::Error::source(&error).is_none());
    }

    #[test]
    fn a_hint_is_joined_to_its_message_only_in_the_text() {
        let error = CliError::config_with_hint("no profile is selected", "pass --profile");
        assert_eq!(
            error.to_string(),
            "configuration error: no profile is selected; pass --profile"
        );
        assert_eq!(error.message(), "no profile is selected");
        assert_eq!(error.hint(), Some("pass --profile"));

        let error = CliError::auth_config_with_hint("no credentials", "log in");
        assert_eq!(
            error.to_string(),
            "authentication configuration error: no credentials; log in"
        );
        assert_eq!(error.message(), "no credentials");
        assert_eq!(error.hint(), Some("log in"));

        let error = CliError::auth_config("CAD_API_KEY is not set");
        assert_eq!(
            error.to_string(),
            "authentication configuration error: CAD_API_KEY is not set"
        );
        assert_eq!(error.hint(), None);
    }

    #[test]
    fn every_variant_has_its_kind_message_and_exit_status() {
        let api = || {
            Some(cad_api::Error::Api {
                status: 500,
                message: "boom".to_owned(),
                details: None,
            })
        };
        let cases = [
            (CliError::config("a"), ErrorKind::Config, "a", 2),
            (CliError::auth_config("a"), ErrorKind::AuthConfig, "a", 2),
            (
                CliError::Authentication {
                    message: "a".to_owned(),
                    source: None,
                },
                ErrorKind::Authentication,
                "a",
                3,
            ),
            (CliError::Input("a".to_owned()), ErrorKind::Input, "a", 2),
            (
                CliError::Api {
                    message: "a".to_owned(),
                    source: api(),
                },
                ErrorKind::Api,
                "a",
                4,
            ),
            (CliError::transport("a"), ErrorKind::Transport, "a", 5),
            (
                CliError::Io(std::io::Error::other("a")),
                ErrorKind::Io,
                "a",
                5,
            ),
            (
                CliError::Timeout("a".to_owned()),
                ErrorKind::Timeout,
                "a",
                4,
            ),
            (
                CliError::Cancelled,
                ErrorKind::Cancelled,
                "operation cancelled",
                130,
            ),
            (CliError::Update("a".to_owned()), ErrorKind::Update, "a", 4),
        ];
        for (error, kind, message, status) in cases {
            assert_eq!(error.kind(), kind, "{error}");
            assert_eq!(error.message(), message, "{error}");
            assert_eq!(error.exit_status(), status, "{error}");
        }
    }

    #[test]
    fn the_kinds_are_named_in_camel_case() {
        let names: Vec<_> = [
            ErrorKind::Config,
            ErrorKind::AuthConfig,
            ErrorKind::Authentication,
            ErrorKind::Input,
            ErrorKind::Api,
            ErrorKind::Transport,
            ErrorKind::Io,
            ErrorKind::Timeout,
            ErrorKind::Cancelled,
            ErrorKind::Update,
        ]
        .into_iter()
        .map(|kind| serde_json::to_value(kind).expect("a kind serializes"))
        .collect();
        assert_eq!(
            names,
            [
                "config",
                "authConfig",
                "authentication",
                "input",
                "api",
                "transport",
                "io",
                "timeout",
                "cancelled",
                "update"
            ]
            .map(serde_json::Value::from)
        );
    }

    #[test]
    fn the_status_comes_from_the_sdk_error() {
        assert_eq!(
            CliError::from(cad_api::Error::Api {
                status: 403,
                message: "no".to_owned(),
                details: None,
            })
            .status(),
            Some(403)
        );
        assert_eq!(
            CliError::from(cad_api::Error::Authentication {
                status: Some(401),
                message: "no".to_owned(),
            })
            .status(),
            Some(401)
        );
        assert_eq!(CliError::config("a").status(), None);
    }
}
