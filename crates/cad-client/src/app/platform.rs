use cad_api::{Client, CompanyScoped, Connected};

use crate::adapters::sdk::SdkAdapter;
use crate::config::{AppConfig, Parts, missing_company};
use crate::error::CliError;
use crate::support::profiles::ProfileName;

/// The authenticated platform handed to every online command.
///
/// When a company is selected, the one identity request that authenticates
/// the key also selects that company, and `scoped()` hands it out without
/// another request. Without one, `scoped()` fails: the company the platform
/// would pick for the key is never adopted. A session-level command never
/// needs a company.
pub(crate) struct Platform {
    connected: Client<Connected>,
    /// The client the identity request scoped, when a company was selected.
    selected: Option<Client<CompanyScoped>>,
    /// The profile in use, named when a missing company is reported.
    profile: Option<ProfileName>,
}

impl Platform {
    const fn new(
        connected: Client<Connected>,
        selected: Option<Client<CompanyScoped>>,
        profile: Option<ProfileName>,
    ) -> Self {
        Self {
            connected,
            selected,
            profile,
        }
    }

    /// Authenticates the resolved configuration against its server.
    ///
    /// The command bootstrap and shell completion both connect through here, so
    /// a completion candidate is fetched exactly the way the command it
    /// completes will later run.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] if the HTTP client cannot be built or the identity
    /// request fails, which includes a selected company the key cannot use.
    pub(crate) async fn connect(config: AppConfig) -> Result<Self, CliError> {
        let Parts {
            server,
            credentials,
            profile,
            company,
        } = config.into_parts();
        let client = Client::new(server)?;
        match company {
            Some(company) => {
                let scoped = client.connect_for_company(credentials, company.id).await?;
                Ok(Self::new(scoped.unscoped(), Some(scoped), profile))
            }
            None => Ok(Self::new(client.connect(credentials).await?, None, profile)),
        }
    }

    pub(crate) const fn connected(&self) -> &Client<Connected> {
        &self.connected
    }

    /// Binds the session to the selected company and wraps it for the domain
    /// ports.
    ///
    /// # Errors
    ///
    /// Returns [`CliError::Config`] if no company was selected. The bootstrap
    /// refuses such a command before connecting; this is the same refusal for
    /// a caller that connected anyway.
    pub(crate) fn scoped(&self) -> Result<SdkAdapter, CliError> {
        self.selected
            .clone()
            .map(SdkAdapter::new)
            .ok_or_else(|| missing_company(self.profile.as_ref()))
    }
}
