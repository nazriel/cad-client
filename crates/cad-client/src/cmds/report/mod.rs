mod create;

use cad_api::ids::ReportId;
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::adapters::sdk::SdkAdapter;
use crate::app::context::CommandContext;
use crate::cli::args::IdArgs;
use crate::error::CliError;
use crate::exec::reports::prepare_reports;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List reports ordered from newest to oldest.
    List(ListOptions),
    /// Get one report and its progress counters.
    Get(IdArgs<ReportId>),
    /// Create a report from JSON input.
    Create(create::Options),
}

#[derive(Debug, Args)]
pub(crate) struct ListOptions {
    /// Also show snapshot-generated reports. The API returns the full list, so
    /// this filter and the newest-first ordering are applied locally.
    #[arg(long)]
    pub(crate) include_snapshots: bool,
}

impl Group {
    /// Listing and reading are one SDK call each and go straight to the client;
    /// only `create --wait` needs the port it is tested against.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        sdk: &SdkAdapter,
    ) -> Result<Value, CliError> {
        match self.command {
            Command::List(options) => {
                let reports = sdk.client().reports().list().await?;
                to_value(prepare_reports(reports, options.include_snapshots))
            }
            Command::Get(args) => {
                let report = sdk.client().reports().get(args.id).await?;
                to_value(report)
            }
            Command::Create(options) => create::run(options, context, sdk).await,
        }
    }
}
