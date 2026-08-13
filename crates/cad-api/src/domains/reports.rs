//! Generated-report creation and progress inspection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Client, CompanyScoped, Result,
    client::ClientCore,
    ids::{OrganizationalUnitId, ReportId, ReportingPeriodId},
};

const REPORTS_PATH: &str = "/api/reports/v1/reports";

/// Report formats supported by the current CAD backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReportType {
    /// Native Carbon+Alt+Delete report.
    #[serde(rename = "CARBON_ALT_DELETE")]
    CarbonAltDelete,
    /// ISO 14064 report.
    #[serde(rename = "ISO14064")]
    Iso14064,
    /// GHG Protocol report.
    #[serde(rename = "GHG_PROTOCOL")]
    GhgProtocol,
    /// Excel inventory dump.
    #[serde(rename = "DATA_DUMP")]
    DataDump,
    /// Machine-readable report data.
    #[serde(rename = "REPORT_DATA")]
    ReportData,
    /// European Sustainability Reporting Standards report.
    #[serde(rename = "ESRS")]
    Esrs,
    /// UK Streamlined Energy and Carbon Reporting report.
    #[serde(rename = "SECR")]
    Secr,
    /// Spanish MITECO report.
    #[serde(rename = "MITECO")]
    Miteco,
    /// French BEGES report.
    #[serde(rename = "BEGES")]
    Beges,
    /// CO2 Performance Ladder report.
    #[serde(rename = "CO2_PERFORMANCE_LADDER")]
    Co2PerformanceLadder,
    /// Product carbon-footprint report.
    #[serde(rename = "PCF")]
    Pcf,
    /// Company-specific report.
    #[serde(rename = "CUSTOM")]
    Custom,
    /// Snapshot generation record.
    #[serde(rename = "SNAPSHOT")]
    Snapshot,
}

/// Lifecycle state used for both report data and generated files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReportStatus {
    /// Work is still running.
    Processing,
    /// Work completed successfully.
    Done,
    /// Work failed.
    Failed,
}

/// Language supported by report generation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReportLanguage {
    /// Czech.
    #[serde(rename = "CS")]
    Cs,
    /// Danish.
    #[serde(rename = "DA")]
    Da,
    /// German.
    #[serde(rename = "DE")]
    De,
    /// English.
    #[default]
    #[serde(rename = "EN")]
    En,
    /// Spanish.
    #[serde(rename = "ES")]
    Es,
    /// Finnish.
    #[serde(rename = "FI")]
    Fi,
    /// French.
    #[serde(rename = "FR")]
    Fr,
    /// Italian.
    #[serde(rename = "IT")]
    It,
    /// Dutch.
    #[serde(rename = "NL")]
    Nl,
    /// Portuguese.
    #[serde(rename = "PT")]
    Pt,
    /// Swedish.
    #[serde(rename = "SV")]
    Sv,
}

/// Input for creating an asynchronous report-generation job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCreate {
    /// Display name of the generated report.
    pub name: String,
    /// Organizational unit used as report scope.
    pub organizational_unit_id: OrganizationalUnitId,
    /// Current reporting period.
    pub reporting_period_id: ReportingPeriodId,
    /// Baseline reporting period.
    pub base_reporting_period_id: ReportingPeriodId,
    /// Requested report format.
    pub report_type: ReportType,
    /// Output language. Missing JSON values default to English.
    #[serde(default)]
    pub language: ReportLanguage,
}

impl ReportCreate {
    /// Creates report input with the backend-compatible English default.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        organizational_unit_id: OrganizationalUnitId,
        reporting_period_id: ReportingPeriodId,
        base_reporting_period_id: ReportingPeriodId,
        report_type: ReportType,
    ) -> Self {
        Self {
            name: name.into(),
            organizational_unit_id,
            reporting_period_id,
            base_reporting_period_id,
            report_type,
            language: ReportLanguage::default(),
        }
    }
}

/// A report-generation record returned immediately after creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Stable report identifier.
    pub id: ReportId,
    /// Display name, if retained by the backend.
    pub name: Option<String>,
    /// Organizational unit scope.
    pub organizational_unit_id: Option<OrganizationalUnitId>,
    /// Current reporting period.
    pub reporting_period_id: Option<ReportingPeriodId>,
    /// Baseline reporting period.
    pub base_reporting_period_id: Option<ReportingPeriodId>,
    /// JSON version.
    pub json_version: i64,
    /// Creation timestamp.
    pub created_on: DateTime<Utc>,
    /// Creator's first name.
    pub creator_first_name: Option<String>,
    /// Creator's last name.
    pub creator_last_name: Option<String>,
    /// Generation state of the underlying report data.
    pub report_status: ReportStatus,
    /// Associated snapshot, if this record was generated for a snapshot.
    pub snapshot_id: Option<Uuid>,
    /// Output language.
    pub language: ReportLanguage,
}

