//! The version `cad-cli --version` prints, from the package version and the
//! commit a CI build names.
//!
//! The build script reads the commit and calls this; the library compiles it
//! only for its unit tests. Keep it free of I/O and dependencies.

/// Hexadecimal digits of the commit hash a development version carries.
pub(crate) const HASH_LENGTH: usize = 12;

/// Shortest abbreviated commit accepted from the environment.
const MIN_HASH_LENGTH: usize = 7;

/// Reads the value of `CAD_CLI_BUILD_COMMIT`: blank means no commit, anything
/// else must be a hexadecimal commit hash.
pub(crate) fn parse_commit(raw: &str) -> Result<Option<&str>, String> {
    let commit = raw.trim();
    if commit.is_empty() {
        return Ok(None);
    }
    if commit.len() < MIN_HASH_LENGTH || !commit.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "`{commit}` is not a commit hash of at least {MIN_HASH_LENGTH} hexadecimal digits"
        ));
    }
    Ok(Some(commit))
}

/// Returns the version to print for `package` built from `commit`.
///
/// - No commit (release builds, local builds, source archives): `package`
///   unchanged, so a release build prints exactly `cad-cli <version>`.
/// - A commit: a semantic-version pre-release of `package` with the
///   abbreviated commit as build metadata, `0.1.0-dev+0123456789ab`.
pub(crate) fn build_version(package: &str, commit: Option<&str>) -> String {
    let Some(commit) = commit else {
        return package.to_owned();
    };

    let hash = commit
        .get(..HASH_LENGTH)
        .unwrap_or(commit)
        .to_ascii_lowercase();
    let (base, metadata) = match package.split_once('+') {
        Some((base, metadata)) => (base, Some(metadata)),
        None => (package, None),
    };
    // A package already on a pre-release extends it with one more identifier.
    let dev = if base.contains('-') { ".dev" } else { "-dev" };
    match metadata {
        Some(metadata) => format!("{base}{dev}+{metadata}.{hash}"),
        None => format!("{base}{dev}+{hash}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_version, parse_commit};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn no_commit_prints_the_package_version() {
        assert_eq!(build_version("0.1.0", None), "0.1.0");
        assert_eq!(build_version("0.2.0-rc.1", None), "0.2.0-rc.1");
    }

    #[test]
    fn a_commit_makes_a_dev_build() {
        assert_eq!(
            build_version("0.1.0", Some(COMMIT)),
            "0.1.0-dev+0123456789ab"
        );
    }

    #[test]
    fn dev_build_extends_an_existing_pre_release_and_build_metadata() {
        assert_eq!(
            build_version("0.2.0-rc.1", Some(COMMIT)),
            "0.2.0-rc.1.dev+0123456789ab"
        );
        assert_eq!(
            build_version("0.2.0+linux", Some(COMMIT)),
            "0.2.0-dev+linux.0123456789ab"
        );
    }

    #[test]
    fn short_and_upper_case_hashes_are_kept_whole_in_lower_case() {
        assert_eq!(build_version("0.1.0", Some("ABC1234")), "0.1.0-dev+abc1234");
    }

    #[test]
    fn blank_commit_means_none() {
        assert_eq!(parse_commit(""), Ok(None));
        assert_eq!(parse_commit("  \n"), Ok(None));
        assert_eq!(parse_commit(&format!("{COMMIT}\n")), Ok(Some(COMMIT)));
    }

    #[test]
    fn anything_but_a_hex_hash_is_rejected() {
        assert!(parse_commit("main").is_err());
        assert!(parse_commit("abc12").is_err());
        assert!(parse_commit("0123456789abcdeg").is_err());
    }
}
