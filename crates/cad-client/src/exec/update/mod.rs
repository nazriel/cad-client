//! Self-update from the project's GitHub releases.
//!
//! GitHub is not the CAD platform, so these requests are outside `cad-api`'s
//! remit: this module is the one place in `cad-client` that uses `reqwest`
//! directly. It looks up the latest release, downloads the archive for the
//! running target within size limits, verifies it against the release's
//! `SHA256SUMS` exactly as `install.sh` and `install.ps1` do, runs the new
//! binary once to prove it works here, and swaps the executable in place.
//!
//! Every URL it follows must be HTTPS. The only exception is the
//! `CAD_CLI_UPDATE_URL` seam, which lets tests serve releases from a
//! plain-HTTP loopback address.

pub(crate) mod archive;
pub(crate) mod install;
pub(crate) mod notes;
pub(crate) mod notice;
pub(crate) mod target;
/// Also the home of the release fixtures the `update` command's tests share.
#[cfg(test)]
pub(crate) mod tests;

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use reqwest::header::ACCEPT;
use reqwest::{StatusCode, Url, redirect};
use semver::Version;
use serde::Deserialize;
use tempfile::TempPath;

use self::target::Target;
use crate::config::Environment;
use crate::error::CliError;

/// GitHub API document describing the latest published (non-draft,
/// non-prerelease) release.
pub(crate) const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/nazriel/cad-client/releases/latest";
/// Replaces [`LATEST_RELEASE_URL`]. It must serve the same JSON shape, and the
/// asset URLs in it must serve the archive and `SHA256SUMS`.
///
/// A test seam, deliberately left out of the help and the documentation: a
/// supported mirror would make the GitHub release JSON a public contract.
pub(crate) const URL_VARIABLE: &str = "CAD_CLI_UPDATE_URL";
/// Asset listing the SHA-256 digest of every archive in a release.
const SUMS_ASSET: &str = "SHA256SUMS";
/// Upper bound on connection setup, whatever the overall request budget is.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// GitHub's release downloads redirect once or twice to a CDN.
const MAX_REDIRECTS: usize = 5;

// Size caps. Every body is read with a running limit, so a misbehaving
// server or mirror can neither exhaust memory nor stall an update on an
// endless stream. Each cap is far above what a release actually contains.

/// Largest release archive downloaded. A release archive holds one stripped
/// binary plus README and LICENSE, a few MiB today.
pub(crate) const MAX_ARCHIVE_BYTES: u64 = 100 * 1024 * 1024;
/// Largest `SHA256SUMS` downloaded: one ~100-byte line per archive, six today.
pub(crate) const MAX_SUMS_BYTES: u64 = 64 * 1024;
/// Largest release document read. It carries the notes and the asset list,
/// around 10 KiB today.
pub(crate) const MAX_DOCUMENT_BYTES: u64 = 1024 * 1024;

/// The version of the running binary.
pub(crate) fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo package versions are semver")
}

/// The one-line notice that `latest` is newer than `current`, or `None` when
/// it is not.
pub(crate) fn available_message(latest: &Version, current: &Version) -> Option<String> {
    (latest > current)
        .then(|| format!("cad-cli {latest} is available (current {current}); run: cad-cli update"))
}

/// Parses a release tag such as `v0.2.0` into its version.
///
/// # Errors
///
/// Fails when the tag, without its optional leading `v`, is not semver.
pub(crate) fn parse_tag(tag: &str) -> Result<Version, CliError> {
    let raw = tag.strip_prefix('v').unwrap_or(tag);
    Version::parse(raw)
        .map_err(|error| CliError::Update(format!("release tag `{tag}` is not a version: {error}")))
}

/// A published release and the files attached to it.
#[derive(Debug, Clone)]
pub(crate) struct Release {
    pub(crate) version: Version,
    /// Markdown release notes, as `mise run release:notes` wrote them.
    pub(crate) notes: Option<String>,
    /// The release's GitHub page.
    pub(crate) page: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
struct Asset {
    name: String,
    url: String,
    /// Size in bytes the release advertises.
    size: u64,
}

impl Release {
    fn asset(&self, name: &str) -> Result<&Asset, CliError> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .ok_or_else(|| {
                CliError::Update(format!("release v{} has no asset {name}", self.version))
            })
    }
}

