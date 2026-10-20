//! Company activity-category operations.

use std::fmt;

use reqwest::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Client, CompanyScoped, Result,
    client::ClientCore,
    domains::DeleteResult,
    ids::{ActivityCategoryId, ActivityGroupId},
    protocol::RequestSpec,
};

const ACTIVITY_CATEGORIES_PATH: &str = "/api/activities/v1/activity-categories";
const MAX_NAME_LENGTH: usize = 128;

/// GHG Protocol category type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ActivityCategoryType {
    #[serde(rename = "STATIONARY_COMBUSTION")]
    StationaryCombustion,
    #[serde(rename = "MOBILE_COMBUSTION")]
    MobileCombustion,
    #[serde(rename = "PROCESS_EMISSIONS")]
    ProcessEmissions,
    #[serde(rename = "FUGITIVE_EMISSIONS")]
    FugitiveEmissions,
    #[serde(rename = "PURCHASED_ELECTRICITY")]
    PurchasedElectricity,
    #[serde(rename = "PURCHASED_STEAM_HEAT_COOLING")]
    PurchasedSteamHeatCooling,
    #[serde(rename = "PURCHASED_GOODS_AND_SERVICES")]
    PurchasedGoodsAndServices,
    #[serde(rename = "CAPITAL_GOODS")]
    CapitalGoods,
    #[serde(rename = "FUEL_AND_ENERGY_RELATED_ACTIVITIES")]
    FuelAndEnergyRelatedActivities,
    #[serde(rename = "UPSTREAM_TRANSPORT_AND_DISTRIBUTION")]
    UpstreamTransportAndDistribution,
    #[serde(rename = "WASTE_GENERATED_IN_OPERATIONS")]
    WasteGeneratedInOperations,
    #[serde(rename = "BUSINESS_TRAVEL")]
    BusinessTravel,
    #[serde(rename = "EMPLOYEE_COMMUTING")]
    EmployeeCommuting,
    #[serde(rename = "UPSTREAM_LEASED_ASSETS")]
    UpstreamLeasedAssets,
    #[serde(rename = "DOWNSTREAM_TRANSPORT_AND_DISTRIBUTION")]
    DownstreamTransportAndDistribution,
    #[serde(rename = "PROCESSING_OF_SOLD_PRODUCTS")]
    ProcessingOfSoldProducts,
    #[serde(rename = "USE_OF_SOLD_PRODUCTS")]
    UseOfSoldProducts,
    #[serde(rename = "END_OF_LIFE_OF_SOLD_PRODUCTS")]
    EndOfLifeOfSoldProducts,
    #[serde(rename = "DOWNSTREAM_LEASED_ASSETS")]
    DownstreamLeasedAssets,
    #[serde(rename = "FRANCHISES")]
    Franchises,
    #[serde(rename = "INVESTMENTS")]
    Investments,
    #[serde(rename = "CUSTOM")]
    Custom,
}

/// GHG scope associated with an activity category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GhgScopeType {
    #[serde(rename = "SCOPE_1")]
    Scope1,
    #[serde(rename = "SCOPE_2")]
    Scope2,
    #[serde(rename = "SCOPE_3")]
    Scope3,
    #[serde(rename = "OUTSIDE_OF_SCOPES")]
    OutsideOfScopes,
    #[serde(rename = "UNDEFINED")]
    Undefined,
}

/// An activity category returned by CAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCategory {
    /// Human-readable category name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the category.
    pub description: String,
    /// Criteria for activity included in the category.
    pub inclusion_criteria: String,
    /// Criteria for activity excluded from the category.
    pub exclusion_criteria: String,
    /// GHG Protocol category type.
    pub activity_category_type: ActivityCategoryType,
    /// Identifier of the parent activity group.
    #[schemars(with = "String")]
    pub activity_group_id: ActivityGroupId,
    /// Zero-based position among sibling categories.
    pub position: u32,
    /// Whether the category is included in reporting.
    pub is_included: bool,
    /// Stable activity-category identifier.
    #[schemars(with = "String")]
    pub id: ActivityCategoryId,
    /// Whether this category is the group's default.
    pub is_default: bool,
    /// GHG scope associated with this category.
    pub ghg_scope_type: GhgScopeType,
    /// GHG Protocol reference supplied by the platform.
    #[serde(rename = "ghg_protocol_reference")]
    pub ghg_protocol_reference: String,
    /// ISO 14064 reference supplied by the platform.
    #[serde(rename = "iso_14064_reference")]
    pub iso_14064_reference: String,
}

