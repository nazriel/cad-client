mod download;
mod upload;

#[cfg(test)]
mod tests;

use cad_api::ids::CompanyId;
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Stream a local file to CAD object storage.
    Upload(upload::Options),
    /// Download a document using a presigned storage URL.
    Download(download::Options),
}

impl Group {
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        port: &impl DocumentsPort,
        company_id: CompanyId,
    ) -> Result<Value, CliError> {
        match self.command {
            Command::Upload(options) => upload::run(options, context, port, company_id).await,
            Command::Download(options) => download::run(options, context, port).await,
        }
    }
}
