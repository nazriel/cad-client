//! `cad-cli auth status`.

use cad_api::ids::CompanyId;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::{Report, connect};
use crate::app::context::CommandContext;
use crate::cli::GlobalArgs;
use crate::config::{Environment, resolve_timed_server};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{self, CredentialStore, Source};
use crate::support::terminal::StatusKind;

/// The command's stdout value.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusResult {
    origin: String,
    /// `environment`, `keyring` or `file`.
    source: &'static str,
    /// The credentials file, when that is the source.
    path: Option<String>,
    key_id: Uuid,
    user_id: Uuid,
    affiliated_company_id: CompanyId,
    resolved_company_id: Option<CompanyId>,
    /// When the login was stored; `null` for the environment.
    stored_at: Option<DateTime<Utc>>,
}

pub(super) async fn run(
    context: &CommandContext<'_>,
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
) -> Result<Report, CliError> {
    let server = resolve_timed_server(global, environment)?;
    let origin = credentials::origin_of(&server);
    let resolved = credentials::resolve(environment, store, &origin)?;
    let source = resolved.source().clone();
    let key_id = resolved.key_id();
    let stored_at = resolved.stored_at();

    let session = connect(context, server, resolved.into_credentials()?)
        .await?
        .context();
    let result = StatusResult {
        origin: origin.clone(),
        source: source.name(),
        path: source.path().map(|path| path.display().to_string()),
        key_id,
        user_id: session.principal.user_id,
        affiliated_company_id: session.principal.affiliated_company_id,
        resolved_company_id: session.company_selection.resolved_company_id,
        stored_at,
    };
    let from = match &source {
        Source::Environment => "CAD_API_KEY and CAD_API_SECRET".to_owned(),
        Source::Keyring => "the system keyring".to_owned(),
        Source::File(path) => path.display().to_string(),
    };
    let stored = stored_at.map_or_else(String::new, |time| {
        format!(
            ", stored {}",
            time.to_rfc3339_opts(SecondsFormat::Secs, true)
        )
    });
    Ok(Report {
        value: to_value(&result)?,
        messages: vec![(
            StatusKind::Success,
            format!(
                "{origin}: logged in as {} with key {key_id}, from {from}{stored}",
                session.principal.user_id
            ),
        )],
    })
}
