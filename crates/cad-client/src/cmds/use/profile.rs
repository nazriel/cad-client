//! `cad-cli use profile <name>`.

use cad_api::ServerConfig;
use serde::Serialize;
use strum::EnumString;

use super::login::{self, Options};
use super::{ProfileArgs, ProfileRow, Report, company, forget, list, no_company};
use crate::app::context::CommandContext;
use crate::cli::GlobalArgs;
use crate::config::{Environment, build_server, origin_of_raw, select_profile};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{CredentialStore, origin_of};
use crate::support::profiles::{Profile, ProfileName, ProfileStore};
use crate::support::terminal::{StatusKind, Terminal};

/// The profile names that suggest a server when the profile is created. It is
/// a convenience of the prompt, not a built-in deployment: any other name gets
/// no suggestion.
#[derive(Debug, Clone, Copy, EnumString)]
#[strum(serialize_all = "lowercase")]
enum KnownProfile {
    Production,
    Staging,
    Dev,
}

impl KnownProfile {
    const fn url(self) -> &'static str {
        match self {
            Self::Production => "https://app.carbonaltdelete.eu",
            Self::Staging => "https://staging.carbonaltdelete.eu",
            Self::Dev => "https://dev.carbonaltdelete.eu",
        }
    }
}

pub(super) fn suggested_url(name: &ProfileName) -> Option<&'static str> {
    name.as_str().parse().ok().map(KnownProfile::url)
}

/// The command's stdout value: the profile's row, and the profile that was
/// active before.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Switched {
    #[serde(flatten)]
    profile: ProfileRow,
    previous: Option<ProfileName>,
}

pub(super) async fn run(
    args: ProfileArgs,
    context: &CommandContext<'_>,
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
    profiles: &ProfileStore,
) -> Result<Report, CliError> {
    if global.profile.is_some() {
        return Err(CliError::Input(
            "--profile does not apply to `cad-cli use profile`; name the profile as its argument"
                .to_owned(),
        ));
    }
    if global.server_url.is_some() {
        return Err(CliError::Input(
            "--server-url does not apply to `cad-cli use profile`; use --url".to_owned(),
        ));
    }
    if args.forget {
        return forget::run(args.name, environment, store, profiles);
    }

    let terminal = context.terminal();
    let mut set = profiles.load()?;
    let previous = set.active().cloned();
    let name = args.name.clone();

    let mut profile = match set.get(&name) {
        Some(existing) => existing_profile(existing.clone(), &name, &args, global)?,
        None => new_profile(&name, &args, global, terminal)?,
    };
    let origin = profile.url.clone();
    let server = build_server(
        &origin,
        global.allow_insecure_http || profile.allow_insecure_http,
    )?
    .with_request_timeout(global.request_timeout)
    .map_err(CliError::from)?;

    let mut messages = Vec::new();
    let log_in = if args.login || args.with_token {
        true
    } else if profile.login.is_some() {
        false
    } else if terminal.is_interactive() {
        let answer = terminal.ask(&format!(
            "No login for profile {name} ({origin}). Log in now?"
        ))?;
        if !answer {
            messages.push((StatusKind::Warning, no_login(&name, "--login")));
        }
        answer
    } else {
        messages.push((StatusKind::Warning, no_login(&name, "--with-token")));
        false
    };
    if log_in {
        let options = Options {
            no_browser: args.no_browser,
            with_token: args.with_token,
            insecure_storage: args.insecure_storage,
        };
        let session = Session {
            context,
            name: &name,
            server,
            environment,
            store,
        };
        messages.extend(log_in_and_choose_company(options, session, &mut profile).await?);
    } else if profile.login.is_some() && profile.company.is_none() {
        messages.push((StatusKind::Warning, no_company()));
    }

    set.insert(name.clone(), profile);
    set.activate(name.clone());
    profiles.save(&set)?;

    // `CAD_PROFILE` (or a server in the environment) can still win in this
    // shell, so whether the profile is selected is asked of the selection.
    let selected = select_profile(global, environment, &set)
        .ok()
        .flatten()
        .map(|selected| selected.name);
    let row = list::rows(&set, selected.as_ref())
        .into_iter()
        .find(|row| row.name == name)
        .expect("the profile was just saved");
    messages.extend(now_using(&row, &origin, previous.as_ref(), environment));

    let value = to_value(Switched {
        profile: row,
        previous,
    })?;
    Ok(Report { value, messages })
}

