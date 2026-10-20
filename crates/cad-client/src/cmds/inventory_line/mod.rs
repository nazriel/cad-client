mod list;

use clap::{Args, Subcommand};

use cad_api::domains::inventory_lines::{InventoryLineCreate, InventoryLinePatch};
use cad_api::ids::InventoryLineId;

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
        "cad-cli inventory-line list",
        "List the first 100 inventory lines",
    ),
    Example::new(
        "cad-cli inventory-line list --page 2 --limit 250",
        "Fetch the second page of 250 lines",
    ),
    Example::new(
        "cad-cli inventory-line list --filter country_inList=PL,DE --sort date_desc",
        "Keep lines from Poland or Germany, newest first",
    ),
];

const GET_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli inventory-line get 00000000-0000-0000-0000-000000000001",
    "Show one inventory line",
)];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli inventory-line create --input line.json",
    "Create an inventory line from a file",
)];

const PATCH_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli inventory-line patch 00000000-0000-0000-0000-000000000001 --input changes.json",
    "Change only the fields present in the file",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli inventory-line delete 00000000-0000-0000-0000-000000000001",
        "Delete an inventory line after a confirmation prompt",
    ),
    Example::new(
        "cad-cli inventory-line delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List inventory lines using dynamic filters and sorting.
    #[command(after_long_help = help::examples(LIST_EXAMPLES))]
    List(list::Options),
    /// Get one inventory line.
    #[command(after_long_help = help::examples(GET_EXAMPLES))]
    Get(IdArgs<InventoryLineId>),
    /// Create an inventory line from JSON input.
    #[command(after_long_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Partially update an inventory line from JSON input.
    #[command(after_long_help = help::examples(PATCH_EXAMPLES))]
    Patch(IdInputArgs<InventoryLineId>),
    /// Delete an inventory line.
    #[command(after_long_help = help::examples(DELETE_EXAMPLES))]
    Delete(IdArgs<InventoryLineId>),
}

impl Group {
    pub(crate) async fn run(
        self,
        context: &crate::app::context::CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<serde_json::Value, CliError> {
        let inventory_lines = client.inventory_lines();
        match self.command {
            Command::List(options) => {
                let params = options.into_params()?;
                to_value(inventory_lines.list(&params).await?)
            }
            Command::Get(args) => to_value(inventory_lines.get(args.id).await?),
            Command::Create(args) => {
                let input: InventoryLineCreate = context.read_json(&args.input)?;
                to_value(inventory_lines.create(&input).await?)
            }
            Command::Patch(args) => {
                let input: InventoryLinePatch = context.read_json(&args.input.input)?;
                to_value(inventory_lines.patch(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this inventory line?")?;
                to_value(inventory_lines.delete(args.id).await?)
            }
        }
    }
}
