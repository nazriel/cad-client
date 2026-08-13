use std::process::ExitCode;

use serde_json::Value;
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
#[derive(Debug, Error)]
pub enum CliError {
    #[error("configuration error: {message}")]
    Config {
        message: String,
        #[source]
        source: Option<cad_api::Error>,
    },
    #[error("authentication configuration error: {0}")]
    AuthConfig(String),
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
}

impl CliError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
            source: None,
        }
    }

    pub fn auth_config(message: impl Into<String>) -> Self {
        Self::AuthConfig(message.into())
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
            Self::AuthConfig(_)
            | Self::Input(_)
            | Self::Io(_)
            | Self::Timeout(_)
            | Self::Cancelled => None,
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
        match self {
            Self::Config { .. } | Self::AuthConfig(_) | Self::Input(_) => ExitCode::from(2),
            Self::Authentication { .. } => ExitCode::from(3),
            Self::Api { .. } | Self::Timeout(_) => ExitCode::from(4),
            Self::Transport { .. } | Self::Io(_) => ExitCode::from(5),
            Self::Cancelled => ExitCode::from(130),
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
    use super::CliError;

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
}
