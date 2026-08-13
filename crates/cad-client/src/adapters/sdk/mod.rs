use cad_api::ids::CompanyId;
use cad_api::{Client, CompanyScoped};

mod documents;
mod reporting_periods;
mod reports;

/// The company-scoped platform handed to command groups.
///
/// It is the production implementation of the workflow ports — reports,
/// duplication and document transfers — and, for everything else, simply the
/// SDK client. Wrapping plain CRUD in a port would add a trait and a delegation
/// per call without buying a seam any test wants.
pub(crate) struct SdkAdapter {
    client: Client<CompanyScoped>,
}

impl SdkAdapter {
    pub(crate) const fn new(client: Client<CompanyScoped>) -> Self {
        Self { client }
    }

    /// The SDK client itself, for command groups whose work is one call.
    pub(crate) const fn client(&self) -> &Client<CompanyScoped> {
        &self.client
    }

    /// The company this adapter is bound to, for commands that must name it in
    /// a request body rather than only in the session.
    pub(crate) const fn company_id(&self) -> CompanyId {
        self.client.company_id()
    }
}
