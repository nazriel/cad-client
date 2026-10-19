//! Newtype identifiers for platform resources.
//!
//! The types deliberately expose no `new()` or `Default`: identifiers are
//! minted by the platform, never by a client, and a `Default` that produced a
//! random UUID would silently create a valid-looking but meaningless value in
//! any future `#[derive(Default)]`.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        #[repr(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<Uuid> for $name {
            fn as_ref(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

identifier!(CompanyId, "A company identifier.");
identifier!(ReportingPeriodId, "A reporting-period identifier.");
identifier!(
    DuplicationStatusId,
    "An identifier used to poll reporting-period duplication."
);
identifier!(InventoryLineId, "An inventory-line identifier.");
identifier!(DocumentId, "A document identifier.");
identifier!(CustomFieldId, "A custom-field identifier.");
identifier!(CustomOptionId, "A custom-option identifier.");
identifier!(ReportId, "A report identifier.");
identifier!(BenchmarkId, "A benchmark identifier.");
identifier!(
    OrganizationalUnitId,
    "An organizational-unit identifier used when creating reports."
);