/// Report record enriched with generated-file progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportWithProgress {
    /// Shared report metadata and data-generation state.
    #[serde(flatten)]
    pub report: Report,
    /// Aggregate generated-file state.
    pub report_file_status: ReportStatus,
    /// Successfully generated files.
    pub done_report_files: u64,
    /// Failed generated files.
    pub failed_report_files: u64,
    /// Expected generated files.
    pub count_report_files: u64,
}

impl ReportWithProgress {
    /// Whether all expected files completed without failures.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.report.report_status, ReportStatus::Done)
            && self.count_report_files > 0
            && self.failed_report_files == 0
            && self.done_report_files == self.count_report_files
    }

    /// Whether data generation or at least one file failed.
    #[must_use]
    pub const fn has_failed(&self) -> bool {
        matches!(self.report.report_status, ReportStatus::Failed)
            || matches!(self.report_file_status, ReportStatus::Failed)
            || self.failed_report_files > 0
    }
}

mod requests {
    use super::{REPORTS_PATH, Report, ReportCreate, ReportId, ReportWithProgress};
    use crate::protocol::RequestSpec;
    use reqwest::Method;

    pub(super) fn list() -> RequestSpec<(), Vec<ReportWithProgress>> {
        RequestSpec::new(Method::GET, REPORTS_PATH, None, None)
    }

    pub(super) fn get(id: ReportId) -> RequestSpec<(), ReportWithProgress> {
        RequestSpec::new(Method::GET, format!("{REPORTS_PATH}/{id}"), None, None)
    }

    pub(super) fn create(input: &ReportCreate) -> RequestSpec<&ReportCreate, Report> {
        RequestSpec::new(Method::POST, REPORTS_PATH, None, Some(input))
    }
}

/// Report operations.
#[derive(Clone, Copy)]
pub struct Reports<'client> {
    core: &'client ClientCore,
}

impl<'client> Reports<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists report records with aggregate file progress.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<ReportWithProgress>> {
        self.core.execute(requests::list()).await
    }

    /// Gets one report with file progress.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn get(&self, id: ReportId) -> Result<ReportWithProgress> {
        self.core.execute(requests::get(id)).await
    }

    /// Starts asynchronous report generation.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &ReportCreate) -> Result<Report> {
        self.core.execute(requests::create(input)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses generated-report operations.
    #[must_use]
    pub fn reports(&self) -> Reports<'_> {
        Reports::new(self.core())
    }
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use crate::ids::{OrganizationalUnitId, ReportingPeriodId};

    use super::{REPORTS_PATH, ReportCreate, ReportLanguage, ReportType, requests};

    #[test]
    fn request_recipes_define_the_report_http_contract_once() {
        let list = requests::list();
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, REPORTS_PATH);
        assert!(list.query.is_none());
        assert!(list.body.is_none());

        let id = crate::ids::ReportId::from(Uuid::nil());
        let get = requests::get(id);
        assert_eq!(get.method, Method::GET);
        assert_eq!(get.path, format!("{REPORTS_PATH}/{id}"));
        assert!(get.query.is_none());
        assert!(get.body.is_none());

        let period = ReportingPeriodId::from(Uuid::nil());
        let input = ReportCreate::new(
            "GHG 2026",
            OrganizationalUnitId::from(Uuid::nil()),
            period,
            period,
            ReportType::GhgProtocol,
        );
        let create = requests::create(&input);
        assert_eq!(create.method, Method::POST);
        assert_eq!(create.path, REPORTS_PATH);
        assert!(create.query.is_none());
        assert_eq!(create.body, Some(&input));
    }

    #[test]
    fn create_uses_exact_backend_enums_and_english_default() {
        let period = ReportingPeriodId::from(Uuid::nil());
        let input = ReportCreate::new(
            "GHG 2026",
            OrganizationalUnitId::from(Uuid::nil()),
            period,
            period,
            ReportType::GhgProtocol,
        );

        assert_eq!(input.language, ReportLanguage::En);
        assert_eq!(
            serde_json::to_value(input).expect("serializable"),
            json!({
                "name": "GHG 2026",
                "organizationalUnitId": Uuid::nil(),
                "reportingPeriodId": Uuid::nil(),
                "baseReportingPeriodId": Uuid::nil(),
                "reportType": "GHG_PROTOCOL",
                "language": "EN"
            })
        );
    }

    #[test]
    fn language_defaults_when_missing_from_create_json() {
        let input: ReportCreate = serde_json::from_value(json!({
            "name": "CAD 2026",
            "organizationalUnitId": Uuid::nil(),
            "reportingPeriodId": Uuid::nil(),
            "baseReportingPeriodId": Uuid::nil(),
            "reportType": "CARBON_ALT_DELETE"
        }))
        .expect("valid create payload");

        assert_eq!(input.language, ReportLanguage::En);
    }
}
