use std::fmt;

use cad_api::{ApiKeyV2Credentials, ServerConfig, ids::CompanyId};
use serde::Serialize;
use uuid::Uuid;

use crate::cli::GlobalArgs;
use crate::error::CliError;
use crate::support::credentials::{self, CredentialStore, Resolved, Source as CredentialSource};
use crate::support::profiles::{ProfileName, ProfileSet};

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

/// Why a profile was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProfileSource {
    /// `--profile`.
    Flag,
    /// `CAD_PROFILE`.
    Variable,
    /// `--server-url`, which picked the profile whose server it is.
    ServerFlag,
    /// `CAD_SERVER_URL`, likewise.
    ServerVariable,
    /// Nothing was asked for: the active profile.
    Active,
}

impl ProfileSource {
    /// How the selection is explained to a person.
    pub(crate) const fn describe(self) -> &'static str {
        match self {
            Self::Flag => "selected by --profile",
            Self::Variable => "selected by CAD_PROFILE",
            Self::ServerFlag => "selected by --server-url",
            Self::ServerVariable => "selected by CAD_SERVER_URL",
            Self::Active => "the active profile",
        }
    }
}

/// The profile a command uses, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedProfile {
    pub(crate) name: ProfileName,
    pub(crate) source: ProfileSource,
}

/// The server a command talks to and the profile it acts for, if any.
#[derive(Debug)]
pub(crate) struct Selection {
    pub(crate) server: ServerConfig,
    pub(crate) profile: Option<SelectedProfile>,
}

impl Selection {
    pub(crate) fn profile_name(&self) -> Option<&ProfileName> {
        self.profile.as_ref().map(|selected| &selected.name)
    }
}

/// Why a company was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CompanySource {
    /// `--company-id`.
    Flag,
    /// `CAD_COMPANY_ID`.
    Environment,
    /// The company of the profile in use (`cad-cli use company`).
    Profile,
}

/// The company a command acts on, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectedCompany {
    pub(crate) id: CompanyId,
    /// The label the profile recorded for this company, if it is the
    /// profile's own; a company given by flag or variable is known by ID only.
    pub(crate) name: Option<String>,
    pub(crate) source: CompanySource,
}

/// Fully validated configuration. Its `Debug` implementation never prints the
/// API secret.
pub struct AppConfig {
    server: ServerConfig,
    credentials: ApiKeyV2Credentials,
    source: CredentialSource,
    profile: Option<ProfileName>,
    company: Option<SelectedCompany>,
}

/// What [`AppConfig`] is made of, for the bootstrap that connects with it.
pub(crate) struct Parts {
    pub(crate) server: ServerConfig,
    pub(crate) credentials: ApiKeyV2Credentials,
    pub(crate) profile: Option<ProfileName>,
    pub(crate) company: Option<SelectedCompany>,
}

impl AppConfig {
    /// Resolves and validates CLI/environment configuration.
    ///
    /// The server comes from the selection (see [`select`]), the credentials
    /// from the environment or else from what `cad-cli use profile` stored
    /// for the selected profile (see [`credentials::resolve`]), and the
    /// company from `--company-id`, `CAD_COMPANY_ID` or the profile's company
    /// (see [`select_company`]). A missing company is not an error here: only
    /// a command that acts on a company needs one (see
    /// [`Self::require_company`]).
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] when the server, credentials, company identifier,
    /// or transport security policy is invalid.
    pub(crate) fn resolve(
        args: &GlobalArgs,
        environment: &impl Environment,
        store: &CredentialStore,
        profiles: &ProfileSet,
    ) -> Result<Self, CliError> {
        let selection = select(args, environment, profiles)?;
        let resolved = resolve_credentials(environment, store, &selection)?;
        let company = select_company(args, environment, profiles, &selection)?;
        Self::resolve_with(selection, resolved, company)
    }

    /// Like [`Self::resolve`], for a caller that has already made the
    /// selection and found the credentials and the company, and must not read
    /// the store a second time.
    pub(crate) fn resolve_with(
        selection: Selection,
        resolved: Resolved,
        company: Option<SelectedCompany>,
    ) -> Result<Self, CliError> {
        let source = resolved.source();
        let credentials = resolved.into_credentials()?;
        Ok(Self {
            server: selection.server,
            credentials,
            source,
            profile: selection.profile.map(|selected| selected.name),
            company,
        })
    }

    pub(crate) const fn server(&self) -> &ServerConfig {
        &self.server
    }

    pub(crate) fn profile(&self) -> Option<&ProfileName> {
        self.profile.as_ref()
    }

    pub(crate) const fn company(&self) -> Option<&SelectedCompany> {
        self.company.as_ref()
    }

    /// Where the credentials come from, and their Key ID.
    pub(crate) fn credentials_in_use(&self) -> (CredentialSource, Uuid) {
        (self.source, self.credentials.api_key())
    }

    /// Refuses a configuration with no company, for a command that acts on
    /// one. The platform's own choice of company for the key is never used.
    ///
    /// # Errors
    ///
    /// Returns [`CliError::Config`] naming how to choose a company.
    pub(crate) fn require_company(&self) -> Result<(), CliError> {
        if self.company.is_some() {
            Ok(())
        } else {
            Err(missing_company(self.profile.as_ref()))
        }
    }

    /// Splits the configuration into the values the bootstrap actually needs.
    pub(crate) fn into_parts(self) -> Parts {
        Parts {
            server: self.server,
            credentials: self.credentials,
            profile: self.profile,
            company: self.company,
        }
    }
}

