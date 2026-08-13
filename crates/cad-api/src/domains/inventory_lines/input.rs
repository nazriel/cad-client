use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::Value;

/// Invalid JSON shape supplied as inventory create or patch input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryInputError;

impl fmt::Display for InventoryInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("inventory input must be a JSON object")
    }
}

impl std::error::Error for InventoryInputError {}

macro_rules! inventory_input {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name(Value);

        impl $name {
            /// Returns the raw, lossless request object.
            #[must_use]
            pub const fn as_value(&self) -> &Value {
                &self.0
            }

            /// Consumes the wrapper.
            #[must_use]
            pub fn into_value(self) -> Value {
                self.0
            }
        }

        impl TryFrom<Value> for $name {
            type Error = InventoryInputError;

            fn try_from(value: Value) -> std::result::Result<Self, Self::Error> {
                if value.is_object() {
                    Ok(Self(value))
                } else {
                    Err(InventoryInputError)
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                self.0.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = Value::deserialize(deserializer)?;
                Self::try_from(value).map_err(de::Error::custom)
            }
        }
    };
}

inventory_input!(
    /// Dynamic `MeasurementCreate` payload used to create an inventory line.
    InventoryLineCreate
);
inventory_input!(
    /// Partial inventory-line payload. Missing and explicit `null` properties
    /// remain distinct in the wrapped JSON object.
    InventoryLinePatch
);
