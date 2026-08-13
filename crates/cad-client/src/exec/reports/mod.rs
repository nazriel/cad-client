mod polling;
mod port;

#[cfg(test)]
mod tests;

use std::cmp::Reverse;

use cad_api::domains::reports::ReportWithProgress;

pub(crate) use polling::wait_for_report_with_cancel;
pub(crate) use port::ReportsPort;

#[must_use]
pub(crate) fn prepare_reports(
    mut reports: Vec<ReportWithProgress>,
    include_snapshots: bool,
) -> Vec<ReportWithProgress> {
    if !include_snapshots {
        reports.retain(|report| report.report.snapshot_id.is_none());
    }
    reports.sort_by_key(|report| Reverse(report.report.created_on));
    reports
}
