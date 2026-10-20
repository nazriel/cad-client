use std::path::{Path, PathBuf};

use cad_api::ids::DocumentId;
use clap::{Args, ValueHint};
use serde_json::Value;
use tempfile::NamedTempFile;

use crate::app::context::CommandContext;
use crate::error::CliError;
use crate::exec::documents::DocumentsPort;
use crate::output::to_value;
use crate::support::terminal::cancelled;

#[derive(Debug, Args)]
pub(crate) struct Options {
    /// Identifier of the document.
    pub(crate) id: DocumentId,

    /// File to write the document to.
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub(crate) out: PathBuf,

    /// Replace the --out file if it already exists.
    #[arg(long)]
    pub(crate) force: bool,
}

pub(super) async fn run(
    options: Options,
    context: &CommandContext<'_>,
    port: &impl DocumentsPort,
) -> Result<Value, CliError> {
    ensure_target(&options.out, options.force)?;
    let temporary = NamedTempFile::new_in(destination_parent(&options.out))?.into_temp_path();
    // Ctrl-C must unwind rather than kill the process, or the temporary sibling
    // file is left next to the destination instead of being removed on drop.
    // Unwinding also drops the progress indicator, which clears its own line.
    let descriptor = context
        .during(&format!("downloading document {}", options.id), async {
            tokio::select! {
                result = port.download_document(options.id, temporary.as_ref()) => result,
                () = cancelled() => Err(CliError::Cancelled),
            }
        })
        .await?;
    if options.force {
        temporary
            .persist(&options.out)
            .map_err(|error| CliError::Io(error.error))?;
    } else {
        // The check above only buys a readable early error. The guarantee is
        // here: a no-clobber rename cannot be beaten by a file that appears
        // between the check and the write.
        temporary
            .persist_noclobber(&options.out)
            .map_err(|error| match error.error.kind() {
                std::io::ErrorKind::AlreadyExists => target_exists(&options.out),
                _ => CliError::Io(error.error),
            })?;
    }

    to_value(serde_json::json!({
        "id": descriptor.id,
        "path": options.out,
    }))
}

fn ensure_target(path: &Path, force: bool) -> Result<(), CliError> {
    if path.exists() && !force {
        return Err(target_exists(path));
    }
    Ok(())
}

fn target_exists(path: &Path) -> CliError {
    CliError::Input(format!(
        "download target {} already exists; pass --force to replace it",
        path.display()
    ))
}

fn destination_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}