impl fmt::Debug for AppConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AppConfig")
            .field("server", &self.server)
            .field("credentials", &self.credentials)
            .field("source", &self.source)
            .field("profile", &self.profile)
            .field("company", &self.company)
            .finish()
    }
}

/// The error for a command that acts on a company when none is selected.
pub(crate) fn missing_company(profile: Option<&ProfileName>) -> CliError {
    match profile {
        Some(profile) => CliError::config_with_hint(
            format!("no company is selected for profile {profile}"),
            "run `cad-cli use company <name|id>`, or pass --company-id or set CAD_COMPANY_ID",
        ),
        None => CliError::config_with_hint(
            "no company is selected",
            "pass --company-id or set CAD_COMPANY_ID",
        ),
    }
}

/// Finds the credentials for what [`select`] chose.
///
/// # Errors
///
/// See [`credentials::resolve`].
pub(crate) fn resolve_credentials(
    environment: &impl Environment,
    store: &CredentialStore,
    selection: &Selection,
) -> Result<Resolved, CliError> {
    credentials::resolve(
        environment,
        store,
        selection.profile_name(),
        &credentials::origin_of(&selection.server),
    )
}

/// The profile named by `--profile` or, failing that, `CAD_PROFILE`.
fn explicit_profile(
    args: &GlobalArgs,
    environment: &impl Environment,
) -> Result<Option<(ProfileName, ProfileSource)>, CliError> {
    if let Some(name) = &args.profile {
        return Ok(Some((name.clone(), ProfileSource::Flag)));
    }
    environment
        .get("CAD_PROFILE")
        .map(|raw| {
            raw.parse::<ProfileName>()
                .map(|name| (name, ProfileSource::Variable))
                .map_err(|reason| CliError::config(format!("CAD_PROFILE is invalid: {reason}")))
        })
        .transpose()
}

/// The server asked for outright: `--server-url`, else `CAD_SERVER_URL`.
fn server_override(
    args: &GlobalArgs,
    environment: &impl Environment,
) -> Option<(String, ProfileSource)> {
    args.server_url
        .clone()
        .map(|raw| (raw, ProfileSource::ServerFlag))
        .or_else(|| {
            environment
                .get("CAD_SERVER_URL")
                .map(|raw| (raw, ProfileSource::ServerVariable))
        })
}

/// The origin of a server URL as given, before the scheme policy is applied:
/// that policy is [`build_server`]'s to report, with its own wording.
pub(crate) fn origin_of_raw(raw: &str) -> Result<String, CliError> {
    ServerConfig::new(raw)
        .or_else(|error| ServerConfig::insecure_loopback(raw).map_err(|_| error))
        .map(|server| credentials::origin_of(&server))
        .map_err(CliError::from)
}