/// An activity category with measurement totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityCategoryWithMeasurementCounts {
    /// Human-readable category name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the category.
    pub description: String,
    /// Criteria for activity included in the category.
    pub inclusion_criteria: String,
    /// Criteria for activity excluded from the category.
    pub exclusion_criteria: String,
    /// GHG Protocol category type.
    pub activity_category_type: ActivityCategoryType,
    /// Identifier of the parent activity group.
    #[schemars(with = "String")]
    pub activity_group_id: ActivityGroupId,
    /// Zero-based position among sibling categories.
    pub position: u32,
    /// Whether the category is included in reporting.
    pub is_included: bool,
    /// Stable activity-category identifier.
    #[schemars(with = "String")]
    pub id: ActivityCategoryId,
    /// Whether this category is the group's default.
    pub is_default: bool,
    /// GHG scope associated with this category.
    pub ghg_scope_type: GhgScopeType,
    /// Total measurements in this category.
    pub count_measurements: u64,
    /// Active measurements in this category.
    pub count_active_measurements: u64,
    /// Draft measurements in this category.
    pub count_draft_measurements: u64,
    /// GHG Protocol reference supplied by the platform.
    #[serde(rename = "ghg_protocol_reference")]
    pub ghg_protocol_reference: String,
    /// ISO 14064 reference supplied by the platform.
    #[serde(rename = "iso_14064_reference")]
    pub iso_14064_reference: String,
}

/// Input for completely updating an activity category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(deny_unknown_fields)]
pub struct ActivityCategoryUpdate {
    /// Human-readable category name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the category.
    pub description: String,
    /// Criteria for activity included in the category.
    pub inclusion_criteria: String,
    /// Criteria for activity excluded from the category.
    pub exclusion_criteria: String,
    /// GHG Protocol category type.
    pub activity_category_type: ActivityCategoryType,
    /// Optional name of the parent activity group.
    pub activity_group_name: Option<String>,
    /// Identifier of the parent activity group.
    #[schemars(with = "String")]
    pub activity_group_id: ActivityGroupId,
    /// Zero-based position among sibling categories.
    pub position: u32,
    /// Whether the category is included in reporting.
    pub is_included: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivityCategoryInput {
    name: String,
    description: String,
    inclusion_criteria: String,
    exclusion_criteria: String,
    activity_category_type: ActivityCategoryType,
    activity_group_name: Option<String>,
    activity_group_id: ActivityGroupId,
    position: u32,
    is_included: bool,
}

macro_rules! activity_category_input {
    ($name:ident) => {
        impl $name {
            /// Creates validated activity-category input.
            ///
            /// # Errors
            ///
            /// Returns [`ActivityCategoryInputError`] when `name` exceeds 128 characters.
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                name: impl Into<String>,
                description: impl Into<String>,
                inclusion_criteria: impl Into<String>,
                exclusion_criteria: impl Into<String>,
                activity_category_type: ActivityCategoryType,
                activity_group_name: Option<String>,
                activity_group_id: ActivityGroupId,
                position: u32,
                is_included: bool,
            ) -> std::result::Result<Self, ActivityCategoryInputError> {
                let input = Self {
                    name: name.into(),
                    description: description.into(),
                    inclusion_criteria: inclusion_criteria.into(),
                    exclusion_criteria: exclusion_criteria.into(),
                    activity_category_type,
                    activity_group_name,
                    activity_group_id,
                    position,
                    is_included,
                };
                input.validate()?;
                Ok(input)
            }

            fn validate(&self) -> std::result::Result<(), ActivityCategoryInputError> {
                validate_name(&self.name)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let input = ActivityCategoryInput::deserialize(deserializer)?;
                Self::new(
                    input.name,
                    input.description,
                    input.inclusion_criteria,
                    input.exclusion_criteria,
                    input.activity_category_type,
                    input.activity_group_name,
                    input.activity_group_id,
                    input.position,
                    input.is_included,
                )
                .map_err(de::Error::custom)
            }
        }
    };
}

