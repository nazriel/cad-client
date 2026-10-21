//! The login step of `cad-cli use profile`.

use cad_api::{ApiKeyV2Credentials, Client, Connected, ServerConfig};
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use uuid::Uuid;

use super::{api_keys_page, environment_decides};
use crate::app::context::CommandContext;
use crate::config::Environment;
use crate::error::CliError;
use crate::support::credentials::{
    CredentialStore, Location, StoreError, StoredCredential, origin_of,
};
use crate::support::profiles::{Login, ProfileName};
use crate::support::terminal::{StatusKind, Terminal};

/// How the key is asked for and where it goes.
#[derive(Debug, Clone, Copy)]
pub(super) struct Options {
    pub(super) no_browser: bool,
    pub(super) with_token: bool,
    pub(super) insecure_storage: bool,
}

/// A login that was made and stored, and the lines to show about it.
pub(super) struct Outcome {
    pub(super) login: Login,
    pub(super) messages: Vec<(StatusKind, String)>,
    /// The session the login was checked with, for whatever else the login
    /// needs to ask.
    pub(super) client: Client<Connected>,
}

/// Logs `profile` in to `server`: asks for the key, checks it with the server,
/// stores it, then reads the account's e-mail.
///
/// Nothing is stored until the server has accepted the key. Failing to read
/// the e-mail afterwards is a warning, never a failed login.
pub(super) async fn run(
    options: Options,
    context: &CommandContext<'_>,
    profile: &ProfileName,
    server: ServerConfig,
    environment: &impl Environment,
    store: &CredentialStore,
) -> Result<Outcome, CliError> {
    let terminal = context.terminal();
    let origin = origin_of(&server);

    let page = api_keys_page(&origin);
    terminal.status(StatusKind::Info, &format!("API keys page: {page}"))?;
    if !options.no_browser && !options.with_token {
        terminal.open_url(&page);
    }

    let (key_id, secret) = if options.with_token {
        read_token(terminal)?
    } else if terminal.is_interactive() {
        ask_for_key(terminal)?
    } else {
        return Err(CliError::Input(
            "no terminal to ask for the key on; pass <Key ID>.<Secret> on standard input with \
             --with-token"
                .to_owned(),
        ));
    };

    // Nothing is stored until the server has accepted the key.
    let credentials = ApiKeyV2Credentials::from_secret(key_id, secret.clone());
    let client = connect(context, server, credentials).await?;
    let user_id = client.context().principal.user_id;

    let credential = StoredCredential::api_key(origin.clone(), key_id, secret, user_id, Utc::now());
    let location = save(
        terminal,
        store,
        profile,
        &credential,
        options.insecure_storage,
    )?;

    let mut messages = Vec::new();
    let email = match context
        .during("reading the account", client.users().get(user_id))
        .await
    {
        Ok(user) => user.email,
        Err(error) => {
            messages.push((
                StatusKind::Warning,
                format!("could not read the e-mail of the account ({error}); the login is stored"),
            ));
            None
        }
    };
    let login = Login {
        key_id,
        user_id,
        email,
        storage: location.place(),
        stored_at: credential.stored_at(),
    };
    let stored_in = location.path().map_or_else(
        || "the system keyring".to_owned(),
        |path| path.display().to_string(),
    );
    messages.insert(
        0,
        (
            StatusKind::Success,
            format!(
                "logged in to {origin} as {}, stored in {stored_in}",
                login.account()
            ),
        ),
    );
    if environment_decides(environment) {
        messages.push((
            StatusKind::Warning,
            "CAD_API_KEY or CAD_API_SECRET is set, and takes precedence over this login; unset \
             them to use it"
                .to_owned(),
        ));
    }
    Ok(Outcome {
        login,
        messages,
        client,
    })
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

/// `<Key ID>.<Secret>` from standard input.
fn read_token(terminal: &dyn Terminal) -> Result<(Uuid, SecretString), CliError> {
    let bytes = terminal.read_stdin()?;
    let text = String::from_utf8(bytes)
        .map_err(|_| CliError::Input("standard input is not text".to_owned()))?;
    let token = SecretString::from(text);
    ApiKeyV2Credentials::parse_token(token.expose_secret()).map_err(input_error)
}

/// The two values the API keys page shows, asked for separately: the Key ID
/// with the typed text shown, the Secret masked. Each is asked again, with the
/// reason, until it is well formed.
fn ask_for_key(terminal: &dyn Terminal) -> Result<(Uuid, SecretString), CliError> {
    let key_id = terminal.prompt("Key ID:", |text| {
        ApiKeyV2Credentials::parse_key_id(text)
            .map(|_| ())
            .map_err(reason)
    })?;
    let secret = terminal.prompt_secret("Secret:", |text| {
        ApiKeyV2Credentials::parse_secret(text)
            .map(|_| ())
            .map_err(reason)
    })?;
    Ok((
        ApiKeyV2Credentials::parse_key_id(&key_id).map_err(input_error)?,
        ApiKeyV2Credentials::parse_secret(secret.expose_secret()).map_err(input_error)?,
    ))
}

/// Why a typed value was rejected, without the error's own prefix.
fn reason(error: cad_api::Error) -> String {
    match error {
        cad_api::Error::InvalidInput { message } => message,
        other => other.to_string(),
    }
}

fn input_error(error: cad_api::Error) -> CliError {
    match error {
        cad_api::Error::InvalidInput { message } => CliError::Input(message),
        other => CliError::from(other),
    }
}

/// Keyring first. When it cannot be used, only a person at a terminal saying
/// yes, or `--insecure-storage`, writes the secret to the file; `--yes` does
/// not answer this question.
fn save(
    terminal: &dyn Terminal,
    store: &CredentialStore,
    profile: &ProfileName,
    credential: &StoredCredential,
    insecure_storage: bool,
) -> Result<Location, CliError> {
    if !insecure_storage {
        match store.save_in_keyring(profile, credential) {
            Ok(()) => return Ok(Location::Keyring),
            Err(StoreError::Failed(error)) => return Err(error),
            Err(StoreError::Unavailable(reason)) => {
                let path = store.file_path().ok_or_else(|| {
                    CliError::auth_config(format!(
                        "cannot use the system keyring: {reason}; and this platform has no \
                         per-user configuration directory to keep a file in. Nothing was stored; \
                         set CAD_API_KEY and CAD_API_SECRET instead"
                    ))
                })?;
                if !terminal.is_interactive() {
                    return Err(CliError::auth_config(format!(
                        "cannot use the system keyring: {reason}. Nothing was stored. Pass \
                         --insecure-storage to keep the key in {}, readable only by you, or set \
                         CAD_API_KEY and CAD_API_SECRET instead",
                        path.display()
                    )));
                }
                terminal.status(
                    StatusKind::Warning,
                    &format!("cannot use the system keyring: {reason}"),
                )?;
                let question = format!(
                    "Store the credentials in {} instead, readable only by you?",
                    path.display()
                );
                if !terminal.ask(&question)? {
                    return Err(CliError::auth_config(
                        "nothing was stored; set CAD_API_KEY and CAD_API_SECRET instead",
                    ));
                }
            }
        }
    }
    store.save_in_file(profile, credential).map(Location::File)
}
