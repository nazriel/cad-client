//! `cad-cli use --check`: the online check of what commands would use now.

use cad_api::CompanySelectionSource;
use cad_api::ids::{CompanyId, UserId};
use serde::Serialize;
use uuid::Uuid;

use super::{Report, no_company};
use crate::app::context::CommandContext;
use crate::app::platform::Platform;
use crate::cli::GlobalArgs;
use crate::config::{AppConfig, CompanySource, Environment};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{CredentialStore, Source, origin_of};
use crate::support::profiles::{ProfileName, ProfileStore};
use crate::support::terminal::StatusKind;

/// The command's stdout value: flat, so that every output format, a table
/// included, can carry it as it is. The company's fields are `null` without a
/// company; the last two are the server's own account of the company the
/// identity request resolved.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Checked {
    profile: Option<ProfileName>,
    url: String,
    credentials_source: Source,
    key_id: Uuid,
    user_id: UserId,
    /// Read fresh from the server; `null` when it could not be.
    email: Option<String>,
    company_id: Option<CompanyId>,
    company_name: Option<String>,
    company_source: Option<CompanySource>,
    resolved_company_id: Option<CompanyId>,
    resolved_company_source: CompanySelectionSource,
}

/// Resolves the profile, the server, the credentials and the company as a
/// command would, connects with them once, and reports what the server made
/// of it. A missing company is reported, not an error: only a command that
/// acts on one is refused.
pub(super) async fn run(
    context: &CommandContext<'_>,
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
    profiles: &ProfileStore,
) -> Result<Report, CliError> {
    let config = AppConfig::resolve(global, environment, store, &profiles.load()?)?;
    let profile = config.profile().cloned();
    let url = origin_of(config.server());
    let (credentials_source, key_id) = config.credentials_in_use();
    let company = config.company().cloned();

    let platform = context
        .during(
            "checking the API key with the server",
            Platform::connect(config),
        )
        .await?;
    let client = platform.connected();
    let session = client.context();
    let user_id = session.principal.user_id;

    let mut messages = Vec::new();
    let email = match context
        .during("reading the account", client.users().get(user_id))
        .await
    {
        Ok(user) => user.email,
        Err(error) => {
            messages.push((
                StatusKind::Warning,
                format!("could not read the e-mail of the account ({error})"),
            ));
            None
        }
    };

    let account = email.clone().unwrap_or_else(|| user_id.to_string());
    let on = profile
        .as_ref()
        .map_or_else(String::new, |profile| format!(" with profile {profile}"));
    messages.insert(
        0,
        (
            StatusKind::Success,
            format!("{url}: authenticated as {account}{on}"),
        ),
    );
    match &company {
        Some(company) => messages.push((
            StatusKind::Info,
            format!(
                "company {} ({})",
                company.name.as_deref().unwrap_or("name unknown"),
                company.id
            ),
        )),
        None => messages.push((StatusKind::Warning, no_company())),
    }

    let value = to_value(Checked {
        profile,
        url,
        credentials_source,
        key_id,
        user_id,
        email,
        company_id: company.as_ref().map(|company| company.id),
        company_name: company.as_ref().and_then(|company| company.name.clone()),
        company_source: company.as_ref().map(|company| company.source),
        resolved_company_id: session.company_selection.resolved_company_id,
        resolved_company_source: session.company_selection.source,
    })?;
    Ok(Report { value, messages })
}
