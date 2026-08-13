use std::future::Future;

use cad_api::domains::reports::{ReportStatus, ReportWithProgress};
use cad_api::ids::ReportId;

use super::ReportsPort;
use crate::error::CliError;
use crate::exec::WaitOptions;
use crate::exec::polling::poll_until;

/// Polls a report with an injectable cancellation future.
///
/// # Errors
///
/// Returns [`CliError`] for API failures, partial generation failure, timeout,
/// or cancellation.
pub(crate) async fn wait_for_report_with_cancel<F>(
    reports: &impl ReportsPort,
    report_id: ReportId,
    options: WaitOptions,
    cancel: F,
) -> Result<ReportWithProgress, CliError>
where
    F: Future<Output = ()>,
{
    poll_until(
        None,
        options,
        cancel,
        || reports.get_report(report_id),
        |report| is_report_finished(report_id, report),
        || {
            CliError::Timeout(format!(
                "report {report_id} did not finish within {:?}",
                options.timeout
            ))
        },
    )
    .await
}

/// Decides whether a report snapshot is terminal.
///
/// `Done` with zero expected files is not treated as finished. Waiting on
/// status alone would return an empty report as success.
fn is_report_finished(report_id: ReportId, report: &ReportWithProgress) -> Result<bool, CliError> {
    if report.report.report_status == ReportStatus::Failed {
        return Err(CliError::api(format!(
            "report {report_id} data generation failed"
        )));
    }

    let finished_files = report
        .done_report_files
        .saturating_add(report.failed_report_files);
    let files_terminal =
        report.count_report_files > 0 && finished_files >= report.count_report_files;
    if report.report.report_status != ReportStatus::Done || !files_terminal {
        return Ok(false);
    }

    if report.failed_report_files > 0 {
        return Err(CliError::api(format!(
            "report {report_id} finished with {done} successful and {failed} failed files",
            done = report.done_report_files,
            failed = report.failed_report_files,
        )));
    }
    Ok(true)
}
