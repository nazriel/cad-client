//! The reports slice: local list shaping and the terminal rules of the wait
//! loop. Both need private items, so they stay in the crate rather than in
//! `tests/`.

use std::sync::Mutex;
use std::time::Duration;

use cad_api::domains::reports::{Report, ReportCreate, ReportLanguage, ReportStatus};
use cad_api::ids::{ReportId, ReportingPeriodId};
use chrono::{TimeZone, Utc};
use uuid::Uuid;

use super::{ReportWithProgress, ReportsPort, prepare_reports, wait_for_report_with_cancel};
use crate::error::CliError;
use crate::exec::WaitOptions;

fn report(day: u32, snapshot: bool) -> ReportWithProgress {
    ReportWithProgress {
        report: Report {
            id: ReportId::from(Uuid::new_v4()),
            name: Some(format!("report {day}")),
            organizational_unit_id: None,
            reporting_period_id: None,
            base_reporting_period_id: None,
            json_version: 1,
            created_on: Utc
                .with_ymd_and_hms(2026, 1, day, 0, 0, 0)
                .single()
                .expect("valid timestamp"),
            creator_first_name: None,
            creator_last_name: None,
            report_status: ReportStatus::Done,
            snapshot_id: snapshot.then(Uuid::new_v4),
            language: ReportLanguage::En,
        },
        report_file_status: ReportStatus::Done,
        done_report_files: 1,
        failed_report_files: 0,
        count_report_files: 1,
    }
}

fn days(reports: &[ReportWithProgress]) -> Vec<u32> {
    reports
        .iter()
        .map(|report| report.report.created_on.format("%d").to_string())
        .map(|day| day.parse().expect("numeric day"))
        .collect()
}

#[test]
fn reports_are_listed_newest_first() {
    let prepared = prepare_reports(
        vec![report(1, false), report(3, false), report(2, false)],
        false,
    );

    assert_eq!(days(&prepared), vec![3, 2, 1]);
}

#[test]
fn snapshots_are_hidden_unless_asked_for() {
    let hidden = prepare_reports(vec![report(1, false), report(2, true)], false);
    assert_eq!(days(&hidden), vec![1]);

    let shown = prepare_reports(vec![report(1, false), report(2, true)], true);
    assert_eq!(days(&shown), vec![2, 1]);
}

struct FakeReports {
    results: Mutex<Vec<ReportWithProgress>>,
}

impl ReportsPort for FakeReports {
    async fn get_report(&self, _id: ReportId) -> Result<ReportWithProgress, CliError> {
        Ok(self.results.lock().unwrap().remove(0))
    }

    async fn create_report(&self, _input: &ReportCreate) -> Result<Report, CliError> {
        unreachable!()
    }
}

fn progress(status: ReportStatus, done: u64, failed: u64, count: u64) -> ReportWithProgress {
    ReportWithProgress {
        report: Report {
            id: ReportId::from_uuid(Uuid::new_v4()),
            name: Some("test".into()),
            organizational_unit_id: Some(Uuid::nil().into()),
            reporting_period_id: Some(ReportingPeriodId::from_uuid(Uuid::new_v4())),
            base_reporting_period_id: Some(ReportingPeriodId::from_uuid(Uuid::new_v4())),
            json_version: 1,
            created_on: Utc::now(),
            creator_first_name: None,
            creator_last_name: None,
            report_status: status,
            snapshot_id: None,
            language: ReportLanguage::En,
        },
        report_file_status: if failed > 0 {
            ReportStatus::Failed
        } else if done == count {
            ReportStatus::Done
        } else {
            ReportStatus::Processing
        },
        done_report_files: done,
        failed_report_files: failed,
        count_report_files: count,
    }
}

#[tokio::test]
async fn report_wait_does_not_stop_on_zero_files() {
    let id = ReportId::from_uuid(Uuid::new_v4());
    let fake = FakeReports {
        results: Mutex::new(vec![
            progress(ReportStatus::Done, 0, 0, 0),
            progress(ReportStatus::Done, 2, 0, 2),
        ]),
    };
    let result = wait_for_report_with_cancel(
        &fake,
        id,
        WaitOptions {
            poll_interval: Duration::from_millis(1),
            timeout: Duration::from_secs(1),
        },
        std::future::pending(),
    )
    .await
    .unwrap();
    assert_eq!(result.done_report_files, 2);
}

#[tokio::test]
async fn report_wait_surfaces_partial_failure_only_when_files_are_terminal() {
    let id = ReportId::from_uuid(Uuid::new_v4());
    let fake = FakeReports {
        results: Mutex::new(vec![
            progress(ReportStatus::Done, 0, 1, 2),
            progress(ReportStatus::Done, 1, 1, 2),
        ]),
    };
    let error = wait_for_report_with_cancel(
        &fake,
        id,
        WaitOptions {
            poll_interval: Duration::from_millis(1),
            timeout: Duration::from_secs(1),
        },
        std::future::pending(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("1 failed files"));
}
