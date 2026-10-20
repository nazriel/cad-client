use serde_json::Value;
use thiserror::Error;

/// Errors returned by the SDK.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid configuration: {message}")]
    Configuration { message: String },

    #[error("authentication failed{status_suffix}: {message}", status_suffix = status.map(|value| format!(" (HTTP {value})")).unwrap_or_default())]
    Authentication {
        status: Option<u16>,
        message: String,
    },

    #[error("CAD API returned HTTP {status}: {message}")]
    Api {
        status: u16,
        message: String,
        details: Option<Value>,
    },

    #[error("transport failed: {message}")]
    Transport {
        message: String,
        /// The underlying `reqwest` failure, stripped of its URL.
        ///
        /// The wrapper message stays deliberately coarse, but discarding the
        /// cause made DNS, TLS and proxy failures indistinguishable. Keeping the
        /// stripped error preserves the chain without ever putting a presigned
        /// query string into a `Display` or `Debug` rendering.
        #[source]
        source: reqwest::Error,
    },

    #[error("invalid API response: {message}")]
    Protocol { message: String },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("I/O failed: {message}")]
    Io {
        message: String,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub(crate) fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    pub(crate) fn authentication(status: Option<u16>, message: impl Into<String>) -> Self {
        Self::Authentication {
            status,
            message: message.into(),
        }
    }

    pub(crate) fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol {
            message: message.into(),
        }
    }

    pub(crate) fn transport(error: reqwest::Error) -> Self {
        Self::Transport {
            message: transport_message(&error).to_owned(),
            // A presigned URL is a bearer credential, so it must not survive
            // into the error the caller may log.
            source: error.without_url(),
        }
    }

    pub(crate) fn from_api_response(status: u16, bytes: &[u8]) -> Self {
        const MAX_ERROR_BODY: usize = 8 * 1024;
        let bounded = &bytes[..bytes.len().min(MAX_ERROR_BODY)];
        let parsed = serde_json::from_slice::<Value>(bounded).ok();

        let message = parsed
            .as_ref()
            .and_then(extract_message)
            .or_else(|| {
                let text = String::from_utf8_lossy(bounded).trim().to_owned();
                (!text.is_empty()).then_some(text)
            })
            .unwrap_or_else(|| "request was rejected without an error body".to_owned());

        Self::Api {
            status,
            message,
            details: parsed,
        }
    }

    /// Reclassifies a rejection from an endpoint whose only job is authentication.
    ///
    /// This is applied solely to the machine identity request: every client
    /// error from it maps to an authentication failure, whatever status the API
    /// chose, so a rejected key always ends with the authentication exit code.
    pub(crate) fn classify_auth(self) -> Self {
        match self {
            Self::Api {
                status, message, ..
            } if (400..500).contains(&status) => Self::authentication(Some(status), message),
            other => other,
        }
    }
}

/// Safe, coarse classification of a `reqwest` failure.
///
/// Shared with the storage transfer errors so both report the same wording for
/// the same class of failure.
pub(crate) fn transport_message(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "request timed out"
    } else if error.is_connect() {
        "could not connect to the remote server"
    } else if error.is_body() {
        "failed while transferring the response body"
    } else if error.is_decode() {
        "failed to decode the HTTP response"
    } else if error.is_redirect() {
        "the remote server attempted a redirect"
    } else if error.is_request() {
        "could not construct or send the HTTP request"
    } else {
        "HTTP request failed"
    }
}

/// Local I/O failures reach the SDK only through `?`, so the conversion is the
/// single place that decides how they are worded.
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            message: error.to_string(),
            source: error,
        }
    }
}

fn extract_message(value: &Value) -> Option<String> {
    for key in ["msg", "message"] {
        if let Some(message) = value.get(key).and_then(Value::as_str) {
            return Some(message.to_owned());
        }
    }

    match value.get("detail") {
        Some(Value::String(message)) => Some(message.clone()),
        Some(detail @ (Value::Array(_) | Value::Object(_))) => serde_json::to_string(detail).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn every_client_error_from_an_auth_endpoint_becomes_an_authentication_failure() {
        // Whatever client-error status the API chose, the key was rejected.
        let unknown_key =
            Error::from_api_response(404, br#"{"msg":"example key rejection"}"#).classify_auth();
        assert!(matches!(unknown_key, Error::Authentication { status, .. } if status == Some(404)));
        assert!(unknown_key.to_string().contains("example key rejection"));

        let rejected = Error::from_api_response(401, b"{}").classify_auth();
        assert!(matches!(rejected, Error::Authentication { .. }));
    }

    #[tokio::test]
    async fn transport_errors_keep_their_cause_but_never_the_url() {
        // Port 1 on loopback is not listening, so this fails at connect.
        let failure = reqwest::Client::new()
            .get("http://127.0.0.1:1/object?X-Amz-Signature=secret")
            .send()
            .await
            .expect_err("a closed loopback port must refuse the connection");
        let error = Error::transport(failure);

        assert!(
            std::error::Error::source(&error).is_some(),
            "the reqwest cause must survive the wrapper"
        );
        // A connect failure still names the peer socket address, which is not a
        // secret. What must never appear is the request target: the path and the
        // signature that make a presigned URL a bearer credential.
        let rendered = format!("{error} / {error:?}");
        assert!(!rendered.contains("X-Amz-Signature"), "{rendered}");
        assert!(!rendered.contains("/object"), "{rendered}");
    }

    #[test]
    fn server_faults_stay_api_errors() {
        let outage = Error::from_api_response(503, b"{}").classify_auth();
        assert!(matches!(outage, Error::Api { status: 503, .. }));
    }
}
