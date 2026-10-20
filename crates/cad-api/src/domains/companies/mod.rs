//! Session-level company operations.

mod company;

#[cfg(test)]
mod tests;

use reqwest::Method;

use crate::{Client, Connected, Result, client::ClientCore, protocol::RequestSpec};

pub use company::{Company, CompanyCreate, CompanyStatus, CompanyTemplateType};

const COMPANIES_PATH: &str = "/api/accounts/v1/companies";

mod requests {
    use super::{COMPANIES_PATH, Company, CompanyCreate, Method, RequestSpec};

    pub(super) fn list() -> RequestSpec<(), Vec<Company>> {
        RequestSpec::new(Method::GET, COMPANIES_PATH, None, None)
    }

    pub(super) fn create(input: &CompanyCreate) -> RequestSpec<&CompanyCreate, Company> {
        RequestSpec::new(Method::POST, COMPANIES_PATH, None, Some(input))
    }
}

/// Company API bound to an authenticated session.
#[derive(Clone, Copy)]
pub struct Companies<'client> {
    core: &'client ClientCore,
}

impl<'client> Companies<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists companies visible to the authenticated user.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<Company>> {
        self.core.execute(requests::list()).await
    }

    /// Creates a company from the supplied initial template selection.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &CompanyCreate) -> Result<Company> {
        self.core.execute(requests::create(input)).await
    }
}

impl Client<Connected> {
    /// Accesses session-level company operations.
    #[must_use]
    pub fn companies(&self) -> Companies<'_> {
        Companies::new(self.core())
    }
}
