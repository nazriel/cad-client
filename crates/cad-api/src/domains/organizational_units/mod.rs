//! Company organizational-unit operations.

use std::fmt;

use reqwest::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Client, CompanyScoped, Result, client::ClientCore, domains::DeleteResult,
    ids::OrganizationalUnitId, protocol::RequestSpec,
};

const ORGANIZATIONAL_UNITS_PATH: &str = "/api/organizational-units/v1/organizational-units";
const MAX_NAME_LENGTH: usize = 255;

/// An organizational unit returned by CAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalUnit {
    /// Human-readable unit name.
    pub name: String,
    /// Zero-based position among sibling units.
    pub position: u32,
    /// Parent unit, or `null` for a root unit.
    #[schemars(
        with = "String",
        required,
        extend("type" = ["string", "null"])
    )]
    pub parent_id: Option<OrganizationalUnitId>,
    /// Stable organizational-unit identifier.
    #[schemars(with = "String")]
    pub id: OrganizationalUnitId,
}

/// Input for creating an organizational unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(deny_unknown_fields)]
pub struct OrganizationalUnitCreate {
    /// Human-readable unit name.
    pub name: String,
    /// Zero-based position among sibling units.
    pub position: u32,
    /// Parent unit for the new unit.
    #[schemars(with = "String")]
    pub parent_id: OrganizationalUnitId,
}

impl OrganizationalUnitCreate {
    /// Creates validated organizational-unit input.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationalUnitInputError`] when `name` exceeds 255 characters.
    pub fn new(
        name: impl Into<String>,
        position: u32,
        parent_id: OrganizationalUnitId,
    ) -> std::result::Result<Self, OrganizationalUnitInputError> {
        let input = Self {
            name: name.into(),
            position,
            parent_id,
        };
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> std::result::Result<(), OrganizationalUnitInputError> {
        validate_name(&self.name)
    }
}

/// Input for completely updating an organizational unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(deny_unknown_fields)]
pub struct OrganizationalUnitUpdate {
    /// Human-readable unit name.
    pub name: String,
    /// Zero-based position among sibling units.
    pub position: u32,
    /// Parent unit for the updated unit.
    #[schemars(with = "String")]
    pub parent_id: OrganizationalUnitId,
}

impl OrganizationalUnitUpdate {
    /// Creates validated organizational-unit update input.
    ///
    /// # Errors
    ///
    /// Returns [`OrganizationalUnitInputError`] when `name` exceeds 255 characters.
    pub fn new(
        name: impl Into<String>,
        position: u32,
        parent_id: OrganizationalUnitId,
    ) -> std::result::Result<Self, OrganizationalUnitInputError> {
        let input = Self {
            name: name.into(),
            position,
            parent_id,
        };
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> std::result::Result<(), OrganizationalUnitInputError> {
        validate_name(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OrganizationalUnitInput {
    name: String,
    position: u32,
    parent_id: OrganizationalUnitId,
}

impl<'de> Deserialize<'de> for OrganizationalUnitCreate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = OrganizationalUnitInput::deserialize(deserializer)?;
        Self::new(input.name, input.position, input.parent_id).map_err(de::Error::custom)
    }
}

impl<'de> Deserialize<'de> for OrganizationalUnitUpdate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = OrganizationalUnitInput::deserialize(deserializer)?;
        Self::new(input.name, input.position, input.parent_id).map_err(de::Error::custom)
    }
}

/// A supplied organizational-unit name exceeded the backend limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrganizationalUnitInputError {
    actual: usize,
}

impl fmt::Display for OrganizationalUnitInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "organizational-unit name contains {} characters; at most {MAX_NAME_LENGTH} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for OrganizationalUnitInputError {}

fn validate_name(name: &str) -> std::result::Result<(), OrganizationalUnitInputError> {
    let actual = name.chars().count();
    if actual > MAX_NAME_LENGTH {
        Err(OrganizationalUnitInputError { actual })
    } else {
        Ok(())
    }
}

/// Number of inventory entries assigned to an organizational unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OrganizationalUnitEntryCount {
    /// Number of entries assigned to the unit.
    pub count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OrganizationalUnitUpdateWire<'a> {
    id: OrganizationalUnitId,
    #[serde(flatten)]
    update: &'a OrganizationalUnitUpdate,
}

