//! Company custom-option operations backed by the tags API.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::{
    Client, CompanyScoped, Result,
    client::ClientCore,
    ids::{CustomFieldId, CustomOptionId},
};

use super::DeleteResult;

const OPTIONS_PATH: &str = "/api/tags/v1/options";
const OPTIONS_BATCH_PATH: &str = "/api/tags/v1/options/batch";
const MAX_BATCH_SIZE: usize = 50;
const MAX_OPTION_VALUE_LENGTH: usize = 1024;

/// A selectable value belonging to a custom field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomOption {
    /// Stable option identifier.
    pub id: CustomOptionId,
    /// Optional label.
    pub value: Option<String>,
    /// Parent custom field.
    pub field_id: CustomFieldId,
    /// Zero-based sibling position within the field.
    pub position: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomOptionInput {
    value: Option<String>,
    field_id: CustomFieldId,
    position: u32,
}

impl CustomOptionInput {
    fn validate(&self) -> std::result::Result<(), CustomOptionInputError> {
        if self
            .value
            .as_deref()
            .is_some_and(|value| value.chars().count() > MAX_OPTION_VALUE_LENGTH)
        {
            Err(CustomOptionInputError {
                actual: self
                    .value
                    .as_deref()
                    .map_or(0, |value| value.chars().count()),
            })
        } else {
            Ok(())
        }
    }
}

/// A custom-option value exceeded the backend limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomOptionInputError {
    actual: usize,
}

impl CustomOptionInputError {
    /// Supplied value length in Unicode scalar values.
    #[must_use]
    pub const fn actual(self) -> usize {
        self.actual
    }

    /// Backend-supported maximum value length.
    #[must_use]
    pub const fn maximum(self) -> usize {
        MAX_OPTION_VALUE_LENGTH
    }
}

impl fmt::Display for CustomOptionInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "custom-option value contains {} characters; at most {MAX_OPTION_VALUE_LENGTH} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for CustomOptionInputError {}

macro_rules! custom_option_input {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        pub struct $name(CustomOptionInput);

        impl $name {
            /// Creates validated custom-option input.
            ///
            /// # Errors
            ///
            /// Returns [`CustomOptionInputError`] when a value exceeds 1024 characters.
            pub fn new(
                value: Option<String>,
                field_id: CustomFieldId,
                position: u32,
            ) -> std::result::Result<Self, CustomOptionInputError> {
                let input = CustomOptionInput {
                    value,
                    field_id,
                    position,
                };
                input.validate()?;
                Ok(Self(input))
            }

            /// Optional display value.
            #[must_use]
            pub fn value(&self) -> Option<&str> {
                self.0.value.as_deref()
            }

            /// Parent custom field.
            #[must_use]
            pub const fn field_id(&self) -> CustomFieldId {
                self.0.field_id
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
                let input = CustomOptionInput::deserialize(deserializer)?;
                input.validate().map_err(de::Error::custom)?;
                Ok(Self(input))
            }
        }
    };
}

custom_option_input!(
    /// Input for creating a custom option.
    CustomOptionCreate
);
custom_option_input!(
    /// Complete replacement for a custom option.
    CustomOptionReplacement
);

impl From<CustomOptionCreate> for CustomOptionReplacement {
    fn from(value: CustomOptionCreate) -> Self {
        Self(value.0)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CustomOptionReplacementWire<'a> {
    id: CustomOptionId,
    #[serde(flatten)]
    replacement: &'a CustomOptionReplacement,
}

/// A validated custom-option batch accepted by the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CustomOptionBatch {
    options: Vec<CustomOptionCreate>,
}

impl CustomOptionBatch {
    /// Maximum number of options accepted by one request.
    pub const MAX_SIZE: usize = MAX_BATCH_SIZE;

    /// Returns the validated options.
    #[must_use]
    pub fn options(&self) -> &[CustomOptionCreate] {
        &self.options
    }

    /// Consumes the batch and returns its options.
    #[must_use]
    pub fn into_options(self) -> Vec<CustomOptionCreate> {
        self.options
    }
}

impl TryFrom<Vec<CustomOptionCreate>> for CustomOptionBatch {
    type Error = CustomOptionBatchError;

    fn try_from(options: Vec<CustomOptionCreate>) -> std::result::Result<Self, Self::Error> {
        if options.len() > MAX_BATCH_SIZE {
            return Err(CustomOptionBatchError {
                actual: options.len(),
            });
        }
        Ok(Self { options })
    }
}

/// A custom-option batch exceeded the backend maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CustomOptionBatchError {
    actual: usize,
}

impl CustomOptionBatchError {
    /// Number of supplied options.
    #[must_use]
    pub const fn actual(self) -> usize {
        self.actual
    }

    /// Maximum supported batch size.
    #[must_use]
    pub const fn maximum(self) -> usize {
        MAX_BATCH_SIZE
    }
}

impl fmt::Display for CustomOptionBatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "custom-option batch contains {} items; at most {MAX_BATCH_SIZE} are allowed",
            self.actual
        )
    }
}

impl std::error::Error for CustomOptionBatchError {}

mod requests {
    use reqwest::Method;

    use crate::protocol::RequestSpec;

    use super::{
        CustomOption, CustomOptionBatch, CustomOptionCreate, CustomOptionId,
        CustomOptionReplacement, CustomOptionReplacementWire, DeleteResult, OPTIONS_BATCH_PATH,
        OPTIONS_PATH,
    };

    pub(super) fn list() -> RequestSpec<(), Vec<CustomOption>> {
        RequestSpec::new(Method::GET, OPTIONS_PATH, None, None)
    }

