use cad_api::ids::CompanyId;
use cad_api::{Client, Connected};

use crate::adapters::sdk::SdkAdapter;
use crate::error::CliError;

/// The authenticated platform handed to every online command.
///
/// Company scoping is resolved on demand rather than by the bootstrap. A
/// command that needs a company asks for it and pays for the round trip; a
/// session-level command never does. Scoping is a real network operation, so
/// asking late does not invent an error path — it moves an existing one next to
/// the command that depends on it.
pub(crate) struct Platform {
    connected: Client<Connected>,
    company_id: Option<CompanyId>,
}

impl Platform {
    pub(crate) const fn new(connected: Client<Connected>, company_id: Option<CompanyId>) -> Self {
        Self {
            connected,
            company_id,
        }
    }

    pub(crate) const fn connected(&self) -> &Client<Connected> {
        &self.connected
    }

    /// The company selected on the command line, if one was.
    pub(crate) const fn requested_company_id(&self) -> Option<CompanyId> {
        self.company_id
    }

    /// Binds the session to a company and wraps it for the domain ports.
    ///
    /// An explicit `--company-id` switches to that company; otherwise the
    /// company already carried by the exchanged token is adopted.
    ///
    /// # Errors
    ///
    /// Returns [`CliError`] if the switch fails or the token carries no company.
    pub(crate) async fn scoped(&self) -> Result<SdkAdapter, CliError> {
        let scoped = match self.company_id {
            Some(company_id) => self.connected.for_company(company_id).await?,
            None => self.connected.clone().scope_current_company().await?,
        };
        Ok(SdkAdapter::new(scoped))
    }
}
