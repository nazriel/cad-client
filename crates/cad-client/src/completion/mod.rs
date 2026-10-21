//! Dynamic shell completion.
//!
//! The shell integration is clap's `CompleteEnv` protocol. Registration is
//! printed by `COMPLETE=<shell> cad-cli` or, equivalently, by
//! `cad-cli completions <shell>`. On every Tab the registered shell function
//! then runs `COMPLETE=<shell> cad-cli -- <words>`, and this module answers
//! with candidates on stdout before any configuration is resolved.
//!
//! Commands, flags and enum values are completed from the command definition.
//! Profile names are completed from the profiles file, without any request.
//! A few identifiers are completed from the platform: `--company-id` and the
//! positional reporting-period IDs. Those lookups follow strict rules, because
//! anything a completer prints ends up in the user's prompt:
//!
//! - they never prompt and never write anything but candidates to stdout;
//! - missing credentials, bad configuration, a network error or a timeout all
//!   mean "no candidates" (or the stale cache), never an error;
//! - a live lookup is bounded by [`lookup::LIVE_TIMEOUT`] and its result is
//!   cached per server, API key, company and resource (see [`cache`]);
//! - `CAD_CLI_COMPLETE_OFFLINE=1` restricts them to the cache.

mod cache;
mod lookup;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::io;
use std::process::ExitCode;
use std::time::SystemTime;

use clap::{CommandFactory, FromArgMatches};
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use clap_complete::env::{Bash, Elvish, EnvCompleter, Fish, Powershell, Shells, Zsh};
use clap_complete::{CompleteEnv, Shell};

use self::cache::{Cache, CacheKey, Candidate, Network};
use crate::cli::{Cli, GlobalArgs};
use crate::config::{
    AppConfig, Environment, ProcessEnvironment, resolve_credentials, select, select_company,
};
use crate::support::credentials::CredentialStore;
use crate::support::profiles::ProfileStore;
use crate::support::switch::is_off_word;

/// Name the shell completes, and the program its registration calls back.
///
/// It is fixed rather than taken from `argv[0]`, so a registration generated
/// from `./target/release/cad-cli` still calls the `cad-cli` on `PATH`.
const BINARY: &str = "cad-cli";

/// Environment variable that switches the binary into completion mode.
const VAR: &str = "COMPLETE";

/// Environment variable that confines platform-backed completion to the cache.
pub(crate) const OFFLINE_VAR: &str = "CAD_CLI_COMPLETE_OFFLINE";

thread_local! {
    /// The words before the one being completed.
    ///
    /// clap hands a value completer only the partial value, but the right
    /// candidates depend on sibling arguments such as `--server-url` and
    /// `--company-id`. The words are stashed here for the duration of one
    /// completion request, which runs synchronously on this thread.
    static WORDS: RefCell<Vec<OsString>> = const { RefCell::new(Vec::new()) };
}

/// Whether this process was started by a shell to compute completions.
pub(crate) fn requested() -> bool {
    std::env::var_os(VAR).is_some_and(|value| !value.is_empty() && value != "0")
}

/// Answers a completion request, or returns `None` for an ordinary run.
pub(crate) fn try_complete(args: &[OsString]) -> Option<ExitCode> {
    if !requested() {
        return None;
    }
    WORDS.set(preceding_words(args, completion_index()));
    let current_dir = std::env::current_dir().ok();
    let result = CompleteEnv::with_factory(Cli::command)
        .var(VAR)
        .bin(BINARY)
        .completer(BINARY)
        .shells(SHELLS)
        .try_complete(args.iter().cloned(), current_dir.as_deref());
    WORDS.take();
    match result {
        Ok(true) => Some(ExitCode::SUCCESS),
        Ok(false) => None,
        Err(error) => {
            let _ = error.print();
            Some(ExitCode::from(2))
        }
    }
}

/// Writes the registration script for `shell`: the same bytes as
/// `COMPLETE=<shell> cad-cli`.
pub(crate) fn write_registration(shell: Shell, buffer: &mut dyn io::Write) -> io::Result<()> {
    let name = shell.to_string();
    let completer = SHELLS
        .completer(&name)
        .ok_or_else(|| io::Error::other(format!("no dynamic completion for {name}")))?;
    completer.write_registration(VAR, BINARY, BINARY, BINARY, buffer)
}

/// clap's built-in shells, with zsh swapped for [`AutoloadableZsh`].
const SHELLS: Shells<'static> = Shells(&[&Bash, &Elvish, &Fish, &Powershell, &AutoloadableZsh]);

/// clap's zsh integration, made usable as an autoloaded `_cad-cli` in `$fpath`.
///
/// clap's script defines the completer and registers it with `compdef`, which
/// is right when it is sourced from `.zshrc`. Installed in `$fpath`, the file
/// is instead the body of `_cad-cli`, run on the first Tab: registering alone
/// would leave that first Tab without candidates. The appended guard, the same
/// idiom clap's static zsh script uses, completes immediately in that case.
struct AutoloadableZsh;

impl EnvCompleter for AutoloadableZsh {
    fn name(&self) -> &'static str {
        Zsh.name()
    }

    fn is(&self, name: &str) -> bool {
        Zsh.is(name)
    }

    fn write_registration(
        &self,
        var: &str,
        name: &str,
        bin: &str,
        completer: &str,
        buffer: &mut dyn io::Write,
    ) -> io::Result<()> {
        Zsh.write_registration(var, name, bin, completer, buffer)?;
        let function = name.replace('-', "_");
        writeln!(
            buffer,
            "if [ \"$funcstack[1]\" = \"_{bin}\" ]; then\n    _clap_dynamic_completer_{function} \"$@\"\nfi"
        )
    }

    fn write_complete(
        &self,
        command: &mut clap::Command,
        args: Vec<OsString>,
        current_dir: Option<&std::path::Path>,
        buffer: &mut dyn io::Write,
    ) -> io::Result<()> {
        Zsh.write_complete(command, args, current_dir, buffer)
    }
}

