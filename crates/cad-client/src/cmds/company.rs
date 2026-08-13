use cad_api::{Client, Connected};
use clap::{Args, Subcommand};
use serde_json::Value;

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
    /// List companies accessible to the authenticated user.
    List,
}

impl Group {
    pub(crate) async fn run(self, client: &Client<Connected>) -> Result<Value, CliError> {
        match self.command {
            Command::List => to_value(client.companies().await?),
        }
    }
}
