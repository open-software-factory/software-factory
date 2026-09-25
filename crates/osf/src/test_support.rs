//! The one place in this crate's own tests that reserves a directory under
//! the OS temp directory, so no two calls, in one process or several, ever
//! share one (ruling F8).
#![cfg(test)]

use std::ops::Deref;
use std::path::{Path, PathBuf};

/// A path unique to this process and this call, under the OS temp
/// directory.
pub(crate) fn unique_temp_path(prefix: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock is after the epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}

/// A fresh, empty directory unique to this process and this call. Its
/// `Drop` removes it.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(prefix: &str) -> Self {
        let dir = unique_temp_path(prefix);
        std::fs::create_dir_all(&dir).expect("temp dir creates");
        TempDir(dir)
    }
}

impl Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for TempDir {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
