mod duplicate;

use cad_api::domains::reporting_periods::{ReportingPeriodCreate, ReportingPeriodReplacement};
use cad_api::ids::{DuplicationStatusId, ReportingPeriodId};
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::adapters::sdk::SdkAdapter;
use crate::app::context::CommandContext;
use crate::cli::args::{IdArgs, IdInputArgs, InputArgs};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List reporting periods.
    List,
    /// Get one reporting period.
    Get(IdArgs<ReportingPeriodId>),
    /// Create a reporting period from JSON input.
    Create(InputArgs),
    /// Fully replace a reporting period from JSON input.
    Replace(IdInputArgs<ReportingPeriodId>),
    /// Delete a reporting period.
    Delete(IdArgs<ReportingPeriodId>),
    /// Start a reporting-period duplication job.
    Duplicate(duplicate::Options),
    /// Inspect a duplication job.
    DuplicationStatus(IdArgs<DuplicationStatusId>),
}

impl Group {
    /// CRUD is one SDK call each and goes straight to the client; only
    /// `duplicate --wait` needs the port it is tested against.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        sdk: &SdkAdapter,
    ) -> Result<Value, CliError> {
        let reporting_periods = sdk.client().reporting_periods();
        match self.command {
            Command::List => to_value(reporting_periods.list().await?),
            Command::Get(args) => {
                let period = reporting_periods.get(args.id).await?;
                to_value(period)
            }
            Command::Create(args) => {
                let input: ReportingPeriodCreate = context.read_json(&args.input)?;
                to_value(reporting_periods.create(&input).await?)
            }
            Command::Replace(args) => {
                let input: ReportingPeriodReplacement = context.read_json(&args.input.input)?;
                let period = reporting_periods.replace(args.id, &input).await?;
                to_value(period)
            }
            Command::Delete(args) => {
                context.confirm("Delete this reporting period?")?;
                let result = reporting_periods.delete(args.id).await?;
                to_value(result)
            }
            Command::Duplicate(options) => duplicate::run(options, context, sdk).await,
            Command::DuplicationStatus(args) => {
                let status = reporting_periods.duplication_status(args.id).await?;
                to_value(status)
            }
        }
    }
}
