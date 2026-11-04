//! Watched folders.

use bae_db::WatchedPathError;
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedFolder {
    pub path: PathBuf,
    /// The text that shows the folder: its last name's escape
    /// (`coven::path_text`), or the whole path's for a root.
    pub name: String,
}

impl WatchedFolder {
    pub(crate) fn from_path(path: PathBuf) -> Self {
        if path.file_name().is_none() {
            warn!(
                "watched folder {} has no final path component; \
                 using the full path as its group name",
                coven::path_text(&path)
            );
        }
        let name = bae_util::path_text::name_text(&path);
        Self { path, name }
    }
}

impl From<WatchedPathError> for crate::ImportError {
    fn from(error: WatchedPathError) -> Self {
        Self::WatchedFolder(error)
    }
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
#[path = "watched_folder_tests.rs"]
mod tests;
