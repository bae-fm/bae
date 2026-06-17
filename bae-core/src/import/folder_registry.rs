//! Persistent list of folders the user watches for imports.
//!
//! Stored as `import_folders.yaml` beside the library's `config.yaml` — appdata,
//! not the database. It's local UI state about *where to look* for music, not
//! synced library content, so it never enters the DB or the sync set. On launch
//! the import service loads the registry and scans each folder; the folders
//! survive restart.

use crate::library_dir::LibraryDir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, warn};

/// A folder the user added to watch for imports, with the display name shown in
/// the candidate-list group header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchedFolder {
    /// Absolute path of the watched folder.
    pub path: String,
    /// Final path component, shown as the group header.
    pub name: String,
}

impl WatchedFolder {
    fn from_path(path: String) -> Self {
        let name = match std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
        {
            Some(name) => name.to_string(),
            None => {
                warn!(
                    "watched folder {path:?} has no usable final path component; \
                     using the full path as its group name"
                );
                path.clone()
            }
        };
        Self { path, name }
    }
}

/// The watched-folder list, persisted per library. Construct via [`load`].
///
/// [`load`]: ImportFolderRegistry::load
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportFolderRegistry {
    /// Watched folder paths, in the order the user added them.
    folders: Vec<String>,
}

impl ImportFolderRegistry {
    fn file_path(library_dir: &LibraryDir) -> PathBuf {
        library_dir.join("import_folders.yaml")
    }

    /// Load the registry from disk, or an empty one when the file is absent,
    /// unreadable, or malformed. A corrupt or unreadable file warns and starts
    /// with an empty watch list rather than failing app start — the user re-adds
    /// folders and the registry rewrites cleanly on the next change. Returning
    /// the empty default here (with a log) keeps the policy in one place instead
    /// of every caller deciding how to recover.
    pub fn load(library_dir: &LibraryDir) -> Self {
        let path = Self::file_path(library_dir);
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_yaml::from_str(&text) {
                Ok(registry) => registry,
                Err(e) => {
                    warn!(
                        "malformed {} ({e}); starting with an empty watch list",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(
                    "no {} yet; starting with an empty watch list",
                    path.display()
                );
                Self::default()
            }
            Err(e) => {
                warn!(
                    "reading {} failed ({e}); starting with an empty watch list",
                    path.display()
                );
                Self::default()
            }
        }
    }

    fn save(&self, library_dir: &LibraryDir) -> Result<(), String> {
        let path = Self::file_path(library_dir);
        let yaml = serde_yaml::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, yaml).map_err(|e| format!("writing {}: {e}", path.display()))
    }

    /// The watched folders, in add order, as the list the UI renders as group
    /// headers.
    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folders
            .iter()
            .cloned()
            .map(WatchedFolder::from_path)
            .collect()
    }

    /// Add `path` if not already watched, persisting on change. Returns `true`
    /// when it was newly added (the caller then scans it), `false` if it was
    /// already present.
    pub fn add(&mut self, library_dir: &LibraryDir, path: String) -> Result<bool, String> {
        if self.folders.contains(&path) {
            return Ok(false);
        }
        self.folders.push(path);
        self.save(library_dir)?;
        Ok(true)
    }

    /// Remove `path`, persisting on change. Returns `true` when it was present.
    pub fn remove(&mut self, library_dir: &LibraryDir, path: &str) -> Result<bool, String> {
        let before = self.folders.len();
        self.folders.retain(|p| p != path);
        if self.folders.len() == before {
            return Ok(false);
        }
        self.save(library_dir)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_library_dir() -> (tempfile::TempDir, LibraryDir) {
        let dir = tempfile::tempdir().unwrap();
        let library_dir = LibraryDir::new(dir.path().to_path_buf());
        (dir, library_dir)
    }

    #[test]
    fn add_persists_and_survives_reload() {
        let (_tmp, library_dir) = temp_library_dir();

        let mut registry = ImportFolderRegistry::load(&library_dir);
        assert!(registry.watched_folders().is_empty());

        assert!(registry
            .add(&library_dir, "/Volumes/Music/New Rips".to_string())
            .unwrap());
        assert!(registry
            .add(&library_dir, "/Users/sam/Downloads/Bandcamp".to_string())
            .unwrap());
        // A duplicate add is a no-op.
        assert!(!registry
            .add(&library_dir, "/Volumes/Music/New Rips".to_string())
            .unwrap());

        // A fresh load sees the persisted folders, in add order, with names.
        let reloaded = ImportFolderRegistry::load(&library_dir);
        let folders = reloaded.watched_folders();
        assert_eq!(
            folders,
            vec![
                WatchedFolder {
                    path: "/Volumes/Music/New Rips".to_string(),
                    name: "New Rips".to_string(),
                },
                WatchedFolder {
                    path: "/Users/sam/Downloads/Bandcamp".to_string(),
                    name: "Bandcamp".to_string(),
                },
            ]
        );
    }

    #[test]
    fn remove_persists_and_reports_presence() {
        let (_tmp, library_dir) = temp_library_dir();
        let mut registry = ImportFolderRegistry::load(&library_dir);
        registry.add(&library_dir, "/a".to_string()).unwrap();
        registry.add(&library_dir, "/b".to_string()).unwrap();

        assert!(registry.remove(&library_dir, "/a").unwrap());
        // Removing an absent path is a no-op.
        assert!(!registry.remove(&library_dir, "/a").unwrap());

        let reloaded = ImportFolderRegistry::load(&library_dir);
        assert_eq!(
            reloaded.watched_folders(),
            vec![WatchedFolder {
                path: "/b".to_string(),
                name: "b".to_string(),
            }]
        );
    }
}
