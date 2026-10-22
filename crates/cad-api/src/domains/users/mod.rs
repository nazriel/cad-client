//! User lookups available to any authenticated session.

#[cfg(test)]
mod tests;

use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    Client, CompanyScoped, Connected, Result, client::ClientCore, ids::UserId,
    protocol::RequestSpec,
};

const USER_PATH: &str = "/api/accounts/v1/users/{userId}";

mod requests {
    use super::{Method, RequestSpec, USER_PATH, User, UserId};

    pub(super) fn get(user_id: UserId) -> RequestSpec<(), User> {
        RequestSpec::new(
            Method::GET,
            USER_PATH.replace("{userId}", &user_id.to_string()),
            None,
            None,
        )
    }
}

/// A platform user, as far as the client reads it.
///
/// The response carries more fields than these; they are ignored.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct User {
    pub id: UserId,
    pub email: Option<String>,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
}

/// User API bound to an authenticated session.
#[derive(Clone, Copy)]
pub struct Users<'client> {
    core: &'client ClientCore,
}

impl<'client> Users<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Reads one user by ID.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn get(&self, user_id: UserId) -> Result<User> {
        self.core.execute(requests::get(user_id)).await
    }
}

impl Client<Connected> {
    /// Accesses user operations.
    #[must_use]
    pub fn users(&self) -> Users<'_> {
        Users::new(self.core())
    }
}

impl Client<CompanyScoped> {
    /// Accesses user operations.
    #[must_use]
    pub fn users(&self) -> Users<'_> {
        Users::new(self.core())
    }
}
