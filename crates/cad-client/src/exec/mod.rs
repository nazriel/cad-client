use std::time::Duration;

pub(crate) mod custom_fields;
pub(crate) mod custom_options;
pub(crate) mod documents;
mod polling;

pub(crate) mod reporting_periods;
pub(crate) mod reports;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy)]
pub(crate) struct WaitOptions {
    pub(crate) poll_interval: Duration,
    pub(crate) timeout: Duration,
}
