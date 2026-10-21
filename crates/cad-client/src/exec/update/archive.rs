//! Verifying a downloaded release archive and taking the executable out of it.
//!
//! The rules match `install.sh` and `install.ps1`: the archive must be listed
//! in the release's `SHA256SUMS`, its digest must match, and the executable is
//! the one file at `<archive stem>/cad-cli[.exe]`.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Component, Path};

use tempfile::TempPath;

use super::target::{ArchiveFormat, Target};
use crate::error::CliError;

/// Finds the expected SHA-256 digest of `file` in a `SHA256SUMS` document.
///
/// Both `sha256sum` spellings are accepted: `<digest>  <name>` (text mode) and
/// `<digest> *<name>` (binary mode).
pub(crate) fn expected_digest<'a>(sums: &'a str, file: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let (digest, name) = line.trim_end().split_once(char::is_whitespace)?;
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        (name == file).then_some(digest)
    })
}

/// Lower-case hexadecimal SHA-256 digest of `bytes`.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest
        .as_ref()
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// Checks `archive` against the digest `SHA256SUMS` lists for `name`.
///
/// # Errors
///
/// Fails when the archive is not listed or its digest differs; in either case
/// nothing from it may be installed.
pub(crate) fn verify(archive: &[u8], name: &str, sums: &str) -> Result<(), CliError> {
    let expected = expected_digest(sums, name)
        .ok_or_else(|| CliError::Update(format!("{name} is not listed in SHA256SUMS")))?;
    if sha256_hex(archive).eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(CliError::Update(format!(
            "checksum mismatch for {name}; nothing was installed"
        )))
    }
}

/// Writes the executable found in `archive` to a new, closed temporary file
/// that is deleted when the returned path is dropped.
///
/// The file goes into `directory` when one is given and writable, otherwise
/// into the system temporary directory. Placing it beside the running
/// executable keeps it off a `noexec` `/tmp`, so it can be run before it is
/// installed. On Unix it is made executable (0o755).
///
/// # Errors
///
/// Fails when the archive cannot be read or does not contain the executable at
/// `<stem>/<binary name>`.
pub(crate) fn extract_binary(
    archive: &[u8],
    target: Target,
    stem: &str,
    directory: Option<&Path>,
) -> Result<TempPath, CliError> {
    let binary = target.binary_name();
    let wanted = |path: &Path| {
        let mut components = path.components().filter(|c| *c != Component::CurDir);
        components.next() == Some(Component::Normal(stem.as_ref()))
            && components.next() == Some(Component::Normal(binary.as_ref()))
            && components.next().is_none()
    };
    let bytes = match target.format() {
        ArchiveFormat::TarGz => read_tar_gz(archive, wanted),
        ArchiveFormat::Zip => read_zip(archive, wanted),
    }?
    .ok_or_else(|| CliError::Update(format!("the archive does not contain {stem}/{binary}")))?;

    let mut builder = tempfile::Builder::new();
    builder
        .prefix(".cad-cli-update-")
        .suffix(target.executable_suffix());
    let mut file = match directory.and_then(|directory| builder.tempfile_in(directory).ok()) {
        Some(file) => file,
        None => builder.tempfile()?,
    };
    file.write_all(&bytes)?;
    file.flush()?;
    // Closed before it runs: Linux refuses to execute a file that is still
    // open for writing, and Windows to run one another handle holds.
    let path = file.into_temp_path();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(path)
}

fn unreadable(error: impl std::fmt::Display) -> CliError {
    CliError::Update(format!("the release archive cannot be read: {error}"))
}

#[cfg(not(windows))]
fn read_tar_gz(
    archive: &[u8],
    wanted: impl Fn(&Path) -> bool,
) -> Result<Option<Vec<u8>>, CliError> {
    use std::io::Read as _;

    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tar.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        if wanted(&entry.path().map_err(unreadable)?) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(unreadable)?;
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn read_tar_gz(
    _archive: &[u8],
    _wanted: impl Fn(&Path) -> bool,
) -> Result<Option<Vec<u8>>, CliError> {
    Err(unreadable("tar archives are not supported on Windows"))
}

#[cfg(windows)]
fn read_zip(archive: &[u8], wanted: impl Fn(&Path) -> bool) -> Result<Option<Vec<u8>>, CliError> {
    use std::io::Read as _;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive)).map_err(unreadable)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(unreadable)?;
        if !entry.is_file() {
            continue;
        }
        if entry.enclosed_name().is_some_and(|path| wanted(&path)) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(unreadable)?;
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

#[cfg(not(windows))]
fn read_zip(_archive: &[u8], _wanted: impl Fn(&Path) -> bool) -> Result<Option<Vec<u8>>, CliError> {
    Err(unreadable("zip archives are only supported on Windows"))
}