/// The lines that say which profile is in use now, and whether `CAD_PROFILE`
/// overrides it in this shell.
fn now_using(
    row: &ProfileRow,
    origin: &str,
    previous: Option<&ProfileName>,
    environment: &impl Environment,
) -> Vec<(StatusKind, String)> {
    let name = &row.name;
    let who = row.account().map_or_else(
        || "with no login".to_owned(),
        |account| format!("as {account}"),
    );
    let was = previous
        .filter(|previous| *previous != name)
        .map(|previous| format!(" (was {previous})"))
        .unwrap_or_default();
    let mut messages = vec![(
        StatusKind::Success,
        format!("now using {name} ({origin}) {who}{was}"),
    )];
    if let Some(variable) = environment.get("CAD_PROFILE") {
        messages.push((
            StatusKind::Warning,
            format!(
                "CAD_PROFILE is set to {variable} and overrides the active profile in this shell"
            ),
        ));
    }
    messages
}

/// What the login step needs besides the profile it changes.
struct Session<'a, E> {
    context: &'a CommandContext<'a>,
    name: &'a ProfileName,
    server: ServerConfig,
    environment: &'a E,
    store: &'a CredentialStore,
}

/// Logs `profile` in, then settles its company from the session the login
/// made. A company belongs to the account it was chosen for, so a login by
/// another user than the profile's last clears it first.
async fn log_in_and_choose_company(
    options: Options,
    session: Session<'_, impl Environment>,
    profile: &mut Profile,
) -> Result<Vec<(StatusKind, String)>, CliError> {
    let Session {
        context,
        name,
        server,
        environment,
        store,
    } = session;
    let outcome = login::run(options, context, name, server, environment, store).await?;
    let mut messages = outcome.messages;
    if profile
        .login
        .as_ref()
        .is_some_and(|before| before.user_id != outcome.login.user_id)
        && profile.company.take().is_some()
    {
        messages.push((
            StatusKind::Info,
            format!("the login is for another account; the company of profile {name} was cleared"),
        ));
    }
    profile.login = Some(outcome.login);
    if profile.company.is_none() {
        let offer = company::offer(&outcome.client, context, name).await?;
        profile.company = offer.company;
        messages.extend(offer.messages);
    }
    Ok(messages)
}

fn no_login(name: &ProfileName, flag: &str) -> String {
    format!(
        "profile {name} has no login: commands need `cad-cli use profile {name} {flag}` or \
         CAD_API_KEY and CAD_API_SECRET"
    )
}

/// A profile that exists, as the arguments change it: a server of another
/// origin is refused, and `--allow-insecure-http` is remembered.
fn existing_profile(
    mut profile: Profile,
    name: &ProfileName,
    args: &ProfileArgs,
    global: &GlobalArgs,
) -> Result<Profile, CliError> {
    if let Some(raw) = &args.url
        && origin_of_raw(raw)? != profile.url
    {
        return Err(CliError::Input(format!(
            "profile {name} is for {}; forget it first with `cad-cli use profile {name} \
             --forget`, or choose another name",
            profile.url
        )));
    }
    profile.allow_insecure_http |= global.allow_insecure_http;
    Ok(profile)
}

/// A profile that does not exist yet. Its server is `--url`, or asked for at
/// a terminal, or the suggestion for the names that have one.
fn new_profile(
    name: &ProfileName,
    args: &ProfileArgs,
    global: &GlobalArgs,
    terminal: &dyn Terminal,
) -> Result<Profile, CliError> {
    let raw = match &args.url {
        Some(raw) => raw.clone(),
        None => ask_for_url(name, terminal)?,
    };
    let server = build_server(&raw, global.allow_insecure_http)?;
    Ok(Profile {
        url: origin_of(&server),
        allow_insecure_http: global.allow_insecure_http,
        login: None,
        company: None,
    })
}

fn ask_for_url(name: &ProfileName, terminal: &dyn Terminal) -> Result<String, CliError> {
    let suggestion = suggested_url(name);
    if terminal.is_interactive() {
        let Some(suggestion) = suggestion else {
            return terminal
                .prompt("Server URL:", |text| valid_url(text.trim()))
                .map(|text| text.trim().to_owned());
        };
        let typed = terminal.prompt(&format!("Server URL (Enter for {suggestion}):"), |text| {
            if text.trim().is_empty() {
                Ok(())
            } else {
                valid_url(text.trim())
            }
        })?;
        return Ok(if typed.trim().is_empty() {
            suggestion.to_owned()
        } else {
            typed.trim().to_owned()
        });
    }
    let suggestion = suggestion.ok_or_else(|| {
        CliError::Input(format!(
            "profile {name} is new and there is no terminal to ask for its server on; pass \
             --url <URL>"
        ))
    })?;
    terminal.status(
        StatusKind::Info,
        &format!("using {suggestion} for profile {name}"),
    )?;
    Ok(suggestion.to_owned())
}

/// Whether `text` is a server address. Whether plain HTTP is allowed is
/// decided afterwards, with the flag.
fn valid_url(text: &str) -> Result<(), String> {
    origin_of_raw(text).map(drop).map_err(|error| match error {
        CliError::Config { message, .. } => message,
        other => other.to_string(),
    })
}