/// Picks the profile a command uses.
///
/// 1. `--profile`, then `CAD_PROFILE`; it must exist. With a server given as
///    well, that server must be the profile's own.
/// 2. Otherwise, with a server given (`--server-url`, `CAD_SERVER_URL`), the
///    profile whose server it is: the active one if it matches, else the only
///    one that does. Several are an error; none means no profile, so no
///    stored login.
/// 3. Otherwise the active profile, if there is one.
///
/// # Errors
///
/// Returns [`CliError`] for a profile that does not exist, a server that is
/// not the named profile's, a server several profiles share, or a server URL
/// that cannot be read.
pub(crate) fn select_profile(
    args: &GlobalArgs,
    environment: &impl Environment,
    profiles: &ProfileSet,
) -> Result<Option<SelectedProfile>, CliError> {
    let server = server_override(args, environment)
        .map(|(raw, source)| origin_of_raw(&raw).map(|origin| (origin, source)))
        .transpose()?;

    if let Some((name, source)) = explicit_profile(args, environment)? {
        let profile = profiles.get(&name).ok_or_else(|| missing_profile(&name))?;
        if let Some((origin, _)) = &server
            && *origin != profile.url
        {
            return Err(CliError::config(format!(
                "profile {name} logs in to {}, not {origin}",
                profile.url
            )));
        }
        return Ok(Some(SelectedProfile { name, source }));
    }

    let Some((origin, source)) = server else {
        return Ok(profiles.active().map(|name| SelectedProfile {
            name: name.clone(),
            source: ProfileSource::Active,
        }));
    };
    if let Some(active) = profiles.active()
        && profiles
            .get(active)
            .is_some_and(|profile| profile.url == origin)
    {
        return Ok(Some(SelectedProfile {
            name: active.clone(),
            source,
        }));
    }
    let mut matching = profiles.with_origin(&origin).map(|(name, _)| name);
    match (matching.next(), matching.next()) {
        (None, _) => Ok(None),
        (Some(name), None) => Ok(Some(SelectedProfile {
            name: name.clone(),
            source,
        })),
        (Some(first), Some(second)) => {
            let mut names = vec![first.to_string(), second.to_string()];
            names.extend(matching.map(ToString::to_string));
            Err(CliError::config_with_hint(
                format!("several profiles use {origin} ({})", names.join(", ")),
                "pass --profile",
            ))
        }
    }
}

fn missing_profile(name: &ProfileName) -> CliError {
    CliError::config_with_hint(
        format!("there is no profile {name}"),
        format!("create it with `cad-cli use profile {name}`"),
    )
}

/// Applies the documented precedence for the server and the profile: see
/// [`select_profile`]; the server is then `--server-url`, `CAD_SERVER_URL`, or
/// the profile's own.
///
/// # Errors
///
/// Returns [`CliError`] for the failures of [`select_profile`], when there is
/// neither a profile nor a server, and when the server is not acceptable.
pub(crate) fn select(
    args: &GlobalArgs,
    environment: &impl Environment,
    profiles: &ProfileSet,
) -> Result<Selection, CliError> {
    let profile = select_profile(args, environment, profiles)?;
    let in_use = profile
        .as_ref()
        .and_then(|selected| profiles.get(&selected.name));
    let allow_http =
        args.allow_insecure_http || in_use.is_some_and(|profile| profile.allow_insecure_http);
    let server = match (server_override(args, environment), in_use) {
        // A server that selected a profile, or that the named profile agreed
        // to, is the profile's own, so its opt-in applies.
        (Some((raw, _)), _) => build_server(&raw, allow_http)?,
        (None, Some(profile)) => build_server(&profile.url, allow_http)?,
        (None, None) => return Err(no_profile()),
    };
    Ok(Selection {
        server: server
            .with_request_timeout(args.request_timeout)
            .map_err(CliError::from)?,
        profile,
    })
}

fn no_profile() -> CliError {
    CliError::config_with_hint(
        "no profile is selected",
        "run `cad-cli use profile <name>` (production, staging, dev, or a name of your own), \
         pass --profile, or set CAD_SERVER_URL with CAD_API_KEY and CAD_API_SECRET",
    )
}

/// Validates a server URL, allowing plain HTTP for loopback only when
/// `allow_http` says so.
pub(crate) fn build_server(raw: &str, allow_http: bool) -> Result<ServerConfig, CliError> {
    if allow_http {
        return ServerConfig::insecure_loopback(raw).map_err(CliError::from);
    }
    ServerConfig::new(raw).map_err(|error| {
        // Ask the SDK, rather than a second copy of the rules, whether the
        // only obstacle was the missing insecure-loopback opt-in.
        if ServerConfig::insecure_loopback(raw).is_ok() {
            CliError::config("plain HTTP is allowed only for loopback with --allow-insecure-http")
        } else {
            CliError::from(error)
        }
    })
}

