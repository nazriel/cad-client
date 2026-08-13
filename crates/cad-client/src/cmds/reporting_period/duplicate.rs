use cad_api::domains::reporting_periods::ReportingPeriodDuplicate;
use clap::Args;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::args::WaitableInputArgs;
use crate::error::CliError;
use crate::exec::WaitOptions;
use crate::exec::reporting_periods::{ReportingPeriodsPort, wait_for_duplication_with_cancel};
use crate::output::to_value;
use crate::support::terminal::cancelled;

#[derive(Debug, Args)]
pub(crate) struct Options {
    #[command(flatten)]
    pub(crate) request: WaitableInputArgs,
}

pub(super) async fn run(
    options: Options,
    context: &CommandContext<'_>,
    port: &impl ReportingPeriodsPort,
) -> Result<Value, CliError> {
    let input: ReportingPeriodDuplicate = context.read_json(&options.request.input.input)?;
    let status = port.duplicate_reporting_period(&input).await?;

    if !options.request.wait.enabled {
        return to_value(status);
    }

    let id = status.id;
    let status = context
        .during(
            &format!("waiting for reporting-period duplication {id}"),
            wait_for_duplication_with_cancel(
                port,
                status,
                WaitOptions {
                    poll_interval: options.request.wait.poll_interval,
                    timeout: options.request.wait.timeout,
                },
                cancelled(),
            ),
        )
        .await?;
    to_value(status)
}
