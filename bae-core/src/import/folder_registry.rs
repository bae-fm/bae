//! Persistent list of folders the user watches for imports, stored as
//! `import_folders.yaml` beside the library's `config.yaml` — appdata, not the
//! database. It's local state about *where to look* for music, not synced library
//! content, so it never enters the DB or the sync set. The import service loads
//! it and scans each folder at launch, so the folders survive restart.

use coven::StoreDir;
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
    /// Candidate folder paths the user manually marked as skipped. A skipped
    /// candidate still scans but renders under the import view's "Skipped" tab
    /// instead of "New".
    skipped: Vec<String>,
}

impl ImportFolderRegistry {
    fn file_path(library_dir: &StoreDir) -> PathBuf {
        library_dir.join("import_folders.yaml")
    }

    /// Load the registry from disk, or an empty one when the file is absent,
    /// unreadable, or malformed — a corrupt file warns and starts with an empty
    /// watch list rather than failing app start, and the user re-adds folders
    /// from there. Recovering here, in one place, keeps every caller from
    /// inventing its own policy.
    pub fn load(library_dir: &StoreDir) -> Self {
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

    fn save(&self, library_dir: &StoreDir) -> Result<(), crate::import::ImportError> {
        let path = Self::file_path(library_dir);
        let yaml =
            serde_yaml::to_string(self).map_err(|e| crate::import::ImportError::Registry {
                detail: e.to_string(),
            })?;
        // Atomic write: this file is durable user intent (watched roots + skip
        // marks). A torn in-place write would truncate it, and `load` recovers a
        // truncated file by starting empty — silently losing the list. temp +
        // rename means a crash leaves the whole old file, never a partial one.
        crate::util::atomic_write::write_atomic_io(&path, yaml.as_bytes()).map_err(|e| {
            crate::import::ImportError::Registry {
                detail: format!("writing {}: {e}", path.display()),
            }
        })
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
    pub fn add(
        &mut self,
        library_dir: &StoreDir,
        path: String,
    ) -> Result<bool, crate::import::ImportError> {
        if self.folders.contains(&path) {
            return Ok(false);
        }
        self.folders.push(path);
        self.save(library_dir)?;
        Ok(true)
    }

    /// Remove `path`, persisting on change. Returns `true` when it was present.
    pub fn remove(
        &mut self,
        library_dir: &StoreDir,
        path: &str,
    ) -> Result<bool, crate::import::ImportError> {
        let before = self.folders.len();
        self.folders.retain(|p| p != path);
        if self.folders.len() == before {
            return Ok(false);
        }
        self.save(library_dir)?;
        Ok(true)
    }

    /// Whether the candidate folder at `path` is manually marked as skipped.
    pub fn is_skipped(&self, path: &str) -> bool {
        self.skipped.iter().any(|p| p == path)
    }

    /// Mark `path` skipped (insert) or unskip it (remove), persisting on change.
    /// Returns `true` when the skip set changed, `false` when the request was a
    /// no-op (already in the requested state).
    pub fn set_skipped(
        &mut self,
        library_dir: &StoreDir,
        path: String,
        skipped: bool,
    ) -> Result<bool, crate::import::ImportError> {
        let changed = if skipped {
            if self.skipped.contains(&path) {
                false
            } else {
                self.skipped.push(path);
                true
            }
        } else {
            let before = self.skipped.len();
            self.skipped.retain(|p| p != &path);
            self.skipped.len() != before
        };
        if changed {
            self.save(library_dir)?;
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_library_dir() -> (tempfile::TempDir, StoreDir) {
        let dir = tempfile::tempdir().unwrap();
        let library_dir = StoreDir::new(dir.path().to_path_buf());
        (dir, library_dir)
    }

    /// A save that cannot complete must leave the previously-persisted file
    /// intact, never a truncated one — the whole point of the atomic write. With
    /// the parent dir read-only, an in-place `std::fs::write` would still modify
    /// the existing file (writing an existing file needs write on the file, not
    /// the dir) and destroy it; the atomic temp-then-rename cannot create its temp
    /// there, so it fails and the old file survives.
    #[cfg(unix)]
    #[test]
    fn save_failure_preserves_the_existing_registry() {
        use std::os::unix::fs::PermissionsExt;

        let (tmp, library_dir) = temp_library_dir();
        let mut registry = ImportFolderRegistry::load(&library_dir);
        registry
            .add(&library_dir, "/music/keep".to_string())
            .unwrap();

        // Make the parent unwritable, so the next save's temp file cannot be
        // created there.
        let read_only = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions(tmp.path(), read_only).unwrap();

        let mut doomed = ImportFolderRegistry::default();
        doomed.folders.push("/music/replacement".to_string());
        let result = doomed.save(&library_dir);

        // Restore write access so TempDir cleanup and the reload below succeed.
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            result.is_err(),
            "a save that cannot write atomically must fail, not silently modify in place",
        );
        let reloaded = ImportFolderRegistry::load(&library_dir);
        assert_eq!(
            reloaded.watched_folders(),
            vec![WatchedFolder {
                path: "/music/keep".to_string(),
                name: "keep".to_string(),
            }],
            "the existing registry must survive a failed save",
        );
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
            .add(&library_dir, "/music/library/incoming".to_string())
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
                    path: "/music/library/incoming".to_string(),
                    name: "incoming".to_string(),
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

    #[test]
    fn skip_persists_and_survives_reload() {
        let (_tmp, library_dir) = temp_library_dir();
        let mut registry = ImportFolderRegistry::load(&library_dir);
        assert!(!registry.is_skipped("/a"));

        assert!(registry
            .set_skipped(&library_dir, "/a".to_string(), true)
            .unwrap());
        // Skipping an already-skipped path is a no-op.
        assert!(!registry
            .set_skipped(&library_dir, "/a".to_string(), true)
            .unwrap());
        assert!(registry.is_skipped("/a"));

        // A fresh load sees the persisted skip.
        let reloaded = ImportFolderRegistry::load(&library_dir);
        assert!(reloaded.is_skipped("/a"));
        assert!(!reloaded.is_skipped("/b"));
    }

    #[test]
    fn unskip_persists_and_reports_change() {
        let (_tmp, library_dir) = temp_library_dir();
        let mut registry = ImportFolderRegistry::load(&library_dir);
        registry
            .set_skipped(&library_dir, "/a".to_string(), true)
            .unwrap();

        assert!(registry
            .set_skipped(&library_dir, "/a".to_string(), false)
            .unwrap());
        // Unskipping an already-unskipped path is a no-op.
        assert!(!registry
            .set_skipped(&library_dir, "/a".to_string(), false)
            .unwrap());

        let reloaded = ImportFolderRegistry::load(&library_dir);
        assert!(!reloaded.is_skipped("/a"));
    }

    #[test]
    fn skip_set_is_independent_of_watched_folders() {
        // The skip set tracks candidate folders, which are distinct from the
        // watched roots; the two lists don't interfere.
        let (_tmp, library_dir) = temp_library_dir();
        let mut registry = ImportFolderRegistry::load(&library_dir);
        registry.add(&library_dir, "/root".to_string()).unwrap();
        registry
            .set_skipped(&library_dir, "/root/Album".to_string(), true)
            .unwrap();

        let reloaded = ImportFolderRegistry::load(&library_dir);
        assert_eq!(
            reloaded.watched_folders(),
            vec![WatchedFolder {
                path: "/root".to_string(),
                name: "root".to_string(),
            }]
        );
        assert!(reloaded.is_skipped("/root/Album"));
    }

    #[test]
    fn loads_file_with_both_fields() {
        // An `import_folders.yaml` carrying both fields loads as written.
        let (_tmp, library_dir) = temp_library_dir();
        let path = ImportFolderRegistry::file_path(&library_dir);
        std::fs::write(&path, "folders:\n  - /a\nskipped:\n  - /a/Album\n").unwrap();

        let registry = ImportFolderRegistry::load(&library_dir);
        assert_eq!(
            registry.watched_folders(),
            vec![WatchedFolder {
                path: "/a".to_string(),
                name: "a".to_string(),
            }]
        );
        assert!(registry.is_skipped("/a/Album"));
        assert!(!registry.is_skipped("/a"));
    }

    #[test]
    fn malformed_yaml_warns_and_starts_empty() {
        // Content that parses as YAML but not into the registry shape (a bare
        // sequence, not the folders/skipped map) must not fail app start —
        // load warns and returns the empty default.
        let (_tmp, library_dir) = temp_library_dir();
        let path = ImportFolderRegistry::file_path(&library_dir);
        std::fs::write(&path, "- not\n- a\n- mapping\n").unwrap();

        let registry = ImportFolderRegistry::load(&library_dir);
        assert!(registry.watched_folders().is_empty());
        assert!(!registry.is_skipped("/anything"));
    }

    #[test]
    fn unreadable_file_warns_and_starts_empty() {
        // A registry path that isn't a readable file — here a directory where
        // the YAML should be — is an error other than NotFound; load warns and
        // starts empty rather than propagating the failure to app start.
        let (_tmp, library_dir) = temp_library_dir();
        let path = ImportFolderRegistry::file_path(&library_dir);
        std::fs::create_dir(&path).unwrap();

        let registry = ImportFolderRegistry::load(&library_dir);
        assert!(registry.watched_folders().is_empty());
    }

    #[test]
    fn watched_folder_without_final_component_uses_full_path_as_name() {
        // A root path has no final component; the group name falls back to the
        // full path so the header is never empty.
        let folder = WatchedFolder::from_path("/".to_string());
        assert_eq!(folder.path, "/");
        assert_eq!(folder.name, "/");
    }
}
