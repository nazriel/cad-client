use cad_api::domains::reports::{Report, ReportCreate, ReportWithProgress};
use cad_api::ids::ReportId;

use super::SdkAdapter;
use crate::error::CliError;
use crate::exec::reports::ReportsPort;

impl ReportsPort for SdkAdapter {
    async fn get_report(&self, id: ReportId) -> Result<ReportWithProgress, CliError> {
        Ok(self.client.reports().get(id).await?)
    }

    async fn create_report(&self, input: &ReportCreate) -> Result<Report, CliError> {
        Ok(self.client.reports().create(input).await?)
    }
}
