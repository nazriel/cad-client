use cad_api::ids::CompanyId;
use cad_api::{Client, Connected};
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

const SHOW_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli context show",
        "Show the machine identity and the company it acts on",
    ),
    Example::new(
        "cad-cli --company-id 00000000-0000-0000-0000-000000000001 context show",
        "Check that another company is accessible before using it",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Show the authenticated machine identity and company selection.
    #[command(after_help = help::examples(SHOW_EXAMPLES))]
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
                    client.for_company(company_id).await?.context()
                } else {
                    client.context()
                };
                to_value(session)
            }
        }
    }
}
