//! The spelling of a folder below a watched folder: its `/`-separated path,
//! and the absolute path a scan spells for it, which a stored reading of the
//! folder is keyed by. The schema reads them through `bae_folder_below` and
//! `bae_folder_relative`, so they hold wherever the library opens.

use std::path::{Component, Path};

/// A watched root or folder path the store refuses to key by: relative,
/// containing `..`, or not in canonical form.
#[derive(Debug, thiserror::Error)]
#[error("watched folder: {0}")]
pub(crate) struct WatchedPathError(pub(super) String);

impl From<WatchedPathError> for crate::db::DbError {
    fn from(error: WatchedPathError) -> Self {
        Self::Message(error.to_string())
    }
}

/// Refuse a `/`-separated path below a watched folder that is not in the
/// spelling the store keys folders by.
pub(crate) fn validate_relative_path(path: &str) -> Result<(), WatchedPathError> {
    let normalized = Path::new(path)
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or(()),
            _ => Err(()),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"));
    if normalized.as_deref() != Ok(path) {
        return Err(WatchedPathError(format!(
            "candidate path must be normalized and root-relative: {path}"
        )));
    }
    Ok(())
}

/// The absolute path of `relative` under `root`, spelled as a scan spells it,
/// which is the key a decision about that folder is stored under.
pub(crate) fn folder_below(root: &str, relative: &str) -> Result<String, WatchedPathError> {
    validate_relative_path(relative)?;
    let mut folder = std::path::PathBuf::from(root);
    for component in relative
        .split('/')
        .filter(|component| !component.is_empty())
    {
        folder.push(component);
    }
    Ok(folder.to_string_lossy().into_owned())
}

/// The `/`-separated path of `folder` below `root`, the inverse of
/// [`folder_below`]: empty for `root` itself, `None` for a folder outside it.
pub(crate) fn folder_relative_below(
    root: &str,
    folder: &str,
) -> Result<Option<String>, WatchedPathError> {
    let Ok(relative) = Path::new(folder).strip_prefix(root) else {
        return Ok(None);
    };
    relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or_else(|| {
                WatchedPathError(format!("folder path is not valid Unicode: {folder}"))
            }),
            _ => Err(WatchedPathError(format!(
                "folder path is not normalized below {root}: {folder}"
            ))),
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| Some(components.join("/")))
}
