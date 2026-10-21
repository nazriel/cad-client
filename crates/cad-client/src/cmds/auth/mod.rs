//! `cad-cli auth login | status | logout`: sign in to a server with an API
//! key and keep it for the commands that follow.
//!
//! The commands act on the selected server only. They select it exactly as
//! every other command does and need one: there is no default host. They are
//! not platform commands: `login` must not look for stored credentials before
//! it has any, and `logout` never connects. Each returns a small JSON object
//! in the selected output format; prompts and the outcome go to stderr.

mod login;
mod logout;
mod status;
#[cfg(test)]
mod tests;

use cad_api::{ApiKeyV2Credentials, Client, Connected, ServerConfig};
use clap::{Args, Subcommand};
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::cli::GlobalArgs;
use crate::cli::help::{self, Example};
use crate::config::Environment;
use crate::error::CliError;
use crate::support::credentials::CredentialStore;
use crate::support::terminal::StatusKind;

const LOGIN_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli --environment production auth login",
        "Open the API keys page, then enter the Key ID and the Secret it shows",
    ),
    Example::new(
        "cad-cli --server-url http://localhost:5001 --allow-insecure-http auth login --no-browser",
        "Log in to a local server without opening a browser",
    ),
    Example::new(
        "printf '%s.%s' \"$KEY_ID\" \"$SECRET\" | cad-cli --environment staging auth login --with-token",
        "Log in without a terminal, from the Key ID and Secret on standard input",
    ),
    Example::new(
        "cad-cli --environment staging auth login --insecure-storage",
        "Keep the key in a private file instead of the system keyring",
    ),
];

const STATUS_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli --environment production auth status",
        "Show which credentials commands use for this server, and check them",
    ),
    Example::new(
        "cad-cli --environment production auth status --output json",
        "The same, as JSON",
    ),
];

const LOGOUT_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli --environment production auth logout",
    "Forget the key stored for this server",
)];

/// Arguments of the `auth` group.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Log in to the selected server with an API key and store it.
    ///
    /// The key is created on the server's API keys page, which this command
    /// prints and opens. The page shows the Key ID and the Secret as two
    /// values: enter them when asked. The key is checked with the server
    /// before anything is saved, and it is saved for that server only.
    ///
    /// It is kept in the system keyring (macOS Keychain, Windows Credential
    /// Manager, or the Secret Service on Linux). When the keyring cannot be
    /// used, you are asked whether to keep it in a file only you can read
    /// instead; `--insecure-storage` answers yes in advance.
    #[command(after_help = help::examples(LOGIN_EXAMPLES))]
    Login(LoginArgs),
    /// Show the credentials commands use for the selected server, and check them.
    ///
    /// `CAD_API_KEY` and `CAD_API_SECRET` take precedence over a stored login.
    #[command(after_help = help::examples(STATUS_EXAMPLES))]
    Status,
    /// Forget the key stored for the selected server.
    ///
    /// The key itself stays valid until it is revoked on the API keys page.
    #[command(after_help = help::examples(LOGOUT_EXAMPLES))]
    Logout,
}

/// Arguments of `cad-cli auth login`.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
    /// Print the API keys page instead of opening it in a browser.
    #[arg(long)]
    pub(crate) no_browser: bool,

    /// Read `<Key ID>.<Secret>` from standard input instead of asking for the
    /// two values. Nothing is asked and no browser is opened.
    #[arg(long)]
    pub(crate) with_token: bool,

    /// Write the key to the credentials file without trying the system keyring.
    /// The file is readable only by you.
    #[arg(long)]
    pub(crate) insecure_storage: bool,
}

/// What a command produced: data for stdout and the lines to show on stderr
/// after it, in order.
#[derive(Debug)]
pub(crate) struct Report {
    pub(crate) value: Value,
    pub(crate) messages: Vec<(StatusKind, String)>,
}

impl Group {
    /// Runs the selected command.
    ///
    /// # Errors
    ///
    /// Fails when no server is selected, when the credentials are missing,
    /// malformed or rejected, or when they cannot be stored or removed.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        global: &GlobalArgs,
        environment: &impl Environment,
        store: &CredentialStore,
    ) -> Result<Report, CliError> {
        match self.command {
            Command::Login(args) => login::run(args, context, global, environment, store).await,
            Command::Status => status::run(context, global, environment, store).await,
            Command::Logout => logout::run(global, environment, store),
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

/// Authenticates against `server`, so the identity request runs.
async fn connect(
    context: &CommandContext<'_>,
    server: ServerConfig,
    credentials: ApiKeyV2Credentials,
) -> Result<Client<Connected>, CliError> {
    let connected = context
        .during("checking the API key with the server", async {
            Client::new(server)?.connect(credentials).await
        })
        .await?;
    Ok(connected)
}
