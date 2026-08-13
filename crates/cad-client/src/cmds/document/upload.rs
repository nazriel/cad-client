use cad_api::domains::documents::DocumentUploadRequest;
use cad_api::ids::CompanyId;
use clap::Args;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::args::LocalInputPath;
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;
use crate::output::to_value;
use crate::support::terminal::cancelled;

#[derive(Debug, Args)]
pub(crate) struct Options {
    /// Local file to stream to object storage.
    #[arg(long)]
    pub(crate) file: LocalInputPath,

    /// Optional prepare-upload metadata JSON. Filename and company are injected.
    #[arg(long)]
    pub(crate) input: Option<LocalInputPath>,
}

pub(super) async fn run(
    options: Options,
    context: &CommandContext<'_>,
    port: &impl DocumentsPort,
    company_id: CompanyId,
) -> Result<Value, CliError> {
    let file_name = options
        .file
        .as_path()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CliError::Input("upload path has no valid UTF-8 file name".to_owned()))?;
    let mut input = if let Some(path) = options.input {
        context.read_json::<DocumentUploadRequest>(&path)?
    } else {
        DocumentUploadRequest::new(file_name)
    };
    input.file_name = file_name.to_owned();
    input.company_id = Some(company_id);

    let descriptor = context
        .during(&format!("uploading {file_name}"), async {
            tokio::select! {
                result = port.upload_document(&input, options.file.as_path()) => result,
                () = cancelled() => Err(CliError::Cancelled),
            }
        })
        .await?;
    to_value(serde_json::json!({
        "id": descriptor.id,
        "file": options.file.as_path(),
    }))
}
