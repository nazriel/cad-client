//! `cad-cli use company <name|UUID>`, and the choice offered after a login.

use std::str::FromStr;

use cad_api::domains::companies::{Company as PlatformCompany, CompanyStatus};
use cad_api::ids::CompanyId;
use cad_api::{Client, Connected};
use serde::Serialize;

use super::{CompanyArgs, Report, no_company};
use crate::app::context::CommandContext;
use crate::app::platform::Platform;
use crate::cli::GlobalArgs;
use crate::config::{AppConfig, Environment, resolve_credentials, select, select_profile};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{CredentialStore, origin_of};
use crate::support::profiles::{Company, Login, ProfileName, ProfileStore, company_label};
use crate::support::terminal::StatusKind;

/// The argument of `cad-cli use company`: a company's ID, or its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompanyChoice {
    Id(CompanyId),
    Name(String),
}

impl FromStr for CompanyChoice {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err("name the company by its name or its ID".to_owned());
        }
        Ok(value
            .parse::<CompanyId>()
            .map_or_else(|_| Self::Name(value.to_owned()), Self::Id))
    }
}

/// The command's stdout value.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Chosen {
    profile: ProfileName,
    company_id: CompanyId,
    company_name: Option<String>,
    /// The company the profile had before, if there was one.
    previous_company_id: Option<CompanyId>,
    previous_company_name: Option<String>,
}

/// The companies a person can choose: the ones that are not deleted.
fn choosable(companies: &[PlatformCompany]) -> impl Iterator<Item = &PlatformCompany> {
    companies
        .iter()
        .filter(|company| company.status != CompanyStatus::Deleted)
}

