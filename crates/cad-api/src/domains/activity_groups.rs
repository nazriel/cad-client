//! Company activity-group operations.

use std::fmt;

use reqwest::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Client, CompanyScoped, Result, client::ClientCore, domains::DeleteResult, ids::ActivityGroupId,
    protocol::RequestSpec,
};

const ACTIVITY_GROUPS_PATH: &str = "/api/activities/v1/activity-groups";
const MAX_NAME_LENGTH: usize = 128;

/// An activity group returned by CAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivityGroup {
    /// Zero-based position among sibling groups.
    pub position: u32,
    /// Human-readable group name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the activities contained in this group.
    pub description: String,
    /// Stable activity-group identifier.
    #[schemars(with = "String")]
    pub id: ActivityGroupId,
}

/// Input for creating an activity group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(deny_unknown_fields)]
pub struct ActivityGroupCreate {
    /// Zero-based position among sibling groups.
    pub position: u32,
    /// Human-readable group name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the activities contained in this group.
    pub description: String,
}

/// Input for completely updating an activity group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[schemars(deny_unknown_fields)]
pub struct ActivityGroupUpdate {
    /// Zero-based position among sibling groups.
    pub position: u32,
    /// Human-readable group name.
    #[schemars(length(max = 128))]
    pub name: String,
    /// Description of the activities contained in this group.
    pub description: String,
}

macro_rules! activity_group_input {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        impl $name {
            /// Creates validated activity-group input.
            ///
            /// # Errors
            ///
            /// Returns [`ActivityGroupInputError`] when `name` exceeds 128 characters.
            pub fn new(
                position: u32,
                name: impl Into<String>,
                description: impl Into<String>,
            ) -> std::result::Result<Self, ActivityGroupInputError> {
                let input = Self {
                    position,
                    name: name.into(),
                    description: description.into(),
                };
                input.validate()?;
                Ok(input)
            }

            fn validate(&self) -> std::result::Result<(), ActivityGroupInputError> {
                validate_name(&self.name)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase", deny_unknown_fields)]
                struct Input {
                    position: u32,
                    name: String,
                    description: String,
                }

                let input = Input::deserialize(deserializer)?;
                Self::new(input.position, input.name, input.description).map_err(de::Error::custom)
            }
        }
    };
}

activity_group_input!(
    /// Input for creating an activity group.
    ActivityGroupCreate
);
activity_group_input!(
    /// Input for completely updating an activity group.
    ActivityGroupUpdate
);

/// A supplied activity-group name exceeded the backend limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityGroupInputError {
    actual: usize,
}

impl fmt::Display for ActivityGroupInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "activity-group name contains {} characters; at most {MAX_NAME_LENGTH} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for ActivityGroupInputError {}

fn validate_name(name: &str) -> std::result::Result<(), ActivityGroupInputError> {
    let actual = name.chars().count();
    if actual > MAX_NAME_LENGTH {
        Err(ActivityGroupInputError { actual })
    } else {
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityGroupUpdateWire<'a> {
    id: ActivityGroupId,
    #[serde(flatten)]
    update: &'a ActivityGroupUpdate,
}

mod requests {
    use super::{
        ACTIVITY_GROUPS_PATH, ActivityGroup, ActivityGroupCreate, ActivityGroupId,
        ActivityGroupUpdate, ActivityGroupUpdateWire, DeleteResult, Method, RequestSpec,
    };

    pub(super) fn list() -> RequestSpec<(), Vec<ActivityGroup>> {
        RequestSpec::new(Method::GET, ACTIVITY_GROUPS_PATH, None, None)
    }

    pub(super) fn create(
        input: &ActivityGroupCreate,
    ) -> RequestSpec<&ActivityGroupCreate, ActivityGroup> {
        RequestSpec::new(Method::POST, ACTIVITY_GROUPS_PATH, None, Some(input))
    }

    pub(super) fn update(
        id: ActivityGroupId,
        update: &ActivityGroupUpdate,
    ) -> RequestSpec<ActivityGroupUpdateWire<'_>, ActivityGroup> {
        RequestSpec::new(
            Method::PUT,
            format!("{ACTIVITY_GROUPS_PATH}/{id}"),
            None,
            Some(ActivityGroupUpdateWire { id, update }),
        )
    }

    pub(super) fn delete(id: ActivityGroupId) -> RequestSpec<(), DeleteResult<ActivityGroupId>> {
        RequestSpec::new(
            Method::DELETE,
            format!("{ACTIVITY_GROUPS_PATH}/{id}"),
            None,
            None,
        )
    }
}

