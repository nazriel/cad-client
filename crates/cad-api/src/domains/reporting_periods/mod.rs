//! Reporting-period CRUD and asynchronous duplication.

mod api;
mod duplication;
mod input;
mod model;
mod requests;

#[cfg(test)]
mod tests;

#[doc(inline)]
pub use api::ReportingPeriods;
#[doc(inline)]
pub use duplication::{
    DuplicationOptions, DuplicationStatus, DuplicationTaskStatus, ReportingPeriodDuplicate,
};
#[doc(inline)]
pub use input::{ReportingPeriodCreate, ReportingPeriodInputError, ReportingPeriodReplacement};
#[doc(inline)]
pub use model::ReportingPeriod;
