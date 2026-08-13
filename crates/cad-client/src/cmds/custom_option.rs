use clap::{Args, Subcommand};

use cad_api::domains::custom_options::{
    CustomOptionBatch, CustomOptionCreate, CustomOptionReplacement,
};
use cad_api::ids::{CustomFieldId, CustomOptionId};

use cad_api::{Client, CompanyScoped};

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
    /// List custom options, optionally filtering by field.
    List(ListOptions),
    /// Create a custom option from JSON input.
    Create(InputArgs),
    /// Create up to 50 custom options from a JSON array.
    CreateBatch(InputArgs),
    /// Fully replace a custom option from JSON input.
    Replace(IdInputArgs<CustomOptionId>),
    /// Delete a custom option.
    Delete(IdArgs<CustomOptionId>),
}

#[derive(Debug, Args)]
pub(crate) struct ListOptions {
    /// Keep only options of this field. The API has no option filter, so every
    /// option is fetched and the selection happens locally.
    #[arg(long)]
    pub(crate) field_id: Option<CustomFieldId>,
}

impl Group {
    pub(crate) async fn run(
        self,
        context: &crate::app::context::CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<serde_json::Value, CliError> {
        let custom_options = client.custom_options();
        match self.command {
            Command::List(args) => to_value(crate::exec::custom_options::prepare(
                custom_options.list().await?,
                args.field_id,
            )),
            Command::Create(args) => {
                let input: CustomOptionCreate = context.read_json(&args.input)?;
                to_value(custom_options.create(&input).await?)
            }
            Command::CreateBatch(args) => {
                let options: Vec<CustomOptionCreate> = context.read_json(&args.input)?;
                let batch = CustomOptionBatch::try_from(options)
                    .map_err(|error| CliError::Input(error.to_string()))?;
                to_value(custom_options.create_batch(&batch).await?)
            }
            Command::Replace(args) => {
                let input: CustomOptionReplacement = context.read_json(&args.input.input)?;
                to_value(custom_options.replace(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this custom option?")?;
                to_value(custom_options.delete(args.id).await?)
            }
        }
    }
}
