//! The release target a running binary updates itself from.
//!
//! The triple is derived from `std::env::consts` rather than baked in by a
//! build script: the six published targets are the only ones an update can
//! come from, and each is fully determined by the operating system and the
//! architecture.

use semver::Version;

/// One of the targets a release publishes an archive for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Target {
    triple: &'static str,
    format: ArchiveFormat,
}

/// How a release archive is packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveFormat {
    TarGz,
    Zip,
}

impl ArchiveFormat {
    const fn extension(self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Zip => "zip",
        }
    }
}

impl Target {
    /// The target of the running binary, or `None` on a platform no release
    /// is published for.
    pub(crate) fn current() -> Option<Self> {
        Self::from_parts(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Maps an operating system and architecture, spelled as in
    /// [`std::env::consts`], to the published target. Linux maps to the
    /// statically linked musl build, the only Linux build that is published.
    pub(crate) fn from_parts(os: &str, arch: &str) -> Option<Self> {
        let (triple, format) = match (os, arch) {
            ("linux", "x86_64") => ("x86_64-unknown-linux-musl", ArchiveFormat::TarGz),
            ("linux", "aarch64") => ("aarch64-unknown-linux-musl", ArchiveFormat::TarGz),
            ("macos", "x86_64") => ("x86_64-apple-darwin", ArchiveFormat::TarGz),
            ("macos", "aarch64") => ("aarch64-apple-darwin", ArchiveFormat::TarGz),
            ("windows", "x86_64") => ("x86_64-pc-windows-msvc", ArchiveFormat::Zip),
            ("windows", "aarch64") => ("aarch64-pc-windows-msvc", ArchiveFormat::Zip),
            _ => return None,
        };
        Some(Self { triple, format })
    }

    pub(crate) const fn format(self) -> ArchiveFormat {
        self.format
    }

    /// Top-level directory inside the archive, e.g.
    /// `cad-cli-v0.2.0-x86_64-unknown-linux-musl`, as `mise run dist` packs it.
    pub(crate) fn archive_stem(self, version: &Version) -> String {
        format!("cad-cli-v{version}-{}", self.triple)
    }

    /// Release asset name, e.g. `cad-cli-v0.2.0-x86_64-pc-windows-msvc.zip`.
    pub(crate) fn archive_name(self, version: &Version) -> String {
        format!("{}.{}", self.archive_stem(version), self.format.extension())
    }

    /// File name of the executable inside the archive directory.
    pub(crate) const fn binary_name(self) -> &'static str {
        match self.format {
            ArchiveFormat::TarGz => "cad-cli",
            ArchiveFormat::Zip => "cad-cli.exe",
        }
    }

    /// Extension an executable needs on this target: Windows only runs a file
    /// it recognises as a program.
    pub(crate) const fn executable_suffix(self) -> &'static str {
        match self.format {
            ArchiveFormat::TarGz => "",
            ArchiveFormat::Zip => ".exe",
        }
    }
}
