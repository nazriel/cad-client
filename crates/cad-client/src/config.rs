use std::fmt;
use std::str::FromStr;

use cad_api::{
    ApiKeyV1Credentials, Environment as ServerEnvironment, ServerConfig, ids::CompanyId,
};
use uuid::Uuid;

use crate::cli::{EnvironmentName, GlobalArgs};
use crate::error::CliError;

/// Environment access seam used to make resolution deterministic in tests.
pub trait Environment {
    fn get(&self, key: &str) -> Option<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Where the application origin came from.
///
/// Only the *selection* of an origin is a CLI concern. Validating it — scheme
/// policy, loopback detection, ambient credentials — belongs to
/// [`ServerConfig`], so the two crates cannot disagree about what is legal.
enum ServerSource {
    Explicit(String),
    Preset(ServerEnvironment),
}

/// Fully validated configuration. Its `Debug` implementation never prints the
/// API secret.
pub struct AppConfig {
    server: ServerConfig,
    credentials: ApiKeyV1Credentials,
    company_id: Option<CompanyId>,
}

impl AppConfig {
    /// Resolves and validates CLI/environment configuration.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] when the server, credentials, company identifier,
    /// or transport security policy is invalid.
    pub(crate) fn resolve(
        args: &GlobalArgs,
        environment: &impl Environment,
    ) -> Result<Self, CliError> {
        let server = resolve_server(args, environment)?
            .with_request_timeout(args.request_timeout)
            .map_err(CliError::from)?;

        let api_key = environment
            .get("CAD_API_KEY")
            .ok_or_else(|| CliError::auth_config("CAD_API_KEY is not set"))?
            .parse::<Uuid>()
            .map_err(|error| {
                CliError::auth_config(format!("CAD_API_KEY is not a UUID: {error}"))
            })?;

        let api_secret = environment
            .get("CAD_API_SECRET")
            .filter(|secret| !secret.is_empty())
            .ok_or_else(|| CliError::auth_config("CAD_API_SECRET is not set or empty"))?;

        let company_id = resolve_company_id(args, environment)?;

        Ok(Self {
            server,
            credentials: ApiKeyV1Credentials::new(api_key, api_secret),
            company_id,
        })
    }

    /// Splits the configuration into the values the bootstrap actually needs.
    pub(crate) fn into_parts(self) -> (ServerConfig, ApiKeyV1Credentials, Option<CompanyId>) {
        (self.server, self.credentials, self.company_id)
    }
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppConfig")
            .field("server", &self.server)
            .field("credentials", &self.credentials)
            .field("company_id", &self.company_id)
            .finish()
    }
}

/// Applies the documented precedence: `--server-url`, `CAD_SERVER_URL`,
/// `--environment`, `CAD_ENV`.
fn resolve_server(
    args: &GlobalArgs,
    environment: &impl Environment,
) -> Result<ServerConfig, CliError> {
    let source = if let Some(raw) = args.server_url.clone() {
        ServerSource::Explicit(raw)
    } else if let Some(raw) = environment.get("CAD_SERVER_URL") {
        ServerSource::Explicit(raw)
    } else if let Some(preset) = args.environment {
        ServerSource::Preset(preset.server())
    } else if let Some(raw) = environment.get("CAD_ENV") {
        ServerSource::Preset(parse_env_preset(&raw)?)
    } else {
        return Err(CliError::config(
            "server is required; use --server-url, CAD_SERVER_URL, --environment, or CAD_ENV",
        ));
    };

    match source {
        ServerSource::Preset(preset) => Ok(ServerConfig::for_environment(preset)),
        ServerSource::Explicit(raw) if args.allow_insecure_http => {
            ServerConfig::insecure_loopback(&raw).map_err(CliError::from)
        }
        ServerSource::Explicit(raw) => ServerConfig::new(&raw).map_err(|error| {
            // Ask the SDK, rather than a second copy of the rules, whether the
            // only obstacle was the missing insecure-loopback opt-in.
            if ServerConfig::insecure_loopback(&raw).is_ok() {
                CliError::config(
                    "plain HTTP is allowed only for loopback with --allow-insecure-http",
                )
            } else {
                CliError::from(error)
            }
        }),
    }
}

fn parse_env_preset(value: &str) -> Result<ServerEnvironment, CliError> {
    ServerEnvironment::from_str(&value.to_ascii_lowercase())
        .map_err(|error| CliError::config(format!("CAD_ENV is invalid: {error}")))
}

fn resolve_company_id(
    args: &GlobalArgs,
    environment: &impl Environment,
) -> Result<Option<CompanyId>, CliError> {
    if let Some(company_id) = args.company_id {
        return Ok(Some(company_id));
    }

    environment
        .get("CAD_COMPANY_ID")
        .map(|raw| {
            raw.parse::<Uuid>()
                .map(CompanyId::from)
                .map_err(|error| CliError::config(format!("CAD_COMPANY_ID is not a UUID: {error}")))
        })
        .transpose()
}