/// The company a command acts on: `--company-id`, else `CAD_COMPANY_ID`,
/// else the company of the profile in use. There is no other source; in
/// particular, the company the platform would pick for the key is never used.
///
/// The profile in use is the one [`select`] chose, so it is never a profile
/// whose server is not the one talked to.
///
/// # Errors
///
/// Returns [`CliError`] when `CAD_COMPANY_ID` is not a UUID.
pub(crate) fn select_company(
    args: &GlobalArgs,
    environment: &impl Environment,
    profiles: &ProfileSet,
    selection: &Selection,
) -> Result<Option<SelectedCompany>, CliError> {
    let of_profile = selection
        .profile_name()
        .and_then(|name| profiles.get(name))
        .and_then(|profile| profile.company.as_ref());
    let (id, source) = if let Some(id) = args.company_id {
        (id, CompanySource::Flag)
    } else if let Some(raw) = environment.get("CAD_COMPANY_ID") {
        let id = raw
            .parse::<Uuid>()
            .map(CompanyId::from)
            .map_err(|error| CliError::config(format!("CAD_COMPANY_ID is not a UUID: {error}")))?;
        (id, CompanySource::Environment)
    } else if let Some(company) = of_profile {
        return Ok(Some(SelectedCompany {
            id: company.id,
            name: company.name.clone(),
            source: CompanySource::Profile,
        }));
    } else {
        return Ok(None);
    };
    let name = of_profile
        .filter(|company| company.id == id)
        .and_then(|company| company.name.clone());
    Ok(Some(SelectedCompany { id, name, source }))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use clap::Parser;
    use secrecy::SecretString;

    use super::*;
    use crate::cli::Cli;
    use crate::support::credentials::StoredCredential;
    use crate::support::credentials::memory::empty_store;
    use crate::support::profiles::Profile;

    const STAGING: &str = "https://staging.carbonaltdelete.eu";
    const PRODUCTION: &str = "https://app.carbonaltdelete.eu";
    const LOCAL: &str = "http://127.0.0.1:5001";

    #[derive(Default)]
    struct Env(HashMap<String, String>);

    impl Env {
        fn with(mut self, key: &str, value: &str) -> Self {
            self.0.insert(key.into(), value.into());
            self
        }
    }

    impl Environment for Env {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn credentials() -> Env {
        Env::default()
            .with("CAD_API_KEY", "00000000-0000-0000-0000-000000000001")
            .with("CAD_API_SECRET", "secret")
    }

    fn name(value: &str) -> ProfileName {
        value.parse().unwrap()
    }

    /// `staging` (active) and `production`, plus whatever `more` adds.
    fn profiles(more: &[(&str, &str)]) -> ProfileSet {
        let mut set = ProfileSet::default();
        for (profile, url) in [("staging", STAGING), ("production", PRODUCTION)]
            .into_iter()
            .chain(more.iter().copied())
        {
            set.insert(
                name(profile),
                Profile {
                    url: url.to_owned(),
                    allow_insecure_http: url.starts_with("http:"),
                    login: None,
                    company: None,
                },
            );
        }
        set.activate(name("staging"));
        set
    }

    fn parse(arguments: &[&str]) -> Cli {
        let mut line = vec!["cad-cli"];
        line.extend_from_slice(arguments);
        line.extend(["company", "list"]);
        Cli::try_parse_from(line).unwrap()
    }

    fn pick(
        arguments: &[&str],
        environment: &Env,
        set: &ProfileSet,
    ) -> Result<Selection, CliError> {
        select(&parse(arguments).global, environment, set)
    }

    fn origin(selection: &Selection) -> String {
        selection.server.base_url().origin().ascii_serialization()
    }

    fn chosen(selection: &Selection) -> Option<(&str, ProfileSource)> {
        selection
            .profile
            .as_ref()
            .map(|selected| (selected.name.as_str(), selected.source))
    }

    fn resolve(arguments: &[&str]) -> Result<AppConfig, CliError> {
        AppConfig::resolve(
            &parse(arguments).global,
            &credentials(),
            &empty_store(),
            &profiles(&[]),
        )
    }

    fn base_url(config: &AppConfig) -> String {
        config.server.base_url().as_str().to_owned()
    }

    // --- the profile ---------------------------------------------------------

    #[test]
    fn the_profile_is_the_flag_then_the_variable_then_a_server_then_the_active_one() {
        let set = profiles(&[]);
        let variable = Env::default().with("CAD_PROFILE", "production");

        let active = pick(&[], &Env::default(), &set).unwrap();
        assert_eq!(chosen(&active), Some(("staging", ProfileSource::Active)));
        assert_eq!(origin(&active), STAGING);

        let by_variable = pick(&[], &variable, &set).unwrap();
        assert_eq!(
            chosen(&by_variable),
            Some(("production", ProfileSource::Variable))
        );
        assert_eq!(origin(&by_variable), PRODUCTION);

        let by_flag = pick(&["--profile", "staging"], &variable, &set).unwrap();
        assert_eq!(chosen(&by_flag), Some(("staging", ProfileSource::Flag)));
        assert_eq!(origin(&by_flag), STAGING);

        let by_server = pick(&["--server-url", PRODUCTION], &Env::default(), &set).unwrap();
        assert_eq!(
            chosen(&by_server),
            Some(("production", ProfileSource::ServerFlag))
        );
        // ...but a named profile is stronger than a server that would pick one.
        let both = pick(&["--server-url", PRODUCTION], &variable, &set).unwrap();
        assert_eq!(chosen(&both), Some(("production", ProfileSource::Variable)));
    }

    #[test]
    fn the_server_is_the_flag_then_the_variable_then_the_profiles() {
        let set = profiles(&[]);
        let variable = Env::default().with("CAD_SERVER_URL", "https://custom.example.test");

        assert_eq!(origin(&pick(&[], &Env::default(), &set).unwrap()), STAGING);

        let selection = pick(&[], &variable, &set).unwrap();
        assert_eq!(origin(&selection), "https://custom.example.test");
        assert_eq!(chosen(&selection), None);

        let selection = pick(&["--server-url", PRODUCTION], &variable, &set).unwrap();
        assert_eq!(origin(&selection), PRODUCTION);
        assert_eq!(
            chosen(&selection),
            Some(("production", ProfileSource::ServerFlag))
        );
    }

    #[test]
    fn a_profile_that_does_not_exist_fails_naming_the_command_that_creates_it() {
        let set = profiles(&[]);
        let expected = "configuration error: there is no profile nope; create it with `cad-cli use profile nope`";
        let error = pick(&["--profile", "nope"], &Env::default(), &set).unwrap_err();
        assert_eq!(error.to_string(), expected);
        let error = pick(&[], &Env::default().with("CAD_PROFILE", "nope"), &set).unwrap_err();
        assert_eq!(error.to_string(), expected);
        // Even with a server that another profile has.
        let error = pick(
            &["--profile", "nope", "--server-url", PRODUCTION],
            &Env::default(),
            &set,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), expected);
    }

    #[test]
    fn an_invalid_cad_profile_is_reported_instead_of_ignored() {
        let error = pick(
            &[],
            &Env::default().with("CAD_PROFILE", "Not Valid"),
            &profiles(&[]),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("CAD_PROFILE is invalid"), "{message}");
        assert!(message.contains("1 to 32 characters"), "{message}");
    }

    #[test]
    fn an_explicit_profile_with_a_foreign_server_fails() {
        let set = profiles(&[]);
        let expected =
            format!("configuration error: profile staging logs in to {STAGING}, not {PRODUCTION}");
        let error = pick(
            &["--profile", "staging", "--server-url", PRODUCTION],
            &Env::default(),
            &set,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), expected);

        let environment = Env::default()
            .with("CAD_PROFILE", "staging")
            .with("CAD_SERVER_URL", PRODUCTION);
        assert_eq!(
            pick(&[], &environment, &set).unwrap_err().to_string(),
            expected
        );

        // The profile's own server, written another way, is the same server.
        let selection = pick(
            &[
                "--profile",
                "staging",
                "--server-url",
                "https://STAGING.carbonaltdelete.eu:443/x",
            ],
            &Env::default(),
            &set,
        )
        .unwrap();
        assert_eq!(chosen(&selection), Some(("staging", ProfileSource::Flag)));
    }

    #[test]
    fn a_foreign_server_never_picks_the_active_profile() {
        let set = profiles(&[]);
        for (arguments, environment) in [
            (
                vec!["--server-url", "https://foreign.example.test"],
                Env::default(),
            ),
            (
                vec![],
                Env::default().with("CAD_SERVER_URL", "https://foreign.example.test"),
            ),
        ] {
            let selection = pick(&arguments, &environment, &set).unwrap();
            assert_eq!(chosen(&selection), None, "{arguments:?}");
            assert_eq!(origin(&selection), "https://foreign.example.test");
        }
    }

    #[test]
    fn a_server_picks_the_active_profile_if_it_matches_else_the_only_one_that_does() {
        let set = profiles(&[("also-staging", STAGING)]);
        // Two profiles share staging, and the active one is one of them.
        let selection = pick(&["--server-url", STAGING], &Env::default(), &set).unwrap();
        assert_eq!(
            chosen(&selection),
            Some(("staging", ProfileSource::ServerFlag))
        );

        // Activate the other: it wins the same way.
        let mut other = set.clone();
        other.activate(name("also-staging"));
        let selection = pick(&["--server-url", STAGING], &Env::default(), &other).unwrap();
        assert_eq!(
            chosen(&selection),
            Some(("also-staging", ProfileSource::ServerFlag))
        );

        // The active profile is elsewhere: the server alone has to decide.
        let mut elsewhere = set.clone();
        elsewhere.activate(name("production"));
        let error = pick(&["--server-url", STAGING], &Env::default(), &elsewhere).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "configuration error: several profiles use {STAGING} (also-staging, staging); \
                 pass --profile"
            )
        );
        let selection = pick(
            &["--server-url", STAGING, "--profile", "also-staging"],
            &Env::default(),
            &elsewhere,
        )
        .unwrap();
        assert_eq!(
            chosen(&selection),
            Some(("also-staging", ProfileSource::Flag))
        );

        // One profile alone on a server is picked although another is active.
        let selection = pick(&["--server-url", PRODUCTION], &Env::default(), &elsewhere).unwrap();
        assert_eq!(
            chosen(&selection),
            Some(("production", ProfileSource::ServerFlag))
        );
    }

    #[test]
    fn without_a_profile_or_a_server_the_error_says_how_to_choose() {
        let error = pick(&[], &Env::default(), &ProfileSet::default()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "configuration error: no profile is selected; run `cad-cli use profile <name>` \
             (production, staging, dev, or a name of your own), pass --profile, or set \
             CAD_SERVER_URL with CAD_API_KEY and CAD_API_SECRET"
        );
        // An `active` that names no profile is no active profile.
        let mut dangling: ProfileSet = serde_json::from_value(serde_json::json!({
            "version": 1, "active": "gone", "profiles": {"staging": {"url": STAGING}}
        }))
        .unwrap();
        assert!(pick(&[], &Env::default(), &dangling).is_err());
        dangling.activate(name("staging"));
        assert!(pick(&[], &Env::default(), &dangling).is_ok());
    }

    // --- the transport policy ------------------------------------------------

    #[test]
    fn insecure_non_loopback_is_rejected() {
        assert!(
            pick(
                &[
                    "--server-url",
                    "http://example.test",
                    "--allow-insecure-http"
                ],
                &Env::default(),
                &ProfileSet::default(),
            )
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
                pick(
                    &["--server-url", value, "--allow-insecure-http"],
                    &Env::default(),
                    &ProfileSet::default(),
                )
                .is_ok(),
                "{value} must be accepted as loopback"
            );
        }
    }

    #[test]
    fn http_loopback_without_the_flag_names_the_flag() {
        let error = pick(
            &["--server-url", "http://127.0.0.1:8080"],
            &Env::default(),
            &ProfileSet::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("--allow-insecure-http"));
    }

    #[test]
    fn a_profiles_opt_in_covers_its_own_server_only() {
        let set = profiles(&[("local", LOCAL)]);
        // Its own server: with the profile, by name or because the server is
        // its, and as the active profile.
        for (arguments, environment) in [
            (vec!["--profile", "local"], Env::default()),
            (vec!["--server-url", LOCAL], Env::default()),
            (vec![], Env::default().with("CAD_PROFILE", "local")),
            (
                vec!["--profile", "local", "--server-url", LOCAL],
                Env::default(),
            ),
        ] {
            let selection = pick(&arguments, &environment, &set).unwrap();
            assert_eq!(origin(&selection), LOCAL, "{arguments:?}");
        }
        // Another loopback server is not covered by it.
        let error = pick(
            &["--server-url", "http://127.0.0.1:5002"],
            &Env::default(),
            &set,
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("--allow-insecure-http"),
            "{error}"
        );
        // A profile without the opt-in does not get one.
        let mut closed = set.clone();
        closed.insert(
            name("local"),
            Profile {
                url: LOCAL.to_owned(),
                allow_insecure_http: false,
                login: None,
                company: None,
            },
        );
        let error = pick(&["--profile", "local"], &Env::default(), &closed).unwrap_err();
        assert!(
            error.to_string().contains("--allow-insecure-http"),
            "{error}"
        );
        assert!(
            pick(
                &["--profile", "local", "--allow-insecure-http"],
                &Env::default(),
                &closed
            )
            .is_ok()
        );
    }

    #[test]
    fn request_timeout_reaches_the_server_config() {
        let config = resolve(&["--request-timeout", "5s"]).unwrap();
        assert_eq!(
            config.server.request_timeout(),
            std::time::Duration::from_secs(5)
        );
    }

    // --- the credentials and the company -------------------------------------

    #[test]
    fn the_profiles_server_is_the_servers_of_a_command() {
        let config = resolve(&[]).unwrap();
        assert_eq!(base_url(&config), "https://staging.carbonaltdelete.eu/");
        let config = resolve(&["--profile", "production"]).unwrap();
        assert_eq!(base_url(&config), "https://app.carbonaltdelete.eu/");
    }

    #[test]
    fn the_environment_decides_alone_and_a_stored_login_for_another_origin_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::support::credentials::CredentialStore::new(
            None,
            Some(crate::support::credentials::FileBackend::new(
                dir.path().join("credentials.json"),
            )),
        );
        // Filed under `staging`, made for production.
        store
            .save_in_file(
                &name("staging"),
                &StoredCredential::api_key(
                    PRODUCTION.to_owned(),
                    uuid::Uuid::from_u128(7),
                    SecretString::from("cad_aks_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde"),
                    uuid::Uuid::nil().into(),
                    chrono::Utc::now(),
                ),
            )
            .unwrap();
        let set = profiles(&[]);
        let global = parse(&[]).global;

        let error = AppConfig::resolve(&global, &Env::default(), &store, &set).unwrap_err();
        assert!(matches!(error, CliError::AuthConfig { .. }), "{error}");
        assert!(
            error.to_string().contains(&format!(
                "the login stored for profile staging is for {PRODUCTION}, not {STAGING}"
            )),
            "{error}"
        );
        // The environment is not subject to the profile's login.
        assert!(AppConfig::resolve(&global, &credentials(), &store, &set).is_ok());
        // Production has no login of its own.
        let production = parse(&["--profile", "production"]).global;
        let error = AppConfig::resolve(&production, &Env::default(), &store, &set).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("profile production has no login"),
            "{error}"
        );
    }

    #[test]
    fn invalid_company_id_is_reported_instead_of_ignored() {
        let environment = credentials().with("CAD_COMPANY_ID", "not-a-uuid");
        let error = AppConfig::resolve(
            &parse(&[]).global,
            &environment,
            &empty_store(),
            &profiles(&[]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("CAD_COMPANY_ID"));
    }

    // --- the company ---------------------------------------------------------

    const ACME: &str = "00000000-0000-0000-0000-0000000000a1";
    const OTHER: &str = "00000000-0000-0000-0000-0000000000a2";
    const FLAGGED: &str = "00000000-0000-0000-0000-0000000000a3";

    /// `profiles(&[])` with Acme as the company of `staging` and no company
    /// for `production`.
    fn profiles_with_a_company() -> ProfileSet {
        let mut set = profiles(&[]);
        let mut staging = set.get(&name("staging")).unwrap().clone();
        staging.company = Some(crate::support::profiles::Company {
            id: ACME.parse().unwrap(),
            name: Some("Acme GmbH".to_owned()),
        });
        set.insert(name("staging"), staging);
        set
    }

    fn company_of(
        arguments: &[&str],
        environment: &Env,
        set: &ProfileSet,
    ) -> Result<Option<(String, Option<String>, CompanySource)>, CliError> {
        let cli = parse(arguments);
        let selection = select(&cli.global, environment, set)?;
        select_company(&cli.global, environment, set, &selection).map(|company| {
            company.map(|company| (company.id.to_string(), company.name, company.source))
        })
    }

    #[test]
    fn the_company_is_the_flag_then_the_variable_then_the_profiles_then_none() {
        let set = profiles_with_a_company();
        let variable = Env::default().with("CAD_COMPANY_ID", OTHER);

        assert_eq!(
            company_of(&[], &Env::default(), &set).unwrap(),
            Some((
                ACME.to_owned(),
                Some("Acme GmbH".to_owned()),
                CompanySource::Profile
            ))
        );
        // The variable beats the profile; it is a company the profile has no
        // label for.
        assert_eq!(
            company_of(&[], &variable, &set).unwrap(),
            Some((OTHER.to_owned(), None, CompanySource::Environment))
        );
        // The flag beats both.
        assert_eq!(
            company_of(&["--company-id", FLAGGED], &variable, &set).unwrap(),
            Some((FLAGGED.to_owned(), None, CompanySource::Flag))
        );
        // The profile's own company by flag keeps the label it recorded, and
        // the flag as its source.
        assert_eq!(
            company_of(&["--company-id", ACME], &Env::default(), &set).unwrap(),
            Some((
                ACME.to_owned(),
                Some("Acme GmbH".to_owned()),
                CompanySource::Flag
            ))
        );
        // A profile without a company has none, and the platform's choice for
        // the key is nowhere in this.
        assert_eq!(
            company_of(&["--profile", "production"], &Env::default(), &set).unwrap(),
            None
        );
        assert_eq!(
            company_of(&[], &Env::default(), &profiles(&[])).unwrap(),
            None
        );
    }

    #[test]
    fn a_server_that_is_no_profiles_never_borrows_a_profiles_company() {
        let set = profiles_with_a_company();
        // `staging` is active and has a company, but this server is not its.
        let foreign = Env::default().with("CAD_SERVER_URL", "https://custom.example.test");
        assert_eq!(company_of(&[], &foreign, &set).unwrap(), None);
        // The profile whose server it is lends it.
        let own = Env::default().with("CAD_SERVER_URL", STAGING);
        assert_eq!(
            company_of(&[], &own, &set)
                .unwrap()
                .map(|company| company.0),
            Some(ACME.to_owned())
        );
    }

    #[test]
    fn a_company_variable_that_is_not_a_uuid_is_an_error() {
        let error = company_of(
            &[],
            &Env::default().with("CAD_COMPANY_ID", "acme"),
            &profiles_with_a_company(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("configuration error: CAD_COMPANY_ID is not a UUID"),
            "{error}"
        );
    }

    #[test]
    fn a_company_scoped_command_is_refused_without_a_company_naming_the_way_out() {
        // With a profile.
        let config = AppConfig::resolve(
            &parse(&[]).global,
            &credentials(),
            &empty_store(),
            &profiles(&[]),
        )
        .unwrap();
        assert!(config.company().is_none());
        let error = config.require_company().unwrap_err();
        assert_eq!(
            error.to_string(),
            "configuration error: no company is selected for profile staging; run `cad-cli use \
             company <name|id>`, or pass --company-id or set CAD_COMPANY_ID"
        );
        assert_eq!(error.exit_code(), std::process::ExitCode::from(2));

        // Without one: a server of no profile.
        let environment = credentials().with("CAD_SERVER_URL", "https://custom.example.test");
        let config = AppConfig::resolve(
            &parse(&[]).global,
            &environment,
            &empty_store(),
            &profiles(&[]),
        )
        .unwrap();
        let error = config.require_company().unwrap_err();
        assert_eq!(
            error.to_string(),
            "configuration error: no company is selected; pass --company-id or set CAD_COMPANY_ID"
        );
        assert_eq!(error.exit_code(), std::process::ExitCode::from(2));

        // With a company, nothing to refuse.
        let config = AppConfig::resolve(
            &parse(&[]).global,
            &credentials(),
            &empty_store(),
            &profiles_with_a_company(),
        )
        .unwrap();
        assert!(config.require_company().is_ok());
    }

    #[test]
    fn debug_redacts_secret() {
        let config = resolve(&[]).unwrap();
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("\"secret\""));
    }
}