/// The subset of GitHub's release document the updater reads.
#[derive(Debug, Deserialize)]
struct ReleaseDocument {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<AssetDocument>,
}

#[derive(Debug, Deserialize)]
struct AssetDocument {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Where releases are looked up, with the HTTP client that fetches them.
pub(crate) struct ReleaseSource {
    http: reqwest::Client,
    latest_url: Url,
    /// Set only when [`URL_VARIABLE`] overrides the default, so a release
    /// served by GitHub can never send the updater to plain HTTP.
    allow_loopback_http: bool,
}

impl ReleaseSource {
    /// Builds the source from [`URL_VARIABLE`], or GitHub when it is unset.
    ///
    /// `timeout` bounds each request, response body included.
    ///
    /// # Errors
    ///
    /// Fails when the override is not an HTTPS URL or a plain-HTTP loopback
    /// URL, or when the HTTP client cannot be built.
    pub(crate) fn from_environment(
        environment: &impl Environment,
        timeout: Duration,
    ) -> Result<Self, CliError> {
        let Some(raw) = environment
            .get(URL_VARIABLE)
            .filter(|value| !value.is_empty())
        else {
            let url = Url::parse(LATEST_RELEASE_URL).expect("the release URL is valid");
            return Self::new(url, false, timeout);
        };
        let url = Url::parse(&raw)
            .ok()
            .filter(|url| permitted(url, true))
            .ok_or_else(|| {
                CliError::config(format!(
                    "{URL_VARIABLE} must be an https URL, or an http URL of a loopback host"
                ))
            })?;
        Self::new(url, true, timeout)
    }

