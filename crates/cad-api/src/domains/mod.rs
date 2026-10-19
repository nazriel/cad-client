//! Company-scoped Carbon+Alt+Delete API capabilities.
//!
//! Every handle in this module can only be obtained from a
//! [`Client<CompanyScoped>`](crate::Client). This keeps authentication and
//! company selection in the type system while the individual handles keep the
//! HTTP details local to their domain.

pub mod benchmarks;
pub mod custom_fields;
pub mod custom_options;
pub mod documents;
pub mod inventory_lines;
pub mod organizational_units;
pub mod reporting_periods;
pub mod reports;

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
