use std::{fmt, time::Duration};

use reqwest::redirect::Policy;
use strum::{EnumString, IntoStaticStr};
use url::{Host, Url};

use crate::{Error, Result};

const PRODUCTION_URL: &str = "https://app.carbonaltdelete.eu";
const STAGING_URL: &str = "https://staging.carbonaltdelete.eu";

/// Total time allowed for one platform API call, including its response body.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_mins(1);

/// Time a presigned transfer may stall without progress.
///
/// Object-storage transfers are unbounded in total duration by design, so they
/// are protected against a silent peer rather than against being slow.
pub(crate) const STORAGE_STALL_TIMEOUT: Duration = Duration::from_mins(1);

/// Redirect policy for presigned object-storage transfers.
///
/// A presigned URL addresses one object on one host. Storage backends do
/// redirect within their own host, so redirects are not forbidden outright, but
/// following one to a different host would hand the presigned query string to
/// an unrelated origin. Cross-host hops fail loudly instead of being silently
/// stopped, because a stopped 3xx is not an error status and would otherwise
/// read as a successful upload.
pub(crate) fn storage_redirect_policy() -> Policy {
    const MAX_STORAGE_REDIRECTS: usize = 3;

    Policy::custom(|attempt| {
        let origin_host = attempt.previous().first().and_then(Url::host_str);
        if attempt.url().host_str() != origin_host {
            attempt.error("presigned transfer redirected to another host")
        } else if attempt.previous().len() > MAX_STORAGE_REDIRECTS {
            attempt.error("presigned transfer exceeded the redirect limit")
        } else {
            attempt.follow()
        }
    })
}

/// A built-in Carbon+Alt+Delete deployment.
///
/// The canonical name and its short alias are declared once and drive both
/// parsing and rendering. The parse error stays [`Error::Configuration`] rather
/// than `strum`'s own type, so the SDK's error contract is unchanged.
#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(parse_err_ty = Error, parse_err_fn = unknown_environment)]
#[non_exhaustive]
pub enum Environment {
    #[strum(serialize = "production", serialize = "prod")]
    Production,
    #[strum(serialize = "staging", serialize = "stage")]
    Staging,
}

fn unknown_environment(value: &str) -> Error {
    Error::configuration(format!(
        "unknown environment `{value}`; expected `production` or `staging`"
    ))
}

impl fmt::Display for Environment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str((*self).into())
    }
}

/// A validated same-origin base URL and the timeout policy for API requests.
#[derive(Clone, Eq, PartialEq)]
pub struct ServerConfig {
    base_url: Url,
    request_timeout: Duration,
}