mod requests {
    use super::{
        DeleteResult, Method, ORGANIZATIONAL_UNITS_PATH, OrganizationalUnit,
        OrganizationalUnitCreate, OrganizationalUnitEntryCount, OrganizationalUnitId,
        OrganizationalUnitUpdate, OrganizationalUnitUpdateWire, RequestSpec,
    };

    pub(super) fn list() -> RequestSpec<(), Vec<OrganizationalUnit>> {
        RequestSpec::new(Method::GET, ORGANIZATIONAL_UNITS_PATH, None, None)
    }

    pub(super) fn get(id: OrganizationalUnitId) -> RequestSpec<(), OrganizationalUnit> {
        RequestSpec::new(
            Method::GET,
            format!("{ORGANIZATIONAL_UNITS_PATH}/{id}"),
            None,
            None,
        )
    }

    pub(super) fn create(
        input: &OrganizationalUnitCreate,
    ) -> RequestSpec<&OrganizationalUnitCreate, OrganizationalUnit> {
        RequestSpec::new(Method::POST, ORGANIZATIONAL_UNITS_PATH, None, Some(input))
    }

    pub(super) fn update(
        id: OrganizationalUnitId,
        update: &OrganizationalUnitUpdate,
    ) -> RequestSpec<OrganizationalUnitUpdateWire<'_>, OrganizationalUnit> {
        RequestSpec::new(
            Method::PUT,
            format!("{ORGANIZATIONAL_UNITS_PATH}/{id}"),
            None,
            Some(OrganizationalUnitUpdateWire { id, update }),
        )
    }

    pub(super) fn delete(
        id: OrganizationalUnitId,
    ) -> RequestSpec<(), DeleteResult<OrganizationalUnitId>> {
        RequestSpec::new(
            Method::DELETE,
            format!("{ORGANIZATIONAL_UNITS_PATH}/{id}"),
            None,
            None,
        )
    }

    pub(super) fn count_entries(
        id: OrganizationalUnitId,
    ) -> RequestSpec<(), OrganizationalUnitEntryCount> {
        RequestSpec::new(
            Method::GET,
            format!("{ORGANIZATIONAL_UNITS_PATH}/{id}/count-entries"),
            None,
            None,
        )
    }

    pub(super) fn duplicate(
        id: OrganizationalUnitId,
        include_children: bool,
    ) -> RequestSpec<(), Vec<OrganizationalUnit>> {
        RequestSpec::new(
            Method::POST,
            format!("{ORGANIZATIONAL_UNITS_PATH}/{id}/duplicate"),
            Some(vec![(
                "include_children".to_owned(),
                include_children.to_string(),
            )]),
            None,
        )
    }
}

/// Organizational-unit operations bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct OrganizationalUnits<'client> {
    core: &'client ClientCore,
}

impl<'client> OrganizationalUnits<'client> {
    const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists organizational units for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<OrganizationalUnit>> {
        self.core.execute(requests::list()).await
    }

    /// Gets one organizational unit.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn get(&self, id: OrganizationalUnitId) -> Result<OrganizationalUnit> {
        self.core.execute(requests::get(id)).await
    }

    /// Creates an organizational unit.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &OrganizationalUnitCreate) -> Result<OrganizationalUnit> {
        self.core.execute(requests::create(input)).await
    }

    /// Completely updates an organizational unit, supplying its ID in path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn update(
        &self,
        id: OrganizationalUnitId,
        update: &OrganizationalUnitUpdate,
    ) -> Result<OrganizationalUnit> {
        self.core.execute(requests::update(id, update)).await
    }

    /// Deletes an organizational unit.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(
        &self,
        id: OrganizationalUnitId,
    ) -> Result<DeleteResult<OrganizationalUnitId>> {
        self.core.execute(requests::delete(id)).await
    }

    /// Counts inventory entries assigned to an organizational unit.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn count_entries(
        &self,
        id: OrganizationalUnitId,
    ) -> Result<OrganizationalUnitEntryCount> {
        self.core.execute(requests::count_entries(id)).await
    }

    /// Duplicates an organizational unit, optionally including its descendants.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn duplicate(
        &self,
        id: OrganizationalUnitId,
        include_children: bool,
    ) -> Result<Vec<OrganizationalUnit>> {
        self.core
            .execute(requests::duplicate(id, include_children))
            .await
    }
}

impl Client<CompanyScoped> {
    /// Accesses organizational-unit operations.
    #[must_use]
    pub fn organizational_units(&self) -> OrganizationalUnits<'_> {
        OrganizationalUnits::new(self.core())
    }
}

#[cfg(test)]
mod tests;
