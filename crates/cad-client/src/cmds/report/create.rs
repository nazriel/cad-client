use cad_api::domains::reports::ReportCreate;
use clap::Args;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::args::WaitableInputArgs;
use crate::error::CliError;
use crate::exec::reports::{ReportsPort, wait_for_report_with_cancel};
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
    port: &impl ReportsPort,
) -> Result<Value, CliError> {
    let input: ReportCreate = context.read_json(&options.request.input.input)?;
    let report = port.create_report(&input).await?;

    if !options.request.wait.enabled {
        return to_value(report);
    }

    let id = report.id;
    let report = context
        .during(
            &format!("waiting for report {id}"),
            wait_for_report_with_cancel(port, id, options.request.wait.options(), cancelled()),
        )
        .await?;
    to_value(report)
}