    fn new(
        latest_url: Url,
        allow_loopback_http: bool,
        timeout: Duration,
    ) -> Result<Self, CliError> {
        let policy = redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if permitted(attempt.url(), allow_loopback_http) {
                attempt.follow()
            } else {
                attempt.error("refusing to follow a redirect to a non-HTTPS URL")
            }
        });
        // GitHub is not the platform, but downloads get the same rustls
        // provider as the SDK's clients. `main` installed it already; this
        // covers the updater's tests, which have no `main`.
        cad_api::tls::install_crypto_provider();
        let http = reqwest::Client::builder()
            .user_agent(concat!("cad-cli/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(timeout.min(CONNECT_TIMEOUT))
            .timeout(timeout)
            .redirect(policy)
            .build()
            .map_err(|error| CliError::transport(format!("cannot build HTTP client: {error}")))?;
        Ok(Self {
            http,
            latest_url,
            allow_loopback_http,
        })
    }

    /// Looks up the latest published release.
    ///
    /// # Errors
    ///
    /// Fails when the release cannot be fetched, none is published, or its
    /// document or tag cannot be read.
    pub(crate) async fn latest(&self) -> Result<Release, CliError> {
        let url = self.latest_url.as_str();
        let response = self.get(url, "application/vnd.github+json").await?;
        let overflow = format!("the release document is larger than {MAX_DOCUMENT_BYTES} bytes");
        let body = read_limited(response, url, MAX_DOCUMENT_BYTES, &overflow).await?;
        let document: ReleaseDocument = serde_json::from_slice(&body).map_err(|error| {
            CliError::Update(format!("the release document cannot be read: {error}"))
        })?;
        Ok(Release {
            version: parse_tag(&document.tag_name)?,
            notes: document.body,
            page: document.html_url,
            assets: document
                .assets
                .into_iter()
                .map(|asset| Asset {
                    name: asset.name,
                    url: asset.browser_download_url,
                    size: asset.size,
                })
                .collect(),
        })
    }

    /// Downloads the archive of `release` for `target`, verifies it against
    /// the release's `SHA256SUMS` and extracts the executable into a
    /// temporary file in `directory` (see [`archive::extract_binary`]).
    ///
    /// # Errors
    ///
    /// Fails when either asset is missing, larger than its cap, cannot be
    /// downloaded or does not match its advertised size; when the archive is
    /// not listed in `SHA256SUMS` or its digest differs; or when the archive
    /// does not contain the executable.
    pub(crate) async fn fetch_binary(
        &self,
        release: &Release,
        target: Target,
        directory: Option<&Path>,
    ) -> Result<TempPath, CliError> {
        let name = target.archive_name(&release.version);
        let archive_asset = release.asset(&name)?;
        let sums_asset = release.asset(SUMS_ASSET)?;

        let sums = self.download(sums_asset, MAX_SUMS_BYTES).await?;
        let sums = String::from_utf8(sums)
            .map_err(|_| CliError::Update(format!("{SUMS_ASSET} is not UTF-8 text")))?;
        let archive = self.download(archive_asset, MAX_ARCHIVE_BYTES).await?;

        archive::verify(&archive, &name, &sums)?;
        archive::extract_binary(
            &archive,
            target,
            &target.archive_stem(&release.version),
            directory,
        )
    }

    /// Downloads `asset`, which must be exactly the size the release
    /// advertises and no larger than `cap`.
    async fn download(&self, asset: &Asset, cap: u64) -> Result<Vec<u8>, CliError> {
        let name = &asset.name;
        if asset.size > cap {
            return Err(CliError::Update(format!(
                "{name} is listed at {} bytes, more than the {cap}-byte limit for it",
                asset.size
            )));
        }
        let response = self.get(&asset.url, "application/octet-stream").await?;
        let overflow = format!(
            "{name} sent more than the {} bytes the release lists for it",
            asset.size
        );
        let body = read_limited(response, &asset.url, asset.size, &overflow).await?;
        let received = u64::try_from(body.len()).unwrap_or(u64::MAX);
        if received < asset.size {
            return Err(CliError::Update(format!(
                "{name} ended after {received} of its {} bytes",
                asset.size
            )));
        }
        Ok(body)
    }

    async fn get(&self, url: &str, accept: &str) -> Result<reqwest::Response, CliError> {
        let parsed = Url::parse(url)
            .ok()
            .filter(|parsed| permitted(parsed, self.allow_loopback_http))
            .ok_or_else(|| CliError::Update(format!("refusing to fetch non-HTTPS URL {url}")))?;
        let response = self
            .http
            .get(parsed)
            .header(ACCEPT, accept)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|error| CliError::transport(format!("request to {url} failed: {error}")))?;
        match response.status() {
            status if status.is_success() => Ok(response),
            StatusCode::NOT_FOUND => Err(CliError::Update(format!(
                "{url} was not found (HTTP 404); is a release published?"
            ))),
            status => Err(CliError::Update(format!("{url} returned HTTP {status}"))),
        }
    }
}

/// Reads `response` into memory, failing as soon as more than `limit` bytes
/// arrive rather than after the whole body has been buffered.
async fn read_limited(
    mut response: reqwest::Response,
    url: &str,
    limit: u64,
    overflow: &str,
) -> Result<Vec<u8>, CliError> {
    let too_large = || CliError::Update(overflow.to_owned());
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        return Err(too_large());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| CliError::transport(format!("download from {url} failed: {error}")))?
    {
        let total = u64::try_from(body.len() + chunk.len()).unwrap_or(u64::MAX);
        if total > limit {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Directory of the running executable, where an update is staged so it can
/// be run from the file system it will live on. `None` when it is unknown.
pub(crate) fn install_directory() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    // Unix: the directory a symlinked binary really lives in, which is where
    // `self_replace` writes too. Windows paths are left as they are.
    #[cfg(unix)]
    let executable = executable.canonicalize().unwrap_or(executable);
    executable.parent().map(Path::to_path_buf)
}

/// Whether the updater may fetch `url`: HTTPS always, plain HTTP only to a
/// loopback host and only when explicitly allowed.
pub(crate) fn permitted(url: &Url, allow_loopback_http: bool) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => allow_loopback_http && is_loopback(url),
        _ => false,
    }
}

fn is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}
