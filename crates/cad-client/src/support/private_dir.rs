//! Directories only their owner may enter, for files that must stay private.

use std::fs;
use std::io;
use std::path::Path;

/// Creates `dir` and any missing parents, mode 0700 on Unix. An existing
/// directory is left as it is.
pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)
}
