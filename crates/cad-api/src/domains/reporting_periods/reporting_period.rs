use std::fmt;

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::ids::ReportingPeriodId;

/// A reporting period returned by CAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPeriod {
    /// Stable reporting-period identifier.
    #[schemars(with = "String")]
    pub id: ReportingPeriodId,
    /// Human-readable description.
    pub description: String,
    /// Short name displayed in compact UIs.
    pub short_name: String,
    /// Inclusive first day.
    pub start_date: NaiveDate,
    /// Inclusive last day.
    pub end_date: NaiveDate,
    /// Whether inventory mutations are disabled for the period.
    pub is_locked: bool,
}

/// Aggregate information about a reporting period.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPeriodDetails {
    /// Total number of emission entries in the period.
    pub count_measurements: u64,
    /// Total calculated emissions in kilograms of `CO₂e`, when available.
    pub total_emissions_in_kg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReportingPeriodInput {
    description: String,
    short_name: String,
    start_date: NaiveDate,
    end_date: NaiveDate,
    is_locked: bool,
}

impl ReportingPeriodInput {
    fn new(
        description: impl Into<String>,
        short_name: impl Into<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        is_locked: bool,
    ) -> std::result::Result<Self, ReportingPeriodInputError> {
        let input = Self {
            description: description.into(),
            short_name: short_name.into(),
            start_date,
            end_date,
            is_locked,
        };
        input.validate()?;
        Ok(input)
    }

    fn validate(&self) -> std::result::Result<(), ReportingPeriodInputError> {
        if self.short_name.chars().count() > 6 {
            return Err(ReportingPeriodInputError::ShortNameTooLong);
        }
        if self.start_date > self.end_date {
            return Err(ReportingPeriodInputError::InvalidDateRange {
                start_date: self.start_date,
                end_date: self.end_date,
            });
        }
        Ok(())
    }
}

/// Invalid client-side reporting-period input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportingPeriodInputError {
    /// `shortName` exceeds the backend limit of six Unicode scalar values.
    ShortNameTooLong,
    /// The inclusive start date is later than the end date.
    InvalidDateRange {
        /// Supplied start date.
        start_date: NaiveDate,
        /// Supplied end date.
        end_date: NaiveDate,
    },
}

impl fmt::Display for ReportingPeriodInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortNameTooLong => {
                formatter.write_str("shortName must contain at most 6 characters")
            }
            Self::InvalidDateRange {
                start_date,
                end_date,
            } => write!(
                formatter,
                "startDate ({start_date}) must not be later than endDate ({end_date})"
            ),
        }
    }
}

impl std::error::Error for ReportingPeriodInputError {}

/// Input for creating a reporting period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ReportingPeriodCreate(ReportingPeriodInput);

impl ReportingPeriodCreate {
    /// Creates and validates reporting-period input.
    ///
    /// # Errors
    ///
    /// Returns an error when `short_name` is longer than six characters or
    /// `start_date` is after `end_date`.
    pub fn new(
        description: impl Into<String>,
        short_name: impl Into<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        is_locked: bool,
    ) -> std::result::Result<Self, ReportingPeriodInputError> {
        ReportingPeriodInput::new(description, short_name, start_date, end_date, is_locked)
            .map(Self)
    }

    /// Human-readable description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.0.description
    }

    /// Short UI name.
    #[must_use]
    pub fn short_name(&self) -> &str {
        &self.0.short_name
    }

    /// Inclusive start date.
    #[must_use]
    pub const fn start_date(&self) -> NaiveDate {
        self.0.start_date
    }

    /// Inclusive end date.
    #[must_use]
    pub const fn end_date(&self) -> NaiveDate {
        self.0.end_date
    }

    /// Whether the period is locked.
    #[must_use]
    pub const fn is_locked(&self) -> bool {
        self.0.is_locked
    }
}

impl<'de> Deserialize<'de> for ReportingPeriodCreate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = ReportingPeriodInput::deserialize(deserializer)?;
        input.validate().map_err(de::Error::custom)?;
        Ok(Self(input))
    }
}

/// Complete replacement for a reporting period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct ReportingPeriodReplacement(ReportingPeriodInput);

impl ReportingPeriodReplacement {
    /// Creates and validates a replacement.
    ///
    /// # Errors
    ///
    /// Returns an error when `short_name` is longer than six characters or
    /// `start_date` is after `end_date`.
    pub fn new(
        description: impl Into<String>,
        short_name: impl Into<String>,
        start_date: NaiveDate,
        end_date: NaiveDate,
        is_locked: bool,
    ) -> std::result::Result<Self, ReportingPeriodInputError> {
        ReportingPeriodInput::new(description, short_name, start_date, end_date, is_locked)
            .map(Self)
    }
}

impl From<ReportingPeriodCreate> for ReportingPeriodReplacement {
    fn from(value: ReportingPeriodCreate) -> Self {
        Self(value.0)
    }
}

impl<'de> Deserialize<'de> for ReportingPeriodReplacement {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = ReportingPeriodInput::deserialize(deserializer)?;
        input.validate().map_err(de::Error::custom)?;
        Ok(Self(input))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReportingPeriodReplacementWire<'a> {
    id: ReportingPeriodId,
    #[serde(flatten)]
    replacement: &'a ReportingPeriodReplacement,
}

impl<'a> ReportingPeriodReplacementWire<'a> {
    pub(super) const fn new(
        id: ReportingPeriodId,
        replacement: &'a ReportingPeriodReplacement,
    ) -> Self {
        Self { id, replacement }
    }
}