impl EnvironmentName {
    #[must_use]
    pub const fn server(self) -> ServerEnvironment {
        match self {
            Self::Production => ServerEnvironment::Production,
            Self::Staging => ServerEnvironment::Staging,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use clap::Parser;

    use super::*;
    use crate::cli::Cli;

    #[derive(Default)]
    struct Env(HashMap<String, String>);

    impl Environment for Env {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn credentials() -> Env {
        Env(HashMap::from([
            (
                "CAD_API_KEY".into(),
                "00000000-0000-0000-0000-000000000001".into(),
            ),
            ("CAD_API_SECRET".into(), "secret".into()),
        ]))
    }

    fn resolve(arguments: &[&str]) -> Result<AppConfig, CliError> {
        AppConfig::resolve(
            &Cli::try_parse_from(arguments).unwrap().global,
            &credentials(),
        )
    }

    fn base_url(config: &AppConfig) -> String {
        config.server.base_url().as_str().to_owned()
    }

    #[test]
    fn explicit_url_beats_environment_presets() {
        let config = resolve(&[
            "cad-cli",
            "--server-url",
            "https://example.test",
            "--environment",
            "staging",
            "company",
            "list",
        ])
        .unwrap();
        assert_eq!(base_url(&config), "https://example.test/");
    }

    #[test]
    fn environment_url_beats_cli_preset() {
        let cli = Cli::try_parse_from(["cad-cli", "--environment", "staging", "company", "list"])
            .unwrap();
        let mut env = credentials();
        env.0.insert(
            "CAD_SERVER_URL".into(),
            "https://custom.example.test".into(),
        );
        let config = AppConfig::resolve(&cli.global, &env).unwrap();
        assert_eq!(base_url(&config), "https://custom.example.test/");
    }

    #[test]
    fn insecure_non_loopback_is_rejected() {
        assert!(
            resolve(&[
                "cad-cli",
                "--server-url",
                "http://example.test",
                "--allow-insecure-http",
                "company",
                "list",
            ])
            .is_err()
        );
    }

    #[test]
    fn every_loopback_form_is_accepted_with_the_insecure_flag() {
        for value in [
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
            "http://localhost:8080",
        ] {
            assert!(
                resolve(&[
                    "cad-cli",
                    "--server-url",
                    value,
                    "--allow-insecure-http",
                    "company",
                    "list",
                ])
                .is_ok(),
                "{value} must be accepted as loopback"
            );
        }
    }

    #[test]
    fn http_loopback_without_the_flag_names_the_flag() {
        let error = resolve(&[
            "cad-cli",
            "--server-url",
            "http://127.0.0.1:8080",
            "company",
            "list",
        ])
        .unwrap_err();
        assert!(error.to_string().contains("--allow-insecure-http"));
    }

    #[test]
    fn request_timeout_reaches_the_server_config() {
        let config = resolve(&[
            "cad-cli",
            "--environment",
            "staging",
            "--request-timeout",
            "5s",
            "company",
            "list",
        ])
        .unwrap();
        assert_eq!(
            config.server.request_timeout(),
            std::time::Duration::from_secs(5)
        );
    }

    #[test]
    fn invalid_company_id_is_reported_instead_of_ignored() {
        let cli = Cli::try_parse_from(["cad-cli", "--environment", "staging", "company", "list"])
            .unwrap();
        let mut env = credentials();
        env.0.insert("CAD_COMPANY_ID".into(), "not-a-uuid".into());
        let error = AppConfig::resolve(&cli.global, &env).unwrap_err();
        assert!(error.to_string().contains("CAD_COMPANY_ID"));
    }

    #[test]
    fn debug_redacts_secret() {
        let config = resolve(&["cad-cli", "--environment", "staging", "company", "list"]).unwrap();
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("\"secret\""));
    }

    #[test]
    fn invalid_cad_env_is_reported_instead_of_ignored() {
        let cli = Cli::try_parse_from(["cad-cli", "company", "list"]).unwrap();
        let mut env = credentials();
        env.0.insert("CAD_ENV".into(), "qa".into());
        let error = AppConfig::resolve(&cli.global, &env).unwrap_err();
        assert!(error.to_string().contains("CAD_ENV"));
    }

    #[test]
    fn cad_env_preset_is_case_insensitive() {
        let cli = Cli::try_parse_from(["cad-cli", "company", "list"]).unwrap();
        let mut env = credentials();
        env.0.insert("CAD_ENV".into(), "Staging".into());
        let config = AppConfig::resolve(&cli.global, &env).unwrap();
        assert_eq!(base_url(&config), "https://staging.carbonaltdelete.eu/");
    }
}
