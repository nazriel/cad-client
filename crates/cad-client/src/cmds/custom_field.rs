use clap::{Args, Subcommand};

use cad_api::domains::custom_fields::{CustomFieldCreate, CustomFieldReplacement};
use cad_api::ids::CustomFieldId;

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

const LIST_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-field list",
    "List the custom fields in display order",
)];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-field create --input field.json",
    "Create a custom field from a file",
)];

const REPLACE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli custom-field replace 00000000-0000-0000-0000-000000000002 --input field.json",
    "Replace every property of a custom field",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli custom-field delete 00000000-0000-0000-0000-000000000002",
        "Delete a custom field after a confirmation prompt",
    ),
    Example::new(
        "cad-cli custom-field delete 00000000-0000-0000-0000-000000000002 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List custom fields ordered by position.
    #[command(after_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Create a custom field from JSON input.
    #[command(after_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Fully replace a custom field from JSON input.
    #[command(after_help = help::examples(REPLACE_EXAMPLES))]
    Replace(IdInputArgs<CustomFieldId>),
    /// Delete a custom field.
    #[command(after_help = help::examples(DELETE_EXAMPLES))]
    Delete(IdArgs<CustomFieldId>),
}

impl Group {
    pub(crate) async fn run(
        self,
        context: &crate::app::context::CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<serde_json::Value, CliError> {
        let custom_fields = client.custom_fields();
        match self.command {
            Command::List => to_value(crate::exec::custom_fields::prepare(
                custom_fields.list().await?,
            )),
            Command::Create(args) => {
                let input: CustomFieldCreate = context.read_json(&args.input)?;
                to_value(custom_fields.create(&input).await?)
            }
            Command::Replace(args) => {
                let input: CustomFieldReplacement = context.read_json(&args.input.input)?;
                to_value(custom_fields.replace(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this custom field?")?;
                to_value(custom_fields.delete(args.id).await?)
            }
        }
    }
}
