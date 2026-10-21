//! On-disk cache of completion candidates.
//!
//! A Tab press must answer quickly, and the platform is often slow, offline or
//! unconfigured. Each list of candidates is therefore kept as one small JSON
//! file in the user's cache directory: a fresh file answers without a request,
//! and a stale one still answers when the live request fails.
//!
//! The cache never holds a secret. Files are told apart by a key built from the
//! server origin, a hash of the API key *identifier* (never the secret), the
//! company and the resource, so environments, keys and companies cannot mix.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cad_api::ids::CompanyId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::Environment;
use crate::support::cache_dir::app_cache_dir;

/// How long a fetched list answers without asking the platform again.
pub(crate) const TTL: Duration = Duration::from_hours(1);

/// Bumped whenever the file layout changes; older files are then ignored.
const FORMAT_VERSION: u32 = 1;

/// One completion value and the text a shell may show beside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Candidate {
    pub(crate) value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) help: Option<String>,
}

impl Candidate {
    pub(crate) fn new(value: impl Into<String>, help: Option<String>) -> Self {
        Self {
            value: value.into(),
            help,
        }
    }
}

/// Identifies one list of candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheKey {
    resource: &'static str,
    /// Human-readable scope, stored in the file so a hash collision between
    /// two file names is detected instead of answered from the wrong list.
    scope: String,
}

impl CacheKey {
    /// Key for a session-level `resource` seen through one API key on one
    /// server: the same for every company the key can act on.
    pub(crate) fn session(resource: &'static str, origin: &str, api_key_id: Uuid) -> Self {
        Self {
            resource,
            scope: format!("{origin} key:{:016x} {resource}", key_hash(api_key_id)),
        }
    }

    /// Key for a company-scoped `resource`.
    ///
    /// `company` is `None` for "the company the API key selects by default",
    /// which the key hash already pins down.
    pub(crate) fn company(
        resource: &'static str,
        origin: &str,
        api_key_id: Uuid,
        company: Option<CompanyId>,
    ) -> Self {
        let company = company.map_or_else(|| "default".to_owned(), |id| id.to_string());
        Self {
            resource,
            scope: format!(
                "{origin} key:{:016x} company:{company} {resource}",
                key_hash(api_key_id)
            ),
        }
    }

    fn file_name(&self) -> String {
        format!(
            "{}-{:016x}.json",
            self.resource,
            fnv1a(self.scope.as_bytes())
        )
    }
}

/// Whether a cache miss may be answered from the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Network {
    Allowed,
    /// `CAD_CLI_COMPLETE_OFFLINE`: answer from the cache, fresh or stale, only.
    Disabled,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    version: u32,
    key: String,
    updated_at: u64,
    candidates: Vec<Candidate>,
}

/// The directory holding one JSON file per cached list.
#[derive(Debug, Clone)]
pub(crate) struct Cache {
    dir: PathBuf,
}

impl Cache {
    pub(crate) const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// `completions/` in the user cache directory (see
    /// [`app_cache_dir`]), or `None` when the platform defines none.
    pub(crate) fn from_environment(environment: &impl Environment) -> Option<Self> {
        app_cache_dir(environment, std::env::consts::OS)
            .map(|dir| Self::at(dir.join("completions")))
    }

    fn path(&self, key: &CacheKey) -> PathBuf {
        self.dir.join(key.file_name())
    }

    /// Reads the list stored for `key`; any unreadable, foreign or outdated
    /// file is simply a miss.
    fn read(&self, key: &CacheKey) -> Option<Entry> {
        let bytes = fs::read(self.path(key)).ok()?;
        let entry: Entry = serde_json::from_slice(&bytes).ok()?;
        (entry.version == FORMAT_VERSION && entry.key == key.scope).then_some(entry)
    }

    /// Replaces the list stored for `key` atomically: readers see the old file
    /// or the new one, never a partial write.
    pub(crate) fn write(
        &self,
        key: &CacheKey,
        candidates: &[Candidate],
        now: SystemTime,
    ) -> io::Result<()> {
        create_private_dir(&self.dir)?;
        let entry = Entry {
            version: FORMAT_VERSION,
            key: key.scope.clone(),
            updated_at: epoch_seconds(now),
            candidates: candidates.to_vec(),
        };
        let mut file = tempfile::Builder::new()
            .prefix(".")
            .suffix(".tmp")
            .tempfile_in(&self.dir)?;
        file.write_all(&serde_json::to_vec(&entry).map_err(io::Error::other)?)?;
        file.persist(self.path(key)).map_err(|error| error.error)?;
        Ok(())
    }
}

/// Answers from a fresh cache, else from `fetch`, else from a stale cache.
///
/// `fetch` returns `None` when the live lookup failed for any reason; the
/// caller has nothing useful to say about why during completion. A failed
/// cache write is ignored for the same reason.
pub(crate) fn lookup(
    cache: Option<&Cache>,
    key: &CacheKey,
    network: Network,
    now: SystemTime,
    fetch: impl FnOnce() -> Option<Vec<Candidate>>,
) -> Vec<Candidate> {
    let cached = cache.and_then(|cache| cache.read(key));
    if let Some(entry) = &cached
        && epoch_seconds(now).saturating_sub(entry.updated_at) < TTL.as_secs()
    {
        return entry.candidates.clone();
    }
    let live = match network {
        Network::Allowed => fetch(),
        Network::Disabled => None,
    };
    match live {
        Some(candidates) => {
            if let Some(cache) = cache {
                let _ = cache.write(key, &candidates, now);
            }
            candidates
        }
        None => cached.map(|entry| entry.candidates).unwrap_or_default(),
    }
}

fn epoch_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The cache lists company names, so only the owner may read it.
fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}

/// The API key identifier as it appears in a cache key: hashed, so the files
/// tell keys apart without recording which key was used.
fn key_hash(api_key_id: Uuid) -> u64 {
    fnv1a(api_key_id.as_bytes())
}

/// 64-bit FNV-1a: stable across toolchains and platforms, unlike
/// `DefaultHasher`, so a file name stays the same after an upgrade.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
