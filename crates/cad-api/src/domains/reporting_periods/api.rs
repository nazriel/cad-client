use crate::{
    Client, CompanyScoped, Result,
    client::ClientCore,
    domains::DeleteResult,
    ids::{DuplicationStatusId, ReportingPeriodId},
};

use super::{
    duplication::{DuplicationStatus, ReportingPeriodDuplicate},
    input::{ReportingPeriodCreate, ReportingPeriodReplacement},
    model::ReportingPeriod,
    requests,
};

/// Reporting-period API bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct ReportingPeriods<'client> {
    core: &'client ClientCore,
}

impl<'client> ReportingPeriods<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
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
