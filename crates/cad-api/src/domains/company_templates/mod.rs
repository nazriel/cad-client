//! Session-level company-template operations.

mod template_company;

#[cfg(test)]
mod tests;

use reqwest::Method;

use crate::{Client, Connected, Result, client::ClientCore, protocol::RequestSpec};

pub use template_company::TemplateCompany;

const COMPANY_TEMPLATES_PATH: &str = "/api/accounts/v1/template-companies";

mod requests {
    use super::{COMPANY_TEMPLATES_PATH, Method, RequestSpec, TemplateCompany};

    pub(super) fn list() -> RequestSpec<(), Vec<TemplateCompany>> {
        RequestSpec::new(Method::GET, COMPANY_TEMPLATES_PATH, None, None)
    }
}

/// Company-template API bound to an authenticated session.
#[derive(Clone, Copy)]
pub struct CompanyTemplates<'client> {
    core: &'client ClientCore,
}

impl<'client> CompanyTemplates<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists company templates visible to the authenticated user.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<TemplateCompany>> {
        self.core.execute(requests::list()).await
    }
}

impl Client<Connected> {
    /// Accesses session-level company-template operations.
    #[must_use]
    pub fn company_templates(&self) -> CompanyTemplates<'_> {
        CompanyTemplates::new(self.core())
    }
}