impl ServerConfig {
    /// Validates a secure server URL.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Configuration`] when the URL is malformed, contains
    /// ambient credentials/query state, or is not HTTPS.
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        Self::parse(value.as_ref(), false)
    }

    /// Validates a local HTTP server URL for explicit development and tests.
    ///
    /// This constructor still rejects HTTP hosts other than loopback.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Configuration`] when the URL is malformed, contains
    /// ambient credentials/query state, or does not resolve to a loopback host.
    pub fn insecure_loopback(value: impl AsRef<str>) -> Result<Self> {
        Self::parse(value.as_ref(), true)
    }

    #[must_use]
    /// Returns the configuration for a built-in deployment.
    ///
    /// # Panics
    ///
    /// Panics only if a compile-time built-in URL is changed to an invalid URL.
    pub fn for_environment(environment: Environment) -> Self {
        let value = match environment {
            Environment::Production => PRODUCTION_URL,
            Environment::Staging => STAGING_URL,
        };

        // Built-in constants are covered by unit tests and cannot be influenced by callers.
        Self::new(value).expect("built-in server URL must remain valid")
    }

    #[must_use]
    pub fn production() -> Self {
        Self::for_environment(Environment::Production)
    }

    #[must_use]
    pub fn staging() -> Self {
        Self::for_environment(Environment::Staging)
    }

    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Replaces the total per-request timeout applied to platform API calls.
    ///
    /// The timeout covers the complete exchange, including reading the response
    /// body. It is deliberately not applied to presigned object-storage
    /// transfers, which are bounded by a stall timeout instead.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Configuration`] for a zero timeout, which would reject
    /// every request instead of disabling the limit.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(Error::configuration(
                "request timeout must be greater than zero",
            ));
        }
        self.request_timeout = timeout;
        Ok(self)
    }

    /// Returns the total per-request timeout for platform API calls.
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    pub(crate) fn endpoint(&self, path: &str) -> Result<Url> {
        if !path.starts_with("/api/") {
            return Err(Error::configuration(format!(
                "API path must start with `/api/`, got `{path}`"
            )));
        }

        self.base_url
            .join(path)
            .map_err(|error| Error::configuration(format!("invalid API path: {error}")))
    }

    fn parse(value: &str, allow_insecure_loopback: bool) -> Result<Self> {
        let mut url = Url::parse(value)
            .map_err(|error| Error::configuration(format!("invalid server URL: {error}")))?;

        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::configuration(
                "server URL must not contain credentials",
            ));
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(Error::configuration(
                "server URL must not contain a query or fragment",
            ));
        }
        if url.host().is_none() {
            return Err(Error::configuration("server URL must contain a host"));
        }

        match url.scheme() {
            "https" => {}
            "http" if allow_insecure_loopback && is_loopback(&url) => {}
            "http" if is_loopback(&url) => {
                return Err(Error::configuration(
                    "unencrypted HTTP requires an explicit insecure-loopback opt-in",
                ));
            }
            "http" => {
                return Err(Error::configuration(
                    "unencrypted HTTP is allowed only for loopback hosts",
                ));
            }
            scheme => {
                return Err(Error::configuration(format!(
                    "unsupported server URL scheme `{scheme}`"
                )));
            }
        }

        // A trailing slash gives Url::join deterministic directory semantics while API
        // endpoint paths remain absolute and therefore cannot inherit a custom prefix.
        if !url.path().ends_with('/') {
            let normalized = format!("{}/", url.path());
            url.set_path(&normalized);
        }

        Ok(Self {
            base_url: url,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("base_url", &self.base_url.as_str())
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_names_accept_their_short_aliases_and_render_canonically() {
        for (value, expected) in [
            ("production", Environment::Production),
            ("prod", Environment::Production),
            ("staging", Environment::Staging),
            ("stage", Environment::Staging),
        ] {
            assert_eq!(
                value.parse::<Environment>().expect("known environment"),
                expected
            );
        }

        assert_eq!(Environment::Production.to_string(), "production");
        assert_eq!(Environment::Staging.to_string(), "staging");
        assert!(
            "qa".parse::<Environment>()
                .expect_err("unknown environment")
                .to_string()
                .contains("unknown environment `qa`")
        );
    }

    #[test]
    fn accepts_https_and_normalizes_trailing_slash() {
        let config = ServerConfig::new("https://example.test").unwrap();
        assert_eq!(config.base_url().as_str(), "https://example.test/");
    }

    #[test]
    fn requires_explicit_opt_in_for_loopback_http() {
        assert!(ServerConfig::new("http://127.0.0.1:8080").is_err());
        assert!(ServerConfig::insecure_loopback("http://127.0.0.1:8080").is_ok());
        assert!(ServerConfig::insecure_loopback("http://example.test").is_err());
    }

    #[test]
    fn rejects_ambient_url_components() {
        assert!(ServerConfig::new("https://user@example.test").is_err());
        assert!(ServerConfig::new("https://example.test?token=secret").is_err());
        assert!(ServerConfig::new("https://example.test/#fragment").is_err());
    }

    #[test]
    fn accepts_every_loopback_form_behind_the_insecure_opt_in() {
        for value in [
            "http://localhost:8080",
            "http://LOCALHOST:8080",
            "http://127.0.0.1:8080",
            "http://127.0.0.2:8080",
            "http://[::1]:8080",
        ] {
            assert!(
                ServerConfig::insecure_loopback(value).is_ok(),
                "{value} must be accepted as loopback"
            );
        }
    }

    #[test]
    fn request_timeout_defaults_and_rejects_zero() {
        let config = ServerConfig::new("https://example.test").unwrap();
        assert_eq!(config.request_timeout(), DEFAULT_REQUEST_TIMEOUT);
        assert_eq!(
            config
                .clone()
                .with_request_timeout(Duration::from_secs(5))
                .unwrap()
                .request_timeout(),
            Duration::from_secs(5)
        );
        assert!(config.with_request_timeout(Duration::ZERO).is_err());
    }
}
