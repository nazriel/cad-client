use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{DuplicationStatusId, ReportingPeriodId};

use super::input::ReportingPeriodCreate;

/// Optional data copied as part of a reporting-period duplication.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicationOptions {
    /// Copy inventory entries.
    pub duplicate_entries: bool,
    /// Copy notes and comments.
    pub duplicate_notes: bool,
    /// Copy document attachments.
    pub duplicate_attachments: bool,
}

/// Input for starting a reporting-period duplication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPeriodDuplicate {
    /// Source reporting period.
    pub duplicated_reporting_period_id: ReportingPeriodId,
    /// New reporting-period data.
    #[serde(flatten)]
    pub reporting_period: ReportingPeriodCreate,
    /// Data-copy switches.
    #[serde(flatten)]
    pub options: DuplicationOptions,
}

impl ReportingPeriodDuplicate {
    /// Creates duplication input from already validated reporting-period data.
    #[must_use]
    pub const fn new(
        source: ReportingPeriodId,
        reporting_period: ReportingPeriodCreate,
        options: DuplicationOptions,
    ) -> Self {
        Self {
            duplicated_reporting_period_id: source,
            reporting_period,
            options,
        }
    }
}

/// State of the reporting-period background duplication task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DuplicationTaskStatus {
    /// The task is still running.
    Ongoing,
    /// The task completed successfully.
    Done,
    /// The task ended with an error.
    Failed,
}

/// Progress returned by the duplication endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicationStatus {
    /// Duplication-job identifier used for polling.
    pub id: DuplicationStatusId,
    /// Source reporting period.
    pub orig_reporting_period_id: ReportingPeriodId,
    /// Newly created period, once available.
    pub new_reporting_period_id: Option<ReportingPeriodId>,
    /// Total inventory lines scheduled for copying.
    pub count_measurements_total: u64,
    /// Inventory lines copied so far.
    pub count_measurements_done: u64,
    /// Backend progress value in the inclusive `0.0..=1.0` range.
    pub progress: f64,
    /// Current background-task state.
    pub shared_task_status: DuplicationTaskStatus,
    /// Last state transition timestamp.
    pub shared_task_status_updated_on: DateTime<Utc>,
}
