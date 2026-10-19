//! Singleton benchmark operations.
//!
//! The benchmarks endpoint returns an array even though a company has one
//! benchmark. This module keeps that transport detail private: callers read and
//! mutate one [`Benchmark`].

mod codes;

use reqwest::Method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Client, CompanyScoped, Error, Result,
    client::ClientCore,
    ids::{BenchmarkId, ReportingPeriodId},
    protocol::RequestSpec,
};

const BENCHMARKS_PATH: &str = "/api/benchmarks/v1/benchmarks";

pub use codes::{HighLevelSectorCode, Location, LowLevelSectorCode, SectorGroup};

/// The company benchmark returned by CAD.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Benchmark {
    /// Stable benchmark identifier managed by CAD.
    #[schemars(with = "String")]
    pub id: BenchmarkId,
    /// Reporting period this benchmark describes.
    #[schemars(with = "String")]
    pub reporting_period_id: ReportingPeriodId,
    /// Number of full-time employees, when known.
    pub full_time_employees: Option<f64>,
    /// Gross annual revenue, when known.
    pub gross_annual_revenue: Option<f64>,
    /// Unit used for gross annual revenue.
    #[schemars(with = "String")]
    pub gross_annual_revenue_unit_id: Uuid,
    /// Top-level sector code, when classified.
    pub high_level_sector_code: Option<HighLevelSectorCode>,
    /// Detailed sector code, when classified.
    pub low_level_sector_code: Option<LowLevelSectorCode>,
    /// Geographic location, when classified.
    pub location: Option<Location>,
    /// Derived sector group, when available.
    pub sector_group: Option<SectorGroup>,
}

/// Benchmark fields accepted by the update endpoint.
///
/// CAD owns the benchmark identifier. The SDK obtains it from the singleton
/// collection response and supplies it in both the URL and request body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BenchmarkInput {
    /// Reporting period this benchmark describes.
    #[schemars(with = "String")]
    pub reporting_period_id: ReportingPeriodId,
    /// Number of full-time employees, when known.
    pub full_time_employees: Option<f64>,
    /// Gross annual revenue, when known.
    pub gross_annual_revenue: Option<f64>,
    /// Unit used for gross annual revenue.
    #[schemars(with = "String")]
    pub gross_annual_revenue_unit_id: Uuid,
    /// Top-level sector code, when classified.
    pub high_level_sector_code: Option<HighLevelSectorCode>,
    /// Detailed sector code, when classified.
    pub low_level_sector_code: Option<LowLevelSectorCode>,
    /// Geographic location, when classified.
    pub location: Option<Location>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkUpdate<'a> {
    id: BenchmarkId,
    #[serde(flatten)]
    input: &'a BenchmarkInput,
}

mod requests {
    use super::{
        BENCHMARKS_PATH, Benchmark, BenchmarkId, BenchmarkInput, BenchmarkUpdate, Method,
        RequestSpec,
    };

    pub(super) fn get_benchmark() -> RequestSpec<(), Vec<Benchmark>> {
        RequestSpec::new(Method::GET, BENCHMARKS_PATH, None, None)
    }

    pub(super) fn put(
        id: BenchmarkId,
        input: &BenchmarkInput,
    ) -> RequestSpec<BenchmarkUpdate<'_>, Benchmark> {
        RequestSpec::new(
            Method::PUT,
            format!("{BENCHMARKS_PATH}/{id}"),
            None,
            Some(BenchmarkUpdate { id, input }),
        )
    }
}

/// Benchmark API bound to a company-scoped client.
#[derive(Clone, Copy)]
pub struct Benchmarks<'client> {
    core: &'client ClientCore,
}

impl<'client> Benchmarks<'client> {
    pub(crate) const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Reads the selected company's benchmark.
    ///
    /// # Errors
    ///
    /// Returns an error when CAD returns anything other than one benchmark, or
    /// for transport, protocol, authentication, or API failures.
    pub async fn read(&self) -> Result<Benchmark> {
        Self::single(self.get_benchmark().await?)
    }

    /// Creates or initializes the selected company's benchmark.
    ///
    /// CAD exposes creation through its singleton collection plus the
    /// ID-addressed update endpoint. The collection's array response is hidden
    /// from callers.
    ///
    /// # Errors
    ///
    /// Returns an error when CAD returns anything other than one benchmark, or
    /// for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &BenchmarkInput) -> Result<Benchmark> {
        self.put_current(input).await
    }

    /// Updates the selected company's benchmark.
    ///
    /// # Errors
    ///
    /// Returns an error when CAD returns anything other than one benchmark, or
    /// for transport, protocol, authentication, or API failures.
    pub async fn update(&self, input: &BenchmarkInput) -> Result<Benchmark> {
        self.put_current(input).await
    }

    async fn put_current(&self, input: &BenchmarkInput) -> Result<Benchmark> {
        let benchmark = Self::single(self.get_benchmark().await?)?;
        self.core.execute(requests::put(benchmark.id, input)).await
    }

    async fn get_benchmark(&self) -> Result<Vec<Benchmark>> {
        self.core.execute(requests::get_benchmark()).await
    }

    fn single(mut benchmarks: Vec<Benchmark>) -> Result<Benchmark> {
        match benchmarks.len() {
            1 => Ok(benchmarks.pop().expect("one benchmark has one element")),
            count => Err(Error::protocol(format!(
                "expected exactly one benchmark for the selected company, received {count}"
            ))),
        }
    }
}

impl Client<CompanyScoped> {
    /// Accesses the selected company's singleton benchmark.
    #[must_use]
    pub fn benchmarks(&self) -> Benchmarks<'_> {
        Benchmarks::new(self.core())
    }
}

#[cfg(test)]
mod tests;