/// Completes a company ID with the companies the API key can access.
pub(crate) fn company_ids() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| complete(Resource::Companies, current))
}

/// Completes a profile name with the profiles in the profiles file. It makes
/// no request and reads no credentials.
pub(crate) fn profile_names() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| {
        let Some(current) = current.to_str() else {
            return Vec::new();
        };
        let profiles = ProfileStore::from_environment(&ProcessEnvironment)
            .load()
            .unwrap_or_default();
        profiles
            .iter()
            .filter(|(name, _)| name.as_str().starts_with(current))
            .map(|(name, profile)| {
                CompletionCandidate::new(name.as_str()).help(Some(profile.url.clone().into()))
            })
            .collect()
    })
}

/// Completes a reporting-period ID with the periods of the selected company.
pub(crate) fn reporting_period_ids() -> ArgValueCompleter {
    ArgValueCompleter::new(|current: &OsStr| complete(Resource::ReportingPeriods, current))
}

/// A platform list that can supply completion candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resource {
    /// Session-level: the same for every company of one API key.
    Companies,
    /// Company-scoped: depends on `--company-id` or `CAD_COMPANY_ID`.
    ReportingPeriods,
}

impl Resource {
    const fn name(self) -> &'static str {
        match self {
            Self::Companies => "companies",
            Self::ReportingPeriods => "reporting-periods",
        }
    }
}

fn complete(resource: Resource, current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    let words = WORDS.with_borrow(Clone::clone);
    let store = CredentialStore::from_environment(&ProcessEnvironment);
    let profiles = ProfileStore::from_environment(&ProcessEnvironment);
    candidates(
        resource,
        &words,
        &ProcessEnvironment,
        &store,
        &profiles,
        SystemTime::now(),
    )
    .into_iter()
    .filter(|candidate| candidate.value.starts_with(current))
    .map(|candidate| CompletionCandidate::new(candidate.value).help(candidate.help.map(Into::into)))
    .collect()
}

/// Every candidate for `resource` in the context the preceding `words`,
/// `environment`, credential `store` and `profiles` describe, unfiltered.
///
/// The server and the login are those the command being completed would use,
/// profile and active profile included.
pub(crate) fn candidates(
    resource: Resource,
    words: &[OsString],
    environment: &impl Environment,
    store: &CredentialStore,
    profiles: &ProfileStore,
    now: SystemTime,
) -> Vec<Candidate> {
    let Some(mut global) = global_args(words) else {
        return Vec::new();
    };
    // Without a server and a key identifier there is no cache key either, so
    // there is nothing to answer from; the secret is only needed to go live.
    // The credentials are found once, here, and reused by the live lookup, so
    // the store is read at most once per request.
    global.request_timeout = lookup::LIVE_TIMEOUT;
    let Ok(profiles) = profiles.load() else {
        return Vec::new();
    };
    let Ok(selection) = select(&global, environment, &profiles) else {
        return Vec::new();
    };
    let Ok(resolved) = resolve_credentials(environment, store, &selection) else {
        return Vec::new();
    };
    let api_key_id = resolved.key_id();
    let origin = selection.server.base_url().as_str().to_owned();
    let origin = origin.as_str();
    // Reporting periods belong to the selected company; without one there is
    // nothing to offer, as there is no command to complete them for.
    let company = match resource {
        Resource::Companies => None,
        Resource::ReportingPeriods => {
            match select_company(&global, environment, &profiles, &selection) {
                Ok(Some(company)) => Some(company),
                Ok(None) | Err(_) => return Vec::new(),
            }
        }
    };
    let key = match &company {
        None => CacheKey::session(resource.name(), origin, api_key_id),
        Some(company) => CacheKey::company(resource.name(), origin, api_key_id, company.id),
    };
    let network = if offline(environment) {
        Network::Disabled
    } else {
        Network::Allowed
    };
    let cache = Cache::from_environment(environment);
    cache::lookup(cache.as_ref(), &key, network, now, || {
        let config = AppConfig::resolve_with(selection, resolved, company).ok()?;
        lookup::fetch(resource, config)
    })
}

/// The global options among the preceding words, parsed leniently: an
/// incomplete command line is the normal state while completing.
fn global_args(words: &[OsString]) -> Option<GlobalArgs> {
    if words.is_empty() {
        return None;
    }
    let matches = Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(words)
        .ok()?;
    GlobalArgs::from_arg_matches(&matches).ok()
}

fn offline(environment: &impl Environment) -> bool {
    environment
        .get(OFFLINE_VAR)
        .is_some_and(|value| !(value.trim().is_empty() || is_off_word(&value)))
}

/// Position of the word being completed, as bash and zsh report it.
fn completion_index() -> Option<usize> {
    std::env::var("_CLAP_COMPLETE_INDEX").ok()?.parse().ok()
}

/// The words of `cad-cli -- <words>` that precede the word being completed.
///
/// Shells that report no index (fish, elvish, PowerShell) pass the line only
/// up to the cursor, so the current word is the last one.
fn preceding_words(args: &[OsString], index: Option<usize>) -> Vec<OsString> {
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        return Vec::new();
    };
    let mut words = args[separator + 1..].to_vec();
    let current = index.unwrap_or_else(|| words.len().saturating_sub(1));
    words.truncate(current);
    words
}
