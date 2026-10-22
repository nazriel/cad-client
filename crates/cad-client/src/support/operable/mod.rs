//! The companies a login can act on.
//!
//! The companies a key can see can be more than the ones it can act on: a login
//! acts only in its user's own company and in the companies the user holds a
//! role in. `use company`, the choice offered after a login and shell
//! completion all offer that narrower list, so this lives in `support`, where
//! each of them can reach it.

#[cfg(test)]
mod tests;

use cad_api::domains::companies::{Company as PlatformCompany, CompanyStatus};
use cad_api::domains::user_roles::UserRole;
use cad_api::ids::CompanyId;
use cad_api::{Client, Connected};

use super::profiles::{Company, company_label};

/// The companies the login of `client` can act on, by name.
///
/// # Errors
///
/// [`cad_api::Error`] when the companies or the roles cannot be read.
pub(crate) async fn operable_companies(
    client: &Client<Connected>,
) -> Result<Vec<Company>, cad_api::Error> {
    let context = client.context();
    let companies_api = client.companies();
    let roles_api = client.user_roles();
    let (companies, roles) = tokio::try_join!(
        companies_api.list(),
        roles_api.for_user(context.principal.user_id),
    )?;
    Ok(operable(
        &companies,
        &roles,
        context.principal.affiliated_company_id,
    ))
}

/// Keeps the `companies` that are not deleted and are the user's `own` or one
/// a role in `roles` grants, sorted by label, then ID.
fn operable(companies: &[PlatformCompany], roles: &[UserRole], own: CompanyId) -> Vec<Company> {
    let mut operable: Vec<_> = companies
        .iter()
        .filter(|company| company.status != CompanyStatus::Deleted)
        .filter(|company| {
            company.id == own
                || roles
                    .iter()
                    .any(|role| role.company_id == company.id && role.user_role.grants_company())
        })
        .map(|company| Company {
            id: company.id,
            name: company_label(company),
        })
        .collect();
    operable.sort_by_cached_key(|company| (company.label().to_lowercase(), company.id));
    operable
}
