//! Reporting-period CRUD and asynchronous duplication.

mod duplication;
mod reporting_period;

#[cfg(test)]
mod tests;

use reqwest::Method;

use crate::{
    Client, CompanyScoped, Result,
    client::ClientCore,
    domains::DeleteResult,
    ids::{DuplicationStatusId, ReportingPeriodId},
    protocol::RequestSpec,
};

#[doc(inline)]
pub use duplication::{
    DuplicationOptions, DuplicationStatus, DuplicationTaskStatus, ReportingPeriodDuplicate,
};
#[doc(inline)]
pub use reporting_period::{
    ReportingPeriod, ReportingPeriodCreate, ReportingPeriodDetails, ReportingPeriodInputError,
    ReportingPeriodReplacement,
};

const REPORTING_PERIODS_PATH: &str = "/api/reporting-periods/v1/reporting-periods";
const REPORTING_PERIOD_DETAILS_SUFFIX: &str = "details";
const DUPLICATION_PATH: &str = "/api/reporting-periods/v1/reporting-periods/duplicate";
const DUPLICATION_STATUS_PATH: &str = "/api/reporting-periods/v1/duplication-status";

mod requests {
    use super::{
        DUPLICATION_PATH, DUPLICATION_STATUS_PATH, DeleteResult, DuplicationStatus,
        DuplicationStatusId, Method, REPORTING_PERIOD_DETAILS_SUFFIX, REPORTING_PERIODS_PATH,
        ReportingPeriod, ReportingPeriodCreate, ReportingPeriodDetails, ReportingPeriodDuplicate,
        ReportingPeriodId, ReportingPeriodReplacement, RequestSpec,
        reporting_period::ReportingPeriodReplacementWire,
    };

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

    pub(super) fn details(id: ReportingPeriodId) -> RequestSpec<(), ReportingPeriodDetails> {
        RequestSpec::new(
            Method::GET,
            format!("{REPORTING_PERIODS_PATH}/{id}/{REPORTING_PERIOD_DETAILS_SUFFIX}"),
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

    pub(super) fn delete(
        id: ReportingPeriodId,
    ) -> RequestSpec<(), DeleteResult<ReportingPeriodId>> {
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

    pub(super) fn duplication_status(
        id: DuplicationStatusId,
    ) -> RequestSpec<(), DuplicationStatus> {
        RequestSpec::new(
            Method::GET,
            format!("{DUPLICATION_STATUS_PATH}/{id}"),
            None,
            None,
        )
    }
}

/// Reporting-period API bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct ReportingPeriods<'client> {
    core: &'client ClientCore,
}

impl<'client> ReportingPeriods<'client> {
    const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists reporting periods for the selected company.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self) -> Result<Vec<ReportingPeriod>> {
        self.core.execute(requests::list()).await
    }

    /// Gets one reporting period.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn get(&self, id: ReportingPeriodId) -> Result<ReportingPeriod> {
        self.core.execute(requests::get(id)).await
    }

    /// Returns aggregate information for one reporting period.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn details(&self, id: ReportingPeriodId) -> Result<ReportingPeriodDetails> {
        self.core.execute(requests::details(id)).await
    }

    /// Creates a reporting period.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &ReportingPeriodCreate) -> Result<ReportingPeriod> {
        self.core.execute(requests::create(input)).await
    }

    /// Replaces a reporting period. The SDK supplies `id` in both path and body.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn replace(
        &self,
        id: ReportingPeriodId,
        replacement: &ReportingPeriodReplacement,
    ) -> Result<ReportingPeriod> {
        self.core.execute(requests::replace(id, replacement)).await
    }

    /// Deletes a reporting period.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: ReportingPeriodId) -> Result<DeleteResult<ReportingPeriodId>> {
        self.core.execute(requests::delete(id)).await
    }

    /// Starts an asynchronous reporting-period duplication.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn duplicate(&self, input: &ReportingPeriodDuplicate) -> Result<DuplicationStatus> {
        self.core.execute(requests::duplicate(input)).await
    }

    /// Reads current duplication progress.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn duplication_status(&self, id: DuplicationStatusId) -> Result<DuplicationStatus> {
        self.core.execute(requests::duplication_status(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses reporting-period operations.
    #[must_use]
    pub fn reporting_periods(&self) -> ReportingPeriods<'_> {
        ReportingPeriods::new(self.core())
    }
}
