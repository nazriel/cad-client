//! Sets `CAD_CLI_BUILD_VERSION`, the version `cad-cli --version` prints.
//!
//! The commit comes from `CAD_CLI_BUILD_COMMIT`, which CI sets for the
//! archives it builds from anything but a release tag; they print a
//! `-dev+<commit>` version. Without it the binary prints the package version.
//!
//! The script deliberately asks Git nothing. A version derived from the
//! checkout changes with every commit, and a changed build-script output makes
//! Cargo rebuild every target of this package, so each commit would recompile
//! the library and its tests locally and in every CI test job, with release
//! builds paying the LTO link again even for a documentation-only commit.

use std::env;

#[path = "build/version.rs"]
mod version;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=build/version.rs");
    println!("cargo::rerun-if-env-changed=CAD_CLI_BUILD_COMMIT");

    let package = env::var("CARGO_PKG_VERSION").expect("Cargo sets CARGO_PKG_VERSION");
    let raw = env::var("CAD_CLI_BUILD_COMMIT").unwrap_or_default();
    let commit = version::parse_commit(&raw)
        .unwrap_or_else(|error| panic!("invalid CAD_CLI_BUILD_COMMIT: {error}"));

    let version = version::build_version(&package, commit);
    println!("cargo::rustc-env=CAD_CLI_BUILD_VERSION={version}");
}
