use std::future::Future;

use cad_api::domains::reporting_periods::{DuplicationStatus, DuplicationTaskStatus};

use super::ReportingPeriodsPort;
use crate::error::CliError;
use crate::exec::WaitOptions;
use crate::exec::polling::poll_until;

/// Polls reporting-period duplication with an injectable cancellation future.
///
/// The status returned by the duplication request is the first snapshot, so a
/// job that is already finished costs no extra call.
///
/// # Errors
///
/// Returns [`CliError`] for API failures, remote failure, timeout, or cancellation.
pub(crate) async fn wait_for_duplication_with_cancel<F>(
    reporting_periods: &impl ReportingPeriodsPort,
    status: DuplicationStatus,
    options: WaitOptions,
    cancel: F,
) -> Result<DuplicationStatus, CliError>
where
    F: Future<Output = ()>,
{
    let id = status.id;
    poll_until(
        Some(status),
        options,
        cancel,
        || reporting_periods.get_duplication_status(id),
        |status| match status.shared_task_status {
            DuplicationTaskStatus::Done => Ok(true),
            DuplicationTaskStatus::Ongoing => Ok(false),
            DuplicationTaskStatus::Failed => Err(CliError::api(format!(
                "reporting-period duplication {id} failed"
            ))),
        },
        || {
            CliError::Timeout(format!(
                "reporting-period duplication {id} did not finish within {:?}",
                options.timeout
            ))
        },
    )
    .await
}
