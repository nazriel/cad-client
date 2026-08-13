use reqwest::Method;

use crate::{
    domains::DeleteResult,
    ids::{DuplicationStatusId, ReportingPeriodId},
    protocol::RequestSpec,
};

use super::{
    duplication::{DuplicationStatus, ReportingPeriodDuplicate},
    input::{ReportingPeriodCreate, ReportingPeriodReplacement, ReportingPeriodReplacementWire},
    model::ReportingPeriod,
};

pub(super) const REPORTING_PERIODS_PATH: &str = "/api/reporting-periods/v1/reporting-periods";
pub(super) const DUPLICATION_PATH: &str = "/api/reporting-periods/v1/reporting-periods/duplicate";
pub(super) const DUPLICATION_STATUS_PATH: &str = "/api/reporting-periods/v1/duplication-status";

pub(super) fn list() -> RequestSpec<(), Vec<ReportingPeriod>> {
    RequestSpec::new(Method::GET, REPORTING_PERIODS_PATH, None, None)
}

pub(super) fn get(id: ReportingPeriodId) -> RequestSpec<(), ReportingPeriod> {
    RequestSpec::new(
        Method::GET,
        format!("{REPORTING_PERIODS_PATH}/{id}"),
        None,
        None,
    )
}

pub(super) fn create(
    input: &ReportingPeriodCreate,
) -> RequestSpec<&ReportingPeriodCreate, ReportingPeriod> {
    RequestSpec::new(Method::POST, REPORTING_PERIODS_PATH, None, Some(input))
}

pub(super) fn replace(
    id: ReportingPeriodId,
    replacement: &ReportingPeriodReplacement,
) -> RequestSpec<ReportingPeriodReplacementWire<'_>, ReportingPeriod> {
    RequestSpec::new(
        Method::PUT,
        format!("{REPORTING_PERIODS_PATH}/{id}"),
        None,
        Some(ReportingPeriodReplacementWire::new(id, replacement)),
    )
}

pub(super) fn delete(id: ReportingPeriodId) -> RequestSpec<(), DeleteResult<ReportingPeriodId>> {
    RequestSpec::new(
        Method::DELETE,
        format!("{REPORTING_PERIODS_PATH}/{id}"),
        None,
        None,
    )
}

pub(super) fn duplicate(
    input: &ReportingPeriodDuplicate,
) -> RequestSpec<&ReportingPeriodDuplicate, DuplicationStatus> {
    RequestSpec::new(Method::POST, DUPLICATION_PATH, None, Some(input))
}

pub(super) fn duplication_status(id: DuplicationStatusId) -> RequestSpec<(), DuplicationStatus> {
    RequestSpec::new(
        Method::GET,
        format!("{DUPLICATION_STATUS_PATH}/{id}"),
        None,
        None,
    )
}