/// Finds the company `choice` names among `companies`.
///
/// An ID selects by ID, even one that is not in the list: whether it can be
/// used is for the platform to say. A name is matched exactly against the
/// names, trimmed and ignoring case.
///
/// # Errors
///
/// [`CliError::Input`] when no company has that name, or several do; the
/// latter lists them with their IDs.
fn find(choice: &CompanyChoice, companies: &[PlatformCompany]) -> Result<Company, CliError> {
    let wanted = match choice {
        CompanyChoice::Id(id) => {
            let name = companies
                .iter()
                .find(|company| company.id == *id)
                .and_then(company_label);
            return Ok(Company { id: *id, name });
        }
        CompanyChoice::Name(name) => name,
    };
    let wanted_lowercase = wanted.to_lowercase();
    let mut found: Vec<_> = choosable(companies)
        .filter(|company| {
            company_label(company).is_some_and(|name| name.to_lowercase() == wanted_lowercase)
        })
        .collect();
    found.sort_by_key(|company| company.id);
    match found.as_slice() {
        [] => Err(CliError::Input(format!(
            "no accessible company is named \"{wanted}\"; list them with `cad-cli company list`"
        ))),
        [company] => Ok(Company {
            id: company.id,
            name: company_label(company),
        }),
        several => {
            let candidates = several
                .iter()
                .map(|company| {
                    format!(
                        "{} {}",
                        company.id,
                        company_label(company).unwrap_or_else(|| "-".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Err(CliError::Input(format!(
                "several accessible companies are named \"{wanted}\"; use the ID of the one you \
                 mean:\n{candidates}"
            )))
        }
    }
}

/// Sets the active company of the profile in use, after checking that the key
/// can act on it.
pub(super) async fn run(
    args: CompanyArgs,
    context: &CommandContext<'_>,
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
    profiles: &ProfileStore,
) -> Result<Report, CliError> {
    if global.company_id.is_some() {
        return Err(CliError::Input(
            "--company-id does not apply to `cad-cli use company`; name the company as its \
             argument"
                .to_owned(),
        ));
    }
    let set = profiles.load()?;
    let Some(selected) = select_profile(global, environment, &set)? else {
        return Err(CliError::config_with_hint(
            "`cad-cli use company` needs a profile",
            "create one with `cad-cli use profile <name>`",
        ));
    };
    let name = selected.name;
    let selection = select(global, environment, &set)?;
    let origin = origin_of(&selection.server);
    let resolved = resolve_credentials(environment, store, &selection)?;
    // Connected like any command, but with no company: the one to use is what
    // is being chosen.
    let config = AppConfig::resolve_with(selection, resolved, None)?;
    let platform = context
        .during(
            "checking the API key with the server",
            Platform::connect(config),
        )
        .await?;
    let client = platform.connected();
    let companies = context
        .during("listing the companies", client.companies().list())
        .await?;
    let company = find(&args.company, &companies)?;
    context
        .during(
            "checking access to the company",
            client.for_company(company.id),
        )
        .await?;

    // Read again: the profiles may have changed while the server was asked.
    let mut set = profiles.load()?;
    let profile = set.get_mut(&name).ok_or_else(|| {
        CliError::config(format!(
            "profile {name} was removed while the company was being chosen"
        ))
    })?;
    let user_id = client.context().principal.user_id;
    let account = profile
        .login
        .as_ref()
        .filter(|login| login.user_id == user_id)
        .map_or_else(|| user_id.to_string(), Login::account);
    let previous = profile.company.replace(company.clone());
    profiles.save(&set)?;

    let was = previous
        .as_ref()
        .filter(|previous| previous.id != company.id)
        .map(|previous| format!(" (was {})", previous.label()))
        .unwrap_or_default();
    let mut messages = vec![(
        StatusKind::Success,
        format!(
            "Using company {} on {name} ({origin}) as {account}{was}",
            company.label()
        ),
    )];
    if let Some(variable) = environment.get("CAD_COMPANY_ID") {
        messages.push((
            StatusKind::Warning,
            format!(
                "CAD_COMPANY_ID is set to {variable} and overrides the company of the profile in \
                 this shell"
            ),
        ));
    }
    Ok(Report {
        value: to_value(Chosen {
            profile: name,
            company_id: company.id,
            company_name: company.name,
            previous_company_id: previous.as_ref().map(|previous| previous.id),
            previous_company_name: previous.and_then(|previous| previous.name),
        })?,
        messages,
    })
}

/// What was settled about a profile's company after its login.
pub(super) struct Offer {
    pub(super) company: Option<Company>,
    pub(super) messages: Vec<(StatusKind, String)>,
}

/// Settles the company of a profile that has none, from the session its login
/// made: the only company is taken; with several, a person at a terminal
/// chooses and anyone else is told how. It never fails the login: whatever
/// goes wrong leaves the company unset, with the way to set it.
///
/// # Errors
///
/// Only what a prompt reports other than the person giving up.
pub(super) async fn offer(
    client: &Client<Connected>,
    context: &CommandContext<'_>,
    profile: &ProfileName,
) -> Result<Offer, CliError> {
    let hint = (StatusKind::Warning, no_company());
    let companies = match context
        .during("listing the companies", client.companies().list())
        .await
    {
        Ok(companies) => companies,
        Err(error) => {
            return Ok(Offer {
                company: None,
                messages: vec![
                    (
                        StatusKind::Warning,
                        format!("could not list the companies ({error})"),
                    ),
                    hint,
                ],
            });
        }
    };
    let mut choosable: Vec<_> = choosable(&companies)
        .map(|company| Company {
            id: company.id,
            name: company_label(company),
        })
        .collect();
    choosable.sort_by_cached_key(|company| (company.label().to_lowercase(), company.id));

    let terminal = context.terminal();
    let company = match choosable.as_slice() {
        [] => None,
        [only] => Some(only.clone()),
        several if terminal.is_interactive() => {
            let options: Vec<_> = several
                .iter()
                .map(|company| match &company.name {
                    Some(name) => format!("{name} ({})", company.id),
                    None => company.id.to_string(),
                })
                .collect();
            match terminal.select(&format!("Company for profile {profile}:"), &options) {
                Ok(index) => several.get(index).cloned(),
                Err(CliError::Cancelled) => None,
                Err(error) => return Err(error),
            }
        }
        _ => None,
    };
    let messages = match &company {
        Some(company) => vec![(
            StatusKind::Success,
            format!("using company {} for profile {profile}", company.label()),
        )],
        None => vec![hint],
    };
    Ok(Offer { company, messages })
}
