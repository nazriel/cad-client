//! Company custom-field operations backed by the tags API.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{Client, CompanyScoped, Result, client::ClientCore, ids::CustomFieldId};

use super::DeleteResult;

const FIELDS_PATH: &str = "/api/tags/v1/fields";

/// A company-defined field displayed on inventory lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomField {
    /// Stable field identifier.
    pub id: CustomFieldId,
    /// Optional display name.
    pub name: Option<String>,
    /// Zero-based sibling position.
    pub position: u32,
}

const MAX_FIELD_NAME_LENGTH: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomFieldInput {
    name: Option<String>,
    position: u32,
}

impl CustomFieldInput {
    fn validate(&self) -> std::result::Result<(), CustomFieldInputError> {
        if self
            .name
            .as_deref()
            .is_some_and(|name| name.chars().count() > MAX_FIELD_NAME_LENGTH)
        {
            Err(CustomFieldInputError {
                actual: self.name.as_deref().map_or(0, |name| name.chars().count()),
            })
        } else {
            Ok(())
        }
    }
}

/// A custom-field name exceeded the backend limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomFieldInputError {
    actual: usize,
}

impl CustomFieldInputError {
    /// Supplied name length in Unicode scalar values.
    #[must_use]
    pub const fn actual(self) -> usize {
        self.actual
    }

    /// Backend-supported maximum name length.
    #[must_use]
    pub const fn maximum(self) -> usize {
        MAX_FIELD_NAME_LENGTH
    }
}

impl fmt::Display for CustomFieldInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "custom-field name contains {} characters; at most {MAX_FIELD_NAME_LENGTH} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for CustomFieldInputError {}

macro_rules! custom_field_input {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub struct $name(CustomFieldInput);

        impl $name {
            /// Creates validated custom-field input.
            ///
            /// # Errors
            ///
            /// Returns [`CustomFieldInputError`] when a name exceeds 128 characters.
            pub fn new(
                name: Option<String>,
                position: u32,
            ) -> std::result::Result<Self, CustomFieldInputError> {
                let input = CustomFieldInput { name, position };
                input.validate()?;
                Ok(Self(input))
            }

            /// Optional display name.
            #[must_use]
            pub fn name(&self) -> Option<&str> {
                self.0.name.as_deref()
            }

            /// Zero-based sibling position.
            #[must_use]
            pub const fn position(&self) -> u32 {
                self.0.position
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let input = CustomFieldInput::deserialize(deserializer)?;
                input.validate().map_err(de::Error::custom)?;
                Ok(Self(input))
            }
        }
    };
}

custom_field_input!(
    /// Input for creating a custom field.
    CustomFieldCreate
);
custom_field_input!(
    /// Complete replacement for a custom field.
    CustomFieldReplacement
);

impl From<CustomFieldCreate> for CustomFieldReplacement {
    fn from(value: CustomFieldCreate) -> Self {
        Self(value.0)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CustomFieldReplacementWire<'a> {
    id: CustomFieldId,
    #[serde(flatten)]
    replacement: &'a CustomFieldReplacement,
}

mod requests {
    use reqwest::Method;

    use crate::protocol::RequestSpec;

    use super::{
        CustomField, CustomFieldCreate, CustomFieldId, CustomFieldReplacement,
        CustomFieldReplacementWire, DeleteResult, FIELDS_PATH,
    };

    pub(super) fn list() -> RequestSpec<(), Vec<CustomField>> {
        RequestSpec::new(Method::GET, FIELDS_PATH, None, None)
    }

    pub(super) fn create(
        input: &CustomFieldCreate,
    ) -> RequestSpec<&CustomFieldCreate, CustomField> {
        RequestSpec::new(Method::POST, FIELDS_PATH, None, Some(input))
    }

    pub(super) fn replace(
        id: CustomFieldId,
        replacement: &CustomFieldReplacement,
    ) -> RequestSpec<CustomFieldReplacementWire<'_>, CustomField> {
        RequestSpec::new(
            Method::PUT,
            format!("{FIELDS_PATH}/{id}"),
            None,
            Some(CustomFieldReplacementWire { id, replacement }),
        )
    }

    pub(super) fn delete(id: CustomFieldId) -> RequestSpec<(), DeleteResult<CustomFieldId>> {
        RequestSpec::new(Method::DELETE, format!("{FIELDS_PATH}/{id}"), None, None)
    }
}

/// Custom-field operations.
#[derive(Clone, Copy)]
pub struct CustomFields<'client> {
    core: &'client ClientCore,
}

impl<'client> CustomFields<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists custom fields for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<CustomField>> {
        self.core.execute(requests::list()).await
    }

    /// Creates a custom field.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &CustomFieldCreate) -> Result<CustomField> {
        self.core.execute(requests::create(input)).await
    }

    /// Replaces a custom field, supplying its identifier in path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn replace(
        &self,
        id: CustomFieldId,
        replacement: &CustomFieldReplacement,
    ) -> Result<CustomField> {
        self.core.execute(requests::replace(id, replacement)).await
    }

    /// Deletes a custom field.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: CustomFieldId) -> Result<DeleteResult<CustomFieldId>> {
        self.core.execute(requests::delete(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses custom-field operations.
    #[must_use]
    pub fn custom_fields(&self) -> CustomFields<'_> {
        CustomFields::new(self.core())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use super::{CustomFieldCreate, CustomFieldReplacement, FIELDS_PATH, requests};

    #[test]
    fn request_recipes_define_the_custom_field_http_contract_once() {
        let list = requests::list();
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, FIELDS_PATH);
        assert!(list.query.is_none());
        assert!(list.body.is_none());

        let create_input =
            CustomFieldCreate::new(Some("Region".to_owned()), 2).expect("valid custom-field input");
        let create = requests::create(&create_input);
        assert_eq!(create.method, Method::POST);
        assert_eq!(create.path, FIELDS_PATH);
        assert!(create.query.is_none());
        assert_eq!(create.body, Some(&create_input));

        let id = crate::ids::CustomFieldId::from(Uuid::nil());
        let replacement = CustomFieldReplacement::new(Some("Market".to_owned()), 3)
            .expect("valid custom-field replacement");
        let replace = requests::replace(id, &replacement);
        assert_eq!(replace.method, Method::PUT);
        assert_eq!(replace.path, format!("{FIELDS_PATH}/{id}"));
        assert!(replace.query.is_none());
        assert_eq!(
            serde_json::to_value(replace.body.expect("replacement body"))
                .expect("serializable replacement"),
            json!({"id": Uuid::nil(), "name": "Market", "position": 3})
        );

        let delete = requests::delete(id);
        assert_eq!(delete.method, Method::DELETE);
        assert_eq!(delete.path, format!("{FIELDS_PATH}/{id}"));
        assert!(delete.query.is_none());
        assert!(delete.body.is_none());
    }

    #[test]
    fn create_uses_position_wire_name() {
        let input = CustomFieldCreate::new(None, 3).expect("valid field input");

        assert_eq!(
            serde_json::to_value(input).expect("serializable"),
            json!({"name": null, "position": 3})
        );
    }

    #[test]
    fn deserialize_rejects_name_over_backend_limit() {
        let error = serde_json::from_value::<CustomFieldCreate>(json!({
            "name": "x".repeat(129),
            "position": 0
        }))
        .expect_err("oversized name must fail");

        assert!(error.to_string().contains("at most 128"));
    }
}
