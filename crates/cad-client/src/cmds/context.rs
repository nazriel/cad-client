use cad_api::ids::CompanyId;
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
    /// Show non-sensitive claims from the active session.
    Show,
}

impl Group {
    pub(crate) async fn run(
        self,
        client: &Client<Connected>,
        company_id: Option<CompanyId>,
    ) -> Result<Value, CliError> {
        match self.command {
            Command::Show => {
                let session = if let Some(company_id) = company_id {
                    client.for_company(company_id).await?.context().await
                } else {
                    client.context().await
                };
                to_value(session)
            }
        }
    }
}
