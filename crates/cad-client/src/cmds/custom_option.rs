use clap::{Args, Subcommand};

use cad_api::domains::custom_options::{
    CustomOptionBatch, CustomOptionCreate, CustomOptionReplacement,
};
use cad_api::ids::{CustomFieldId, CustomOptionId};

use cad_api::{Client, CompanyScoped};

use crate::cli::args::{IdArgs, IdInputArgs, InputArgs};
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

const LIST_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli custom-option list",
        "List the options of every custom field",
    ),
    Example::new(
        "cad-cli custom-option list --field-id 00000000-0000-0000-0000-000000000002",
        "List the options of one custom field",
    ),
];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-option create --input option.json",
    "Create a custom option from a file",
)];

const CREATE_BATCH_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-option create-batch --input options.json",
    "Create up to 50 options from a JSON array",
)];

const REPLACE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-option replace 00000000-0000-0000-0000-000000000001 --input option.json",
    "Replace every property of a custom option",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli custom-option delete 00000000-0000-0000-0000-000000000001",
        "Delete a custom option after a confirmation prompt",
    ),
    Example::new(
        "cad-cli custom-option delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List custom options, optionally filtering by field.
    #[command(after_long_help = help::examples(LIST_EXAMPLES))]
    List(ListOptions),
    /// Create a custom option from JSON input.
    #[command(after_long_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Create up to 50 custom options from a JSON array.
    #[command(after_long_help = help::examples(CREATE_BATCH_EXAMPLES))]
    CreateBatch(InputArgs),
    /// Fully replace a custom option from JSON input.
    #[command(after_long_help = help::examples(REPLACE_EXAMPLES))]
    Replace(IdInputArgs<CustomOptionId>),
    /// Delete a custom option.
    #[command(after_long_help = help::examples(DELETE_EXAMPLES))]
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
