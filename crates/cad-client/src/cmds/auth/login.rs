//! `cad-cli auth login`.

use std::path::{Path, PathBuf};

use cad_api::ApiKeyV2Credentials;
use cad_api::ids::UserId;
use chrono::Utc;
use secrecy::{ExposeSecret, SecretString};
use serde::Serialize;
use uuid::Uuid;

use super::{LoginArgs, Report, api_keys_page, connect, environment_decides};
use crate::app::context::CommandContext;
use crate::cli::GlobalArgs;
use crate::config::{Environment, resolve_timed_server};
use crate::error::CliError;
use crate::output::to_value;
use crate::support::credentials::{
    CredentialStore, Location, Place, StoreError, StoredCredential, origin_of,
};
use crate::support::terminal::{StatusKind, Terminal};

/// The command's stdout value.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResult {
    origin: String,
    key_id: Uuid,
    user_id: UserId,
    affiliated_company_id: cad_api::ids::CompanyId,
    storage: Place,
    /// The credentials file, when that is where the key went.
    path: Option<PathBuf>,
}

pub(super) async fn run(
    args: LoginArgs,
    context: &CommandContext<'_>,
    global: &GlobalArgs,
    environment: &impl Environment,
    store: &CredentialStore,
) -> Result<Report, CliError> {
    let terminal = context.terminal();
    let server = resolve_timed_server(global, environment)?;
    let origin = origin_of(&server);

    let page = api_keys_page(&origin);
    terminal.status(StatusKind::Info, &format!("API keys page: {page}"))?;
    if !args.no_browser && !args.with_token {
        terminal.open_url(&page);
    }

    let (key_id, secret) = if args.with_token {
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
    let session = connect(context, server, credentials).await?.context();
    let user_id = session.principal.user_id;

    let credential = StoredCredential::api_key(key_id, secret, user_id, Utc::now());
    let location = save(terminal, store, &origin, &credential, args.insecure_storage)?;

    let result = LoginResult {
        origin: origin.clone(),
        key_id,
        user_id,
        affiliated_company_id: session.principal.affiliated_company_id,
        storage: location.place(),
        path: location.path().map(Path::to_path_buf),
    };
    let stored_in = location.path().map_or_else(
        || "the system keyring".to_owned(),
        |path| path.display().to_string(),
    );
    let mut messages = vec![(
        StatusKind::Success,
        format!("logged in to {origin} as {user_id}, stored in {stored_in}"),
    )];
    if environment_decides(environment) {
        messages.push((
            StatusKind::Warning,
            "CAD_API_KEY or CAD_API_SECRET is set, and takes precedence over this login; unset \
             them to use it"
                .to_owned(),
        ));
    }
    Ok(Report {
        value: to_value(&result)?,
        messages,
    })
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
    origin: &str,
    credential: &StoredCredential,
    insecure_storage: bool,
) -> Result<Location, CliError> {
    if !insecure_storage {
        match store.save_in_keyring(origin, credential) {
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
    store.save_in_file(origin, credential).map(Location::File)
}
