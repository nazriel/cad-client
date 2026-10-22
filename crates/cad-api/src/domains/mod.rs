//! Carbon+Alt+Delete API capabilities.
//!
//! Each handle is available only from the client state required by its service:
//! session-level capabilities use [`Client<Connected>`](crate::Client), while
//! company resources use [`Client<CompanyScoped>`](crate::Client). This keeps
//! authentication and company selection in the type system while individual
//! handles keep HTTP details local to their domain.

pub mod activity_categories;
pub mod activity_groups;
pub mod benchmarks;
pub mod companies;
pub mod company_templates;
pub mod custom_fields;
pub mod custom_options;
pub mod documents;
pub mod inventory_lines;
pub mod organizational_units;
pub mod reporting_periods;
pub mod reports;
pub mod user_roles;
pub mod users;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The common response returned by CAD delete endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult<Id> {
    /// Identifier of the deleted resource.
    #[schemars(with = "String")]
    pub id: Id,
    /// Localized message returned by the service.
    pub msg: String,
}
