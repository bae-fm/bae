//! In-memory index of device-local watched-folder state stored in SQLite.

use std::collections::HashSet;
use std::path::{Component, Path};
use tracing::warn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchedFolder {
    pub path: String,
    pub name: String,
}

impl WatchedFolder {
    pub(crate) fn from_path(path: String) -> Self {
        let name = match Path::new(&path).file_name().and_then(|name| name.to_str()) {
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

#[derive(Debug, Clone, Default)]
pub struct ImportFolderRegistry {
    folders: Vec<String>,
    skipped: HashSet<(String, String)>,
}

impl ImportFolderRegistry {
    pub(crate) fn from_stored(
        folders: Vec<String>,
        skipped: Vec<(String, String)>,
    ) -> Result<Self, crate::import::ImportError> {
        for (index, root) in folders.iter().enumerate() {
            validate_absolute_root(root)?;
            if let Some(conflict) = folders[index + 1..]
                .iter()
                .find(|other| paths_overlap(Path::new(root), Path::new(other)))
            {
                return Err(crate::import::ImportError::Registry {
                    detail: format!(
                        "watched folders cannot overlap: {root} conflicts with {conflict}"
                    ),
                });
            }
        }
        let folder_set: HashSet<_> = folders.iter().map(String::as_str).collect();
        for (root, relative) in &skipped {
            if !folder_set.contains(root.as_str()) {
                return Err(crate::import::ImportError::Registry {
                    detail: format!("skipped candidate belongs to unknown watched folder {root}"),
                });
            }
            validate_relative_path(relative)?;
        }
        Ok(Self {
            folders,
            skipped: skipped.into_iter().collect(),
        })
    }

    pub fn watched_folders(&self) -> Vec<WatchedFolder> {
        self.folders
            .iter()
            .cloned()
            .map(WatchedFolder::from_path)
            .collect()
    }

    pub(crate) fn apply_added(&mut self, path: String) {
        if !self.folders.contains(&path) {
            self.folders.push(path);
        }
    }

    pub(crate) fn apply_removed(&mut self, path: &str) {
        self.folders.retain(|root| root != path);
        self.skipped.retain(|(root, _)| root != path);
    }

    pub(crate) fn apply_skipped(
        &mut self,
        watched_folder_path: String,
        relative_candidate_path: String,
        skipped: bool,
    ) {
        let key = (watched_folder_path, relative_candidate_path);
        if skipped {
            self.skipped.insert(key);
        } else {
            self.skipped.remove(&key);
        }
    }

    pub(crate) fn is_skipped(
        &self,
        watched_folder_path: &str,
        candidate_path: &Path,
    ) -> Result<bool, crate::import::ImportError> {
        let relative = candidate_relative_path(watched_folder_path, candidate_path)?;
        Ok(self
            .skipped
            .contains(&(watched_folder_path.to_string(), relative)))
    }
}

pub(crate) fn validate_absolute_root(path: &str) -> Result<(), crate::import::ImportError> {
    let parsed = Path::new(path);
    let normalized: std::path::PathBuf = parsed.components().collect();
    if !parsed.is_absolute()
        || parsed
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || normalized.to_string_lossy() != path
    {
        return Err(crate::import::ImportError::Registry {
            detail: format!(
                "watched folder must be an absolute normalized path: {}",
                parsed.display()
            ),
        });
    }
    Ok(())
}

pub(crate) fn validate_relative_path(path: &str) -> Result<(), crate::import::ImportError> {
    let normalized = Path::new(path)
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or(()),
            _ => Err(()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"));
    if normalized.as_deref() != Ok(path) {
        return Err(crate::import::ImportError::Registry {
            detail: format!("candidate path must be normalized and root-relative: {path}"),
        });
    }
    Ok(())
}

pub(crate) fn candidate_relative_path(
    watched_folder_path: &str,
    candidate_path: &Path,
) -> Result<String, crate::import::ImportError> {
    let relative = candidate_path
        .strip_prefix(watched_folder_path)
        .map_err(|_| crate::import::ImportError::Registry {
            detail: format!(
                "{} is outside watched folder {watched_folder_path}",
                candidate_path.display()
            ),
        })?;
    let components: Result<Vec<_>, _> = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string).ok_or_else(|| {
                crate::import::ImportError::Registry {
                    detail: format!(
                        "candidate path is not valid Unicode: {}",
                        candidate_path.display()
                    ),
                }
            }),
            _ => Err(crate::import::ImportError::Registry {
                detail: format!(
                    "candidate path is not normalized below its watched folder: {}",
                    candidate_path.display()
                ),
            }),
        })
        .collect();
    let relative = components?.join("/");
    validate_relative_path(&relative)?;
    Ok(relative)
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_registry_preserves_order_and_derives_names() {
        let registry = ImportFolderRegistry::from_stored(
            vec!["/Volumes/Incoming".to_string(), "/music/rips".to_string()],
            vec![("/music/rips".to_string(), "Release".to_string())],
        )
        .unwrap();
        assert_eq!(
            registry.watched_folders(),
            vec![
                WatchedFolder {
                    path: "/Volumes/Incoming".to_string(),
                    name: "Incoming".to_string(),
                },
                WatchedFolder {
                    path: "/music/rips".to_string(),
                    name: "rips".to_string(),
                },
            ]
        );
        assert!(registry
            .is_skipped("/music/rips", Path::new("/music/rips/Release"))
            .unwrap());
    }

    #[test]
    fn stored_registry_rejects_overlapping_roots() {
        let error = ImportFolderRegistry::from_stored(
            vec!["/music".to_string(), "/music/artist".to_string()],
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cannot overlap"));
    }

    #[test]
    fn root_path_uses_the_full_path_as_name() {
        let folder = WatchedFolder::from_path("/".to_string());
        assert_eq!(folder.name, "/");
    }

    #[test]
    fn watched_root_can_itself_be_a_skipped_candidate() {
        let registry = ImportFolderRegistry::from_stored(
            vec!["/music/release".to_string()],
            vec![("/music/release".to_string(), String::new())],
        )
        .unwrap();
        assert!(registry
            .is_skipped("/music/release", Path::new("/music/release"))
            .unwrap());
    }

    #[test]
    fn relative_paths_have_one_canonical_spelling() {
        for invalid in ["a//b", "a/./b", "a/../b", "/a"] {
            assert!(validate_relative_path(invalid).is_err(), "{invalid}");
        }
        assert!(validate_relative_path("").is_ok());
        assert!(validate_relative_path("a/b").is_ok());
    }
}
