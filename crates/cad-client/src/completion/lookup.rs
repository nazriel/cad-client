//! Live candidate lists fetched from the platform through `cad-api`.

use std::time::Duration;

use cad_api::domains::companies::{Company, CompanyStatus};
use cad_api::domains::reporting_periods::ReportingPeriod;

use super::Resource;
use super::cache::Candidate;
use crate::app::platform::Platform;
use crate::config::AppConfig;
use crate::error::CliError;

/// Upper bound for one live lookup, identity request included. A shell waits
/// for the answer, so a slow platform must cost a short pause, not a hang.
pub(crate) const LIVE_TIMEOUT: Duration = Duration::from_secs(2);

/// Fetches the candidates for `resource`, or `None` on any failure.
///
/// Completers are synchronous and run while the application's own runtime is
/// already driving `run()`, where blocking on another future would panic. The
/// lookup therefore gets a private thread and a single-threaded runtime; a
/// panic in it is contained by the join instead of reaching the shell.
pub(crate) fn fetch(resource: Resource, config: AppConfig) -> Option<Vec<Candidate>> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;
        runtime.block_on(async {
            tokio::time::timeout(LIVE_TIMEOUT, list(resource, config))
                .await
                .ok()?
                .ok()
        })
    })
    .join()
    .ok()
    .flatten()
}

async fn list(resource: Resource, config: AppConfig) -> Result<Vec<Candidate>, CliError> {
    let platform = Platform::connect(config).await?;
    Ok(match resource {
        // Companies are a session-level list: any authenticated client may
        // ask for them.
        Resource::Companies => companies(platform.connected().companies().list().await?),
        // Reporting periods belong to one company, so they need the
        // company-scoped client (`--company-id`, `CAD_COMPANY_ID`, or the
        // API key's own company); the SDK's typestate offers the call only
        // on that client.
        Resource::ReportingPeriods => {
            let sdk = platform.scoped()?;
            reporting_periods(sdk.client().reporting_periods().list().await?)
        }
    })
}

/// Companies a `--company-id` can switch to, by name; deleted ones cannot.
fn companies(companies: Vec<Company>) -> Vec<Candidate> {
    let mut companies: Vec<_> = companies
        .into_iter()
        .filter(|company| company.status != CompanyStatus::Deleted)
        .collect();
    companies.sort_by_key(label);
    companies
        .into_iter()
        .map(|company| {
            let help = label(&company);
            Candidate::new(company.id.to_string(), help)
        })
        .collect()
}

fn label(company: &Company) -> Option<String> {
    company
        .display_name
        .clone()
        .or_else(|| company.name.clone())
        .filter(|name| !name.trim().is_empty())
}

/// Reporting periods, most recent first, labelled with name and date range.
fn reporting_periods(mut periods: Vec<ReportingPeriod>) -> Vec<Candidate> {
    periods.sort_by_key(|period| std::cmp::Reverse(period.start_date));
    periods
        .into_iter()
        .map(|period| {
            let help = format!(
                "{} ({} to {})",
                period.short_name, period.start_date, period.end_date
            );
            Candidate::new(period.id.to_string(), Some(help))
        })
        .collect()
}
