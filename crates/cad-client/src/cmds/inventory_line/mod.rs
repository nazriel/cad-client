mod list;

use clap::{Args, Subcommand};

use cad_api::domains::inventory_lines::{InventoryLineCreate, InventoryLinePatch};
use cad_api::ids::InventoryLineId;

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
    /// List inventory lines using dynamic filters and sorting.
    List(list::Options),
    /// Get one inventory line.
    Get(IdArgs<InventoryLineId>),
    /// Create an inventory line from JSON input.
    Create(InputArgs),
    /// Partially update an inventory line from JSON input.
    Patch(IdInputArgs<InventoryLineId>),
    /// Delete an inventory line.
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