    pub(super) fn create(
        input: &CustomOptionCreate,
    ) -> RequestSpec<&CustomOptionCreate, CustomOption> {
        RequestSpec::new(Method::POST, OPTIONS_PATH, None, Some(input))
    }

    pub(super) fn create_batch(
        batch: &CustomOptionBatch,
    ) -> RequestSpec<&CustomOptionBatch, Vec<CustomOption>> {
        RequestSpec::new(Method::POST, OPTIONS_BATCH_PATH, None, Some(batch))
    }

    pub(super) fn replace(
        id: CustomOptionId,
        replacement: &CustomOptionReplacement,
    ) -> RequestSpec<CustomOptionReplacementWire<'_>, CustomOption> {
        RequestSpec::new(
            Method::PUT,
            format!("{OPTIONS_PATH}/{id}"),
            None,
            Some(CustomOptionReplacementWire { id, replacement }),
        )
    }

    pub(super) fn delete(id: CustomOptionId) -> RequestSpec<(), DeleteResult<CustomOptionId>> {
        RequestSpec::new(Method::DELETE, format!("{OPTIONS_PATH}/{id}"), None, None)
    }
}

/// Custom-option operations.
#[derive(Clone, Copy)]
pub struct CustomOptions<'client> {
    core: &'client ClientCore,
}

impl<'client> CustomOptions<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists all custom options for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<CustomOption>> {
        self.core.execute(requests::list()).await
    }

    /// Creates one custom option.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &CustomOptionCreate) -> Result<CustomOption> {
        self.core.execute(requests::create(input)).await
    }

    /// Creates up to 50 options atomically in one API request.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create_batch(&self, batch: &CustomOptionBatch) -> Result<Vec<CustomOption>> {
        self.core.execute(requests::create_batch(batch)).await
    }

    /// Replaces a custom option, supplying its identifier in path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn replace(
        &self,
        id: CustomOptionId,
        replacement: &CustomOptionReplacement,
    ) -> Result<CustomOption> {
        self.core.execute(requests::replace(id, replacement)).await
    }

    /// Deletes a custom option.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: CustomOptionId) -> Result<DeleteResult<CustomOptionId>> {
        self.core.execute(requests::delete(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses custom-option operations.
    #[must_use]
    pub fn custom_options(&self) -> CustomOptions<'_> {
        CustomOptions::new(self.core())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use crate::ids::{CustomFieldId, CustomOptionId};

    use super::{
        CustomOptionBatch, CustomOptionCreate, CustomOptionReplacement, OPTIONS_BATCH_PATH,
        OPTIONS_PATH, requests,
    };

    fn option(position: u32) -> CustomOptionCreate {
        CustomOptionCreate::new(
            Some(format!("option {position}")),
            CustomFieldId::from(Uuid::nil()),
            position,
        )
        .expect("valid option input")
    }

    #[test]
    fn request_recipes_define_the_custom_option_http_contract_once() {
        let list = requests::list();
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, OPTIONS_PATH);
        assert!(list.query.is_none());
        assert!(list.body.is_none());

        let create_input = option(0);
        let create = requests::create(&create_input);
        assert_eq!(create.method, Method::POST);
        assert_eq!(create.path, OPTIONS_PATH);
        assert!(create.query.is_none());
        assert_eq!(create.body, Some(&create_input));

        let batch = CustomOptionBatch::try_from(vec![option(1)]).expect("valid batch");
        let create_batch = requests::create_batch(&batch);
        assert_eq!(create_batch.method, Method::POST);
        assert_eq!(create_batch.path, OPTIONS_BATCH_PATH);
        assert!(create_batch.query.is_none());
        assert_eq!(create_batch.body, Some(&batch));

        let id = CustomOptionId::from(Uuid::nil());
        let replacement = CustomOptionReplacement::new(
            Some("Updated".to_owned()),
            CustomFieldId::from(Uuid::nil()),
            2,
        )
        .expect("valid custom-option replacement");
        let replace = requests::replace(id, &replacement);
        assert_eq!(replace.method, Method::PUT);
        assert_eq!(replace.path, format!("{OPTIONS_PATH}/{id}"));
        assert!(replace.query.is_none());
        assert_eq!(
            serde_json::to_value(replace.body.expect("replacement body"))
                .expect("serializable replacement"),
            json!({
                "id": Uuid::nil(),
                "value": "Updated",
                "fieldId": Uuid::nil(),
                "position": 2
            })
        );

        let delete = requests::delete(id);
        assert_eq!(delete.method, Method::DELETE);
        assert_eq!(delete.path, format!("{OPTIONS_PATH}/{id}"));
        assert!(delete.query.is_none());
        assert!(delete.body.is_none());
    }

    #[test]
    fn batch_serializes_using_embedded_options_contract() {
        let batch = CustomOptionBatch::try_from(vec![option(0)]).expect("valid batch");

        assert_eq!(
            serde_json::to_value(batch).expect("serializable"),
            json!({
                "options": [{
                    "value": "option 0",
                    "fieldId": Uuid::nil(),
                    "position": 0
                }]
            })
        );
    }

    #[test]
    fn batch_rejects_more_than_fifty_options() {
        let error = CustomOptionBatch::try_from((0..51).map(option).collect::<Vec<_>>())
            .expect_err("oversized batch must fail");

        assert_eq!(error.actual(), 51);
        assert_eq!(error.maximum(), 50);
    }

    #[test]
    fn deserialize_rejects_value_over_backend_limit() {
        let error = serde_json::from_value::<CustomOptionCreate>(json!({
            "value": "x".repeat(1025),
            "fieldId": Uuid::nil(),
            "position": 0
        }))
        .expect_err("oversized value must fail");

        assert!(error.to_string().contains("at most 1024"));
    }
}
