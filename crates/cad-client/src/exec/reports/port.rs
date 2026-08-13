use cad_api::domains::reports::{Report, ReportCreate, ReportWithProgress};
use cad_api::ids::ReportId;

use crate::error::CliError;

/// The report-generation capability, sized to what `report create --wait`
/// needs.
///
/// Listing and reading a report go straight to the SDK from the command group.
/// Creation and progress live here because the wait loop is tested against a
/// fake that scripts a sequence of progress snapshots.
pub(crate) trait ReportsPort: Send + Sync {
    async fn get_report(&self, id: ReportId) -> Result<ReportWithProgress, CliError>;
    async fn create_report(&self, input: &ReportCreate) -> Result<Report, CliError>;
}
