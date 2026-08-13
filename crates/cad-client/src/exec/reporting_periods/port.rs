use cad_api::domains::reporting_periods::{DuplicationStatus, ReportingPeriodDuplicate};
use cad_api::ids::DuplicationStatusId;

use crate::error::CliError;

/// The duplication capability, sized to what the wait workflow needs.
///
/// Plain reporting-period CRUD is not here: it goes straight to the SDK from
/// the command group. This port exists because the duplication wait loop is
/// worth testing against a fake that returns a scripted sequence of statuses,
/// which a one-line delegation never is.
pub(crate) trait ReportingPeriodsPort: Send + Sync {
    async fn duplicate_reporting_period(
        &self,
        input: &ReportingPeriodDuplicate,
    ) -> Result<DuplicationStatus, CliError>;

    async fn get_duplication_status(
        &self,
        id: DuplicationStatusId,
    ) -> Result<DuplicationStatus, CliError>;
}
