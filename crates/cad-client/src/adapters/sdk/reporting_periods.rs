use cad_api::domains::reporting_periods::{DuplicationStatus, ReportingPeriodDuplicate};
use cad_api::ids::DuplicationStatusId;

use super::SdkAdapter;
use crate::error::CliError;
use crate::exec::reporting_periods::ReportingPeriodsPort;

impl ReportingPeriodsPort for SdkAdapter {
    async fn duplicate_reporting_period(
        &self,
        input: &ReportingPeriodDuplicate,
    ) -> Result<DuplicationStatus, CliError> {
        Ok(self.client.reporting_periods().duplicate(input).await?)
    }

    async fn get_duplication_status(
        &self,
        id: DuplicationStatusId,
    ) -> Result<DuplicationStatus, CliError> {
        Ok(self
            .client
            .reporting_periods()
            .duplication_status(id)
            .await?)
    }
}
