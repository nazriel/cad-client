//! The roles a user holds in companies.

#[cfg(test)]
mod tests;

use reqwest::Method;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Client, CompanyScoped, Connected, Result,
    client::ClientCore,
    ids::{CompanyId, UserId},
    protocol::RequestSpec,
};

const USER_ROLES_PATH: &str = "/api/accounts/v1/user-roles";

mod requests {
    use super::{Method, RequestSpec, USER_ROLES_PATH, UserId, UserRole};

    pub(super) fn for_user(user_id: UserId) -> RequestSpec<(), Vec<UserRole>> {
        RequestSpec::new(
            Method::GET,
            USER_ROLES_PATH,
            Some(vec![("userId_equals".to_owned(), user_id.to_string())]),
            None,
        )
    }
}

/// What a user may do in a company.
///
/// A role the client does not know reads as [`Other`](Self::Other), so a role
/// the platform adds later never breaks reading the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum UserRoleType {
    CadAdmin,
    Admin,
    Expert,
    ReadOnly,
    ReadOnlyLimited,
    #[serde(other)]
    Other,
}

impl UserRoleType {
    /// Whether holding this role lets the user act in the company it is held in.
    ///
    /// `Admin` and `CadAdmin` administer the platform or an account; they grant
    /// no company. A role the client does not know grants none either.
    #[must_use]
    pub const fn grants_company(self) -> bool {
        matches!(self, Self::Expert | Self::ReadOnly | Self::ReadOnlyLimited)
    }
}

/// A role a user holds in a company.
///
/// The response carries more fields than these; they are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct UserRole {
    pub id: Uuid,
    pub user_id: UserId,
    pub company_id: CompanyId,
    pub user_role: UserRoleType,
}

/// User-role API bound to an authenticated session.
#[derive(Clone, Copy)]
pub struct UserRoles<'client> {
    core: &'client ClientCore,
}

impl<'client> UserRoles<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists the roles `user_id` holds, in every company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn for_user(&self, user_id: UserId) -> Result<Vec<UserRole>> {
        self.core.execute(requests::for_user(user_id)).await
    }
}

impl Client<Connected> {
    /// Accesses user-role operations.
    #[must_use]
    pub fn user_roles(&self) -> UserRoles<'_> {
        UserRoles::new(self.core())
    }
}

impl Client<CompanyScoped> {
    /// Accesses user-role operations.
    #[must_use]
    pub fn user_roles(&self) -> UserRoles<'_> {
        UserRoles::new(self.core())
    }
}
