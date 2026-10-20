mod download;
mod upload;

#[cfg(test)]
mod tests;

use cad_api::ids::CompanyId;
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

const UPLOAD_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli document upload --file invoice.pdf",
        "Upload a file and print the new document ID",
    ),
    Example::new(
        "cad-cli document upload --file invoice.pdf --input metadata.json",
        "Upload a file with extra document metadata",
    ),
];

const DOWNLOAD_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli document download 00000000-0000-0000-0000-000000000001 --out invoice.pdf",
        "Download a document to a new file",
    ),
    Example::new(
        "cad-cli document download 00000000-0000-0000-0000-000000000001 --out invoice.pdf --force",
        "Replace the file if it already exists",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Stream a local file to CAD object storage.
    #[command(after_long_help = help::examples(UPLOAD_EXAMPLES))]
    Upload(upload::Options),
    /// Download a document using a presigned storage URL.
    #[command(after_long_help = help::examples(DOWNLOAD_EXAMPLES))]
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
