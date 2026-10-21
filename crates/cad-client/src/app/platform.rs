use cad_api::ids::CompanyId;
use cad_api::{Client, CompanyScoped, Connected};

use crate::adapters::sdk::SdkAdapter;
use crate::config::AppConfig;
use crate::error::CliError;

/// The authenticated platform handed to every online command.
///
/// When a company was requested, the one identity request that authenticates
/// the key also selects that company, and `scoped()` hands it out without
/// another request. Without a requested company the key's own company is
/// adopted on demand, which costs no request either; a session-level command
/// never needs one.
pub(crate) struct Platform {
    connected: Client<Connected>,
    /// The client the identity request scoped, when a company was requested.
    requested: Option<Client<CompanyScoped>>,
}

impl Platform {
    const fn new(connected: Client<Connected>, requested: Option<Client<CompanyScoped>>) -> Self {
        Self {
            connected,
            requested,
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
    /// request fails, which includes a requested company the key cannot use.
    pub(crate) async fn connect(config: AppConfig) -> Result<Self, CliError> {
        let (server, credentials, company_id) = config.into_parts();
        let client = Client::new(server)?;
        match company_id {
            Some(company_id) => {
                let scoped = client.connect_for_company(credentials, company_id).await?;
                Ok(Self::new(scoped.unscoped(), Some(scoped)))
            }
            None => Ok(Self::new(client.connect(credentials).await?, None)),
        }
    }

    pub(crate) const fn connected(&self) -> &Client<Connected> {
        &self.connected
    }

    /// The company selected on the command line, if one was.
    pub(crate) fn requested_company_id(&self) -> Option<CompanyId> {
        self.requested.as_ref().map(Client::company_id)
    }

    /// Binds the session to a company and wraps it for the domain ports.
    ///
    /// An explicit `--company-id` is the company the identity request already
    /// selected; otherwise the company the machine identity selected for the
    /// key is adopted.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] if the identity selected no company.
    pub(crate) fn scoped(&self) -> Result<SdkAdapter, CliError> {
        let scoped = match &self.requested {
            Some(scoped) => scoped.clone(),
            None => self.connected.clone().scope_current_company()?,
        };
        Ok(SdkAdapter::new(scoped))
    }
}
