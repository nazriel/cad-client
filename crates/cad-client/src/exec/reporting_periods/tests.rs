use std::sync::Mutex;
use std::time::Duration;

use cad_api::domains::reporting_periods::ReportingPeriodDuplicate;
use cad_api::ids::{DuplicationStatusId, ReportingPeriodId};
use chrono::Utc;
use uuid::Uuid;

use cad_api::domains::reporting_periods::{DuplicationStatus, DuplicationTaskStatus};

use super::{ReportingPeriodsPort, wait_for_duplication_with_cancel};
use crate::error::CliError;
use crate::exec::WaitOptions;

struct FakeDuplication {
    results: Mutex<Vec<DuplicationStatus>>,
}

impl ReportingPeriodsPort for FakeDuplication {
    async fn duplicate_reporting_period(
        &self,
        _input: &ReportingPeriodDuplicate,
    ) -> Result<DuplicationStatus, CliError> {
        unreachable!("the wait starts from a status the caller already holds")
    }

    async fn get_duplication_status(
        &self,
        _id: DuplicationStatusId,
    ) -> Result<DuplicationStatus, CliError> {
        Ok(self.results.lock().expect("results lock").remove(0))
    }
}

fn status(shared_task_status: DuplicationTaskStatus) -> DuplicationStatus {
    DuplicationStatus {
        id: DuplicationStatusId::from(Uuid::nil()),
        orig_reporting_period_id: ReportingPeriodId::from(Uuid::nil()),
        new_reporting_period_id: None,
        count_measurements_total: 2,
        count_measurements_done: 0,
        progress: 0.0,
        shared_task_status,
        shared_task_status_updated_on: Utc::now(),
    }
}

fn options() -> WaitOptions {
    WaitOptions {
        poll_interval: Duration::from_millis(1),
        timeout: Duration::from_secs(1),
    }
}

#[tokio::test]
async fn an_already_finished_job_needs_no_further_call() {
    let fake = FakeDuplication {
        // Empty: any refresh would panic on the missing element, which is
        // how this asserts that the seed alone ended the wait.
        results: Mutex::new(Vec::new()),
    };

    let status = wait_for_duplication_with_cancel(
        &fake,
        status(DuplicationTaskStatus::Done),
        options(),
        std::future::pending(),
    )
    .await
    .expect("a finished job must be returned as is");

    assert_eq!(status.shared_task_status, DuplicationTaskStatus::Done);
}

#[tokio::test]
async fn an_ongoing_job_is_polled_until_it_finishes() {
    let fake = FakeDuplication {
        results: Mutex::new(vec![
            status(DuplicationTaskStatus::Ongoing),
            status(DuplicationTaskStatus::Done),
        ]),
    };

    let status = wait_for_duplication_with_cancel(
        &fake,
        status(DuplicationTaskStatus::Ongoing),
        options(),
        std::future::pending(),
    )
    .await
    .expect("the job finishes on the second refresh");

    assert_eq!(status.shared_task_status, DuplicationTaskStatus::Done);
}

#[tokio::test]
async fn a_remote_failure_stops_the_wait() {
    let fake = FakeDuplication {
        results: Mutex::new(vec![status(DuplicationTaskStatus::Failed)]),
    };

    let error = wait_for_duplication_with_cancel(
        &fake,
        status(DuplicationTaskStatus::Ongoing),
        options(),
        std::future::pending(),
    )
    .await
    .expect_err("a failed job must not be reported as success");

    assert!(error.to_string().contains("failed"));
}
