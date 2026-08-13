use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::ids::ReportingPeriodId;

/// A reporting period returned by CAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPeriod {
    /// Stable reporting-period identifier.
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
