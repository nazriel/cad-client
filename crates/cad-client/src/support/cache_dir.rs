//! The per-user cache directory, shared by the update check and shell
//! completion so both always agree on where `cad-cli` keeps its files.

use std::path::PathBuf;

use crate::config::Environment;

/// This binary's directory inside the user cache directory.
const APP_DIR: &str = "cad-cli";

/// `<user cache dir>/cad-cli`, or `None` when the platform defines none.
///
/// An absolute `XDG_CACHE_HOME` wins on every platform, so the cache can be
/// relocated (or isolated in tests) the conventional way. Otherwise `os`,
/// normally [`std::env::consts::OS`], picks the platform default:
/// `%LOCALAPPDATA%` on Windows, `~/Library/Caches` on macOS and `~/.cache`
/// elsewhere. Relative values are ignored.
pub(crate) fn app_cache_dir(environment: &impl Environment, os: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        environment
            .get(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let root = absolute("XDG_CACHE_HOME").or_else(|| match os {
        "windows" => absolute("LOCALAPPDATA"),
        "macos" => absolute("HOME").map(|home| home.join("Library").join("Caches")),
        _ => absolute("HOME").map(|home| home.join(".cache")),
    })?;
    Some(root.join(APP_DIR))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::app_cache_dir;
    use crate::config::Environment;

    struct Env(HashMap<&'static str, String>);

    impl Environment for Env {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    fn env<const N: usize>(pairs: [(&'static str, &str); N]) -> Env {
        Env(pairs.into_iter().map(|(k, v)| (k, v.to_owned())).collect())
    }

    #[test]
    fn an_absolute_xdg_cache_home_wins_on_every_platform() {
        let base = std::env::temp_dir();
        let base_str = base.to_str().expect("UTF-8 temp dir");
        for os in ["linux", "macos", "windows"] {
            let environment = env([
                ("XDG_CACHE_HOME", base_str),
                ("HOME", "/unused"),
                ("LOCALAPPDATA", "/unused"),
            ]);
            assert_eq!(
                app_cache_dir(&environment, os),
                Some(base.join("cad-cli")),
                "{os}"
            );
        }
    }

    #[test]
    fn otherwise_the_platform_default_is_used() {
        let base = std::env::temp_dir();
        let base_str = base.to_str().expect("UTF-8 temp dir");
        let home = env([("XDG_CACHE_HOME", "relative"), ("HOME", base_str)]);
        assert_eq!(
            app_cache_dir(&home, "linux"),
            Some(base.join(".cache").join("cad-cli"))
        );
        assert_eq!(
            app_cache_dir(&home, "macos"),
            Some(base.join("Library").join("Caches").join("cad-cli"))
        );
        assert_eq!(
            app_cache_dir(&env([("LOCALAPPDATA", base_str)]), "windows"),
            Some(base.join("cad-cli"))
        );
        assert_eq!(app_cache_dir(&env([]), "linux"), None::<PathBuf>);
    }
}