/// Activity-group operations bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct ActivityGroups<'client> {
    core: &'client ClientCore,
}

impl<'client> ActivityGroups<'client> {
    const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists activity groups for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<ActivityGroup>> {
        self.core.execute(requests::list()).await
    }

    /// Creates an activity group.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &ActivityGroupCreate) -> Result<ActivityGroup> {
        self.core.execute(requests::create(input)).await
    }

    /// Completely updates an activity group, supplying its ID in path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn update(
        &self,
        id: ActivityGroupId,
        update: &ActivityGroupUpdate,
    ) -> Result<ActivityGroup> {
        self.core.execute(requests::update(id, update)).await
    }

    /// Deletes an activity group.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: ActivityGroupId) -> Result<DeleteResult<ActivityGroupId>> {
        self.core.execute(requests::delete(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses activity-group operations.
    #[must_use]
    pub fn activity_groups(&self) -> ActivityGroups<'_> {
        ActivityGroups::new(self.core())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use super::{ACTIVITY_GROUPS_PATH, ActivityGroupCreate, ActivityGroupUpdate, requests};
    use crate::ids::ActivityGroupId;

    #[test]
    fn request_recipes_define_the_activity_group_http_contract_once() {
        let id = ActivityGroupId::from(Uuid::nil());
        let create =
            ActivityGroupCreate::new(0, "Scope 1", "Direct emissions").expect("valid create input");
        let update = ActivityGroupUpdate::new(1, "Scope 2", "Indirect emissions")
            .expect("valid update input");

        let list = requests::list();
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, ACTIVITY_GROUPS_PATH);

        let create_request = requests::create(&create);
        assert_eq!(create_request.method, Method::POST);
        assert_eq!(create_request.path, ACTIVITY_GROUPS_PATH);
        assert_eq!(
            serde_json::to_value(create_request.body.expect("create body")).expect("JSON body"),
            json!({"position": 0, "name": "Scope 1", "description": "Direct emissions"})
        );

        let update_request = requests::update(id, &update);
        assert_eq!(update_request.method, Method::PUT);
        assert_eq!(update_request.path, format!("{ACTIVITY_GROUPS_PATH}/{id}"));
        assert_eq!(
            serde_json::to_value(update_request.body.expect("update body")).expect("JSON body"),
            json!({"id": Uuid::nil(), "position": 1, "name": "Scope 2", "description": "Indirect emissions"})
        );

        let delete = requests::delete(id);
        assert_eq!(delete.method, Method::DELETE);
        assert_eq!(delete.path, format!("{ACTIVITY_GROUPS_PATH}/{id}"));
    }

    #[test]
    fn input_rejects_unknown_fields_and_long_names() {
        let error = serde_json::from_value::<ActivityGroupCreate>(json!({
            "position": 0,
            "name": "Scope 1",
            "description": "Direct emissions",
            "id": Uuid::nil()
        }))
        .expect_err("server-managed ID must be rejected");
        assert!(error.to_string().contains("unknown field"));

        let error = ActivityGroupCreate::new(0, "x".repeat(129), "Description")
            .expect_err("long name must fail");
        assert!(error.to_string().contains("at most 128"));
    }
}
