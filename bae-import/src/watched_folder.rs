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
        let name = match path.file_name() {
            Some(name) => coven::path_text(Path::new(name)),
            None => {
                warn!(
                    "watched folder {} has no final path component; \
                     using the full path as its group name",
                    coven::path_text(&path)
                );
                coven::path_text(&path)
            }
        };
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
mod tests {
    use super::*;

    #[test]
    fn root_path_uses_the_full_path_as_name() {
        let folder = WatchedFolder::from_path(PathBuf::from("/"));
        assert_eq!(folder.name, "/");
    }
}
