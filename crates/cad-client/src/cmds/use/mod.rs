//! `cad-cli use`: the profiles, which one is active, and logging in to them.
//!
//! A profile is a name for a server and the login made there. `use` lists the
//! profiles; `use profile <name>` creates one, switches to it, logs in to it
//! and forgets it. The command is deliberately outside the singular-noun
//! convention of the platform commands: it is not one of them. It is not a
//! platform command in the other sense either: `use profile` must not look for
//! stored credentials before it has any, and neither form needs a server up.
//! Each returns a small JSON value in the selected output format; prompts and
//! the outcome go to stderr.

mod forget;
mod list;
mod login;
mod profile;
#[cfg(test)]
mod tests;

use cad_api::ids::UserId;
use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::app::context::CommandContext;
use crate::cli::GlobalArgs;
use crate::cli::help::{self, Example};
use crate::config::Environment;
use crate::error::CliError;
use crate::support::credentials::{CredentialStore, Place};
use crate::support::profiles::{Profile, ProfileName, ProfileStore};
use crate::support::terminal::StatusKind;

const USE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli use",
        "List the profiles, the active one, and the one commands use now",
    ),
    Example::new("cad-cli use --output json", "The same, as JSON"),
];

const PROFILE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli use profile production",
        "Switch to a profile; a new one asks for its server and offers to log in",
    ),
    Example::new(
        "cad-cli use profile staging --login",
        "Log in again, opening the API keys page and asking for the Key ID and the Secret",
    ),
    Example::new(
        "printf '%s.%s' \"$KEY_ID\" \"$SECRET\" | cad-cli use profile staging --with-token",
        "Log in without a terminal, from the Key ID and Secret on standard input",
    ),
    Example::new(
        "cad-cli use profile local --url http://localhost:5001 --allow-insecure-http --no-browser",
        "Create a profile for a local server, logging in without opening a browser",
    ),
    Example::new(
        "cad-cli use profile staging --login --insecure-storage",
        "Keep the key in a private file instead of the system keyring",
    ),
    Example::new(
        "cad-cli use profile staging --forget",
        "Forget the profile and the key stored for it",
    ),
];

/// Arguments of the `use` command.
#[derive(Debug, Args)]
#[command(after_help = help::examples(USE_EXAMPLES))]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create a profile, switch to it, log in to it, or forget it.
    ///
    /// A profile is a name for a server and the API key you log in with there.
    /// Naming a profile that does not exist yet creates it: its server is
    /// `--url`, or is asked for (production, staging and dev suggest the
    /// address of their server). Naming one that exists makes it the active
    /// profile, which commands use unless `--profile` or `CAD_PROFILE` names
    /// another.
    ///
    /// A profile with no login is offered one. The API keys page of the server
    /// is printed and opened; it shows the Key ID and the Secret as two
    /// values: enter them when asked. The key is checked with the server
    /// before anything is saved, and it is saved for that server only.
    ///
    /// It is kept in the system keyring (macOS Keychain, Windows Credential
    /// Manager, or the Secret Service on Linux). When the keyring cannot be
    /// used, you are asked whether to keep it in a file only you can read
    /// instead; `--insecure-storage` answers yes in advance.
    ///
    /// `CAD_API_KEY` and `CAD_API_SECRET` take precedence over a stored login.
    #[command(after_help = help::examples(PROFILE_EXAMPLES))]
    Profile(ProfileArgs),
}

/// Arguments of `cad-cli use profile`.
// A bundle of independent command-line switches, not a state to be modelled.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
pub(crate) struct ProfileArgs {
    /// Name of the profile: 1 to 32 characters of a-z, 0-9, - and _.
    #[arg(add = crate::completion::profile_names())]
    pub(crate) name: ProfileName,

    /// Address of the profile's server, for a profile that does not exist yet.
    #[arg(long, value_hint = clap::ValueHint::Url)]
    pub(crate) url: Option<String>,

    /// Log in, even if the profile has a login already.
    #[arg(long)]
    pub(crate) login: bool,

    /// Log in with `<Key ID>.<Secret>` read from standard input instead of
    /// asking for the two values. Nothing is asked and no browser is opened.
    #[arg(long)]
    pub(crate) with_token: bool,

    /// Print the API keys page instead of opening it in a browser.
    #[arg(long)]
    pub(crate) no_browser: bool,

    /// Write the key to the credentials file without trying the system keyring.
    /// The file is readable only by you.
    #[arg(long)]
    pub(crate) insecure_storage: bool,

    /// Forget the profile and the key stored for it. The key itself stays
    /// valid until it is revoked on the API keys page.
    #[arg(
        long,
        conflicts_with_all = [
            "url", "login", "with_token", "no_browser", "insecure_storage", "allow_insecure_http"
        ]
    )]
    pub(crate) forget: bool,
}

/// What a command produced: data for stdout and the lines to show on stderr
/// after it, in order.
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) value: Value,
    pub(crate) messages: Vec<(StatusKind, String)>,
}

/// One profile as `use` shows it: flat, so that every output format, a table
/// included, can carry it as it is. The login's fields are `null` for a
/// profile with no login.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfileRow {
    pub(crate) name: ProfileName,
    pub(crate) url: String,
    /// Whether it is the active profile.
    pub(crate) active: bool,
    /// Whether commands run now would use it.
    pub(crate) selected: bool,
    pub(crate) email: Option<String>,
    pub(crate) user_id: Option<UserId>,
    pub(crate) key_id: Option<Uuid>,
    pub(crate) storage: Option<Place>,
    pub(crate) stored_at: Option<DateTime<Utc>>,
}

impl ProfileRow {
    pub(crate) fn new(name: &ProfileName, profile: &Profile, active: bool, selected: bool) -> Self {
        let login = profile.login.as_ref();
        Self {
            name: name.clone(),
            url: profile.url.clone(),
            active,
            selected,
            email: login.and_then(|login| login.email.clone()),
            user_id: login.map(|login| login.user_id),
            key_id: login.map(|login| login.key_id),
            storage: login.map(|login| login.storage),
            stored_at: login.map(|login| login.stored_at),
        }
    }

    /// Who the profile is logged in as: the e-mail, or the user ID when the
    /// e-mail is not known; `None` without a login.
    pub(crate) fn account(&self) -> Option<String> {
        self.email
            .clone()
            .or_else(|| self.user_id.map(|user_id| user_id.to_string()))
    }
}

impl Group {
    /// Runs the selected command.
    ///
    /// # Errors
    ///
    /// Fails when the profiles file is unusable, when a flag does not apply,
    /// when the key is missing, malformed or rejected, or when it cannot be
    /// stored or removed.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        global: &GlobalArgs,
        environment: &impl Environment,
        store: &CredentialStore,
        profiles: &ProfileStore,
    ) -> Result<Report, CliError> {
        match self.command {
            None => list::run(global, environment, profiles),
            Some(Command::Profile(args)) => {
                profile::run(args, context, global, environment, store, profiles).await
            }
        }
    }
}

/// The page where API keys are created, on the server's own origin. The path
/// is exact: a path without a language prefix is redirected to the user's.
fn api_keys_page(origin: &str) -> String {
    format!("{origin}/app/user-account-settings/api-keys")
}

/// Whether `CAD_API_KEY` or `CAD_API_SECRET` is set, and so decides alone
/// which credentials commands use.
fn environment_decides(environment: &impl Environment) -> bool {
    environment.get("CAD_API_KEY").is_some() || environment.get("CAD_API_SECRET").is_some()
}