activity_category_input!(ActivityCategoryUpdate);

/// A supplied activity-category name exceeded the backend limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityCategoryInputError {
    actual: usize,
}

impl fmt::Display for ActivityCategoryInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "activity-category name contains {} characters; at most {MAX_NAME_LENGTH} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for ActivityCategoryInputError {}

fn validate_name(name: &str) -> std::result::Result<(), ActivityCategoryInputError> {
    let actual = name.chars().count();
    if actual > MAX_NAME_LENGTH {
        Err(ActivityCategoryInputError { actual })
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityCategoryUpdateWire<'a> {
    id: ActivityCategoryId,
    #[serde(flatten)]
    update: &'a ActivityCategoryUpdate,
}

mod requests {
    use super::{
        ACTIVITY_CATEGORIES_PATH, ActivityCategory, ActivityCategoryId, ActivityCategoryUpdate,
        ActivityCategoryUpdateWire, ActivityCategoryWithMeasurementCounts, DeleteResult, Method,
        RequestSpec,
    };

    pub(super) fn list() -> RequestSpec<(), Vec<ActivityCategoryWithMeasurementCounts>> {
        RequestSpec::new(Method::GET, ACTIVITY_CATEGORIES_PATH, None, None)
    }

    pub(super) fn update(
        id: ActivityCategoryId,
        update: &ActivityCategoryUpdate,
    ) -> RequestSpec<ActivityCategoryUpdateWire<'_>, ActivityCategoryWithMeasurementCounts> {
        RequestSpec::new(
            Method::PUT,
            format!("{ACTIVITY_CATEGORIES_PATH}/{id}"),
            None,
            Some(ActivityCategoryUpdateWire { id, update }),
        )
    }

    pub(super) fn delete(
        id: ActivityCategoryId,
    ) -> RequestSpec<(), DeleteResult<ActivityCategoryId>> {
        RequestSpec::new(
            Method::DELETE,
            format!("{ACTIVITY_CATEGORIES_PATH}/{id}"),
            None,
            None,
        )
    }

    pub(super) fn duplicate(id: ActivityCategoryId) -> RequestSpec<(), ActivityCategory> {
        RequestSpec::new(
            Method::POST,
            format!("{ACTIVITY_CATEGORIES_PATH}/{id}/duplicate"),
            None,
            None,
        )
    }
}

/// Activity-category operations bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct ActivityCategories<'client> {
    core: &'client ClientCore,
}

impl<'client> ActivityCategories<'client> {
    const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists activity categories for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<ActivityCategoryWithMeasurementCounts>> {
        self.core.execute(requests::list()).await
    }

    /// Completely updates an activity category, supplying its ID in path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn update(
        &self,
        id: ActivityCategoryId,
        update: &ActivityCategoryUpdate,
    ) -> Result<ActivityCategoryWithMeasurementCounts> {
        self.core.execute(requests::update(id, update)).await
    }

    /// Deletes an activity category.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: ActivityCategoryId) -> Result<DeleteResult<ActivityCategoryId>> {
        self.core.execute(requests::delete(id)).await
    }

    /// Duplicates an activity category.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn duplicate(&self, id: ActivityCategoryId) -> Result<ActivityCategory> {
        self.core.execute(requests::duplicate(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses activity-category operations.
    #[must_use]
    pub fn activity_categories(&self) -> ActivityCategories<'_> {
        ActivityCategories::new(self.core())
    }
}

#[cfg(test)]
mod tests;
