//! The per-user configuration directory, where `cad-cli` keeps the one file
//! it is allowed to treat as settings: the credentials fallback.

use std::path::PathBuf;

use crate::config::Environment;

/// This binary's directory inside the user configuration directory.
const APP_DIR: &str = "cad-cli";

/// `<user config dir>/cad-cli`, or `None` when the platform defines none.
///
/// Mirrors [`super::cache_dir::app_cache_dir`]: an absolute `XDG_CONFIG_HOME`
/// wins on every platform, so the directory can be relocated (or isolated in
/// tests) the conventional way. Otherwise `os`, normally
/// [`std::env::consts::OS`], picks the platform default: `%APPDATA%` on
/// Windows, `~/Library/Application Support` on macOS and `~/.config`
/// elsewhere. Relative values are ignored.
pub(crate) fn app_config_dir(environment: &impl Environment, os: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        environment
            .get(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let root = absolute("XDG_CONFIG_HOME").or_else(|| match os {
        "windows" => absolute("APPDATA"),
        "macos" => absolute("HOME").map(|home| home.join("Library").join("Application Support")),
        _ => absolute("HOME").map(|home| home.join(".config")),
    })?;
    Some(root.join(APP_DIR))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::app_config_dir;
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
    fn an_absolute_xdg_config_home_wins_on_every_platform() {
        let base = std::env::temp_dir();
        let base_str = base.to_str().expect("UTF-8 temp dir");
        for os in ["linux", "macos", "windows"] {
            let environment = env([
                ("XDG_CONFIG_HOME", base_str),
                ("HOME", "/unused"),
                ("APPDATA", "/unused"),
            ]);
            assert_eq!(
                app_config_dir(&environment, os),
                Some(base.join("cad-cli")),
                "{os}"
            );
        }
    }

    #[test]
    fn otherwise_the_platform_default_is_used() {
        let base = std::env::temp_dir();
        let base_str = base.to_str().expect("UTF-8 temp dir");
        let home = env([("XDG_CONFIG_HOME", "relative"), ("HOME", base_str)]);
        assert_eq!(
            app_config_dir(&home, "linux"),
            Some(base.join(".config").join("cad-cli"))
        );
        assert_eq!(
            app_config_dir(&home, "macos"),
            Some(
                base.join("Library")
                    .join("Application Support")
                    .join("cad-cli")
            )
        );
        assert_eq!(
            app_config_dir(&env([("APPDATA", base_str)]), "windows"),
            Some(base.join("cad-cli"))
        );
        assert_eq!(app_config_dir(&env([]), "linux"), None);
    }
}
