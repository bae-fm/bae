//! Watched folders, and the rules a watched root or folder path must meet
//! before it is stored or compared.

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

/// A watched root or folder path the store refuses to key by: relative,
/// containing `..`, or not in canonical form.
#[derive(Debug, thiserror::Error)]
#[error("watched folder: {0}")]
pub(crate) struct WatchedPathError(String);

impl From<WatchedPathError> for crate::import::ImportError {
    fn from(error: WatchedPathError) -> Self {
        Self::WatchedFolder { detail: error.0 }
    }
}

impl From<WatchedPathError> for coven::DbError {
    fn from(error: WatchedPathError) -> Self {
        Self::Message(error.to_string())
    }
}

/// The one spelling of `path` this device stores for the folder it names.
///
/// Stored roots and folders are compared as strings, so each folder needs one
/// spelling however it arrived (a picker, a `file://` drop or `bae://import`
/// link, which on Windows gives `C:/Music`, or typed with a trailing
/// separator). Rejoining the path's [`Component`]s uses the host's separator,
/// collapses repeated ones, and drops `.` and trailing separators.
///
/// Refused rather than rewritten:
///
/// - `..`: resolving it without reading the filesystem is wrong when a symlink
///   is above it, and reading the filesystem fails for an offline folder.
/// - A path that is not absolute by the host's rule. On Windows `\music` is
///   relative to the current drive, so the same text can name different
///   folders.
pub(crate) fn canonical_absolute_root(path: &str) -> Result<String, WatchedPathError> {
    let refuse = |reason: &str| Err(WatchedPathError(format!("watched folder {reason}: {path}")));
    if Path::new(path)
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return refuse("must not contain `..`");
    }
    let canonical: std::path::PathBuf = Path::new(path).components().collect();
    if !canonical.is_absolute() {
        return refuse("must be an absolute path");
    }
    // Built from a `&str`, so the lossy conversion never substitutes characters.
    Ok(canonical.to_string_lossy().into_owned())
}

/// Refuse a stored root that is not in canonical form: it is corrupt, and
/// rewriting it would orphan the rows keyed by it.
pub(crate) fn validate_absolute_root(path: &str) -> Result<(), WatchedPathError> {
    let canonical = canonical_absolute_root(path)?;
    if canonical != path {
        return Err(WatchedPathError(format!(
            "stored watched folder is not its canonical spelling {canonical}: {path}"
        )));
    }
    Ok(())
}

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

pub(crate) fn candidate_relative_path(
    watched_folder_path: &str,
    candidate_path: &Path,
) -> Result<String, crate::import::ImportError> {
    let relative = candidate_path
        .strip_prefix(watched_folder_path)
        .map_err(|_| crate::import::ImportError::WatchedFolder {
            detail: format!(
                "{} is outside watched folder {watched_folder_path}",
                candidate_path.display()
            ),
        })?;
    let components: Result<Vec<_>, _> = relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().map(str::to_string).ok_or_else(|| {
                crate::import::ImportError::WatchedFolder {
                    detail: format!(
                        "candidate path is not valid Unicode: {}",
                        candidate_path.display()
                    ),
                }
            }),
            _ => Err(crate::import::ImportError::WatchedFolder {
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

/// The absolute path of `relative` under `root`, spelled as a scan spells it,
/// which is the key a decision about that folder is stored under.
pub(crate) fn folder_below(root: &str, relative: &str) -> Result<String, WatchedPathError> {
    validate_relative_path(relative)?;
    let mut folder = std::path::PathBuf::from(root);
    for component in relative.split('/').filter(|component| !component.is_empty()) {
        folder.push(component);
    }
    Ok(folder.to_string_lossy().into_owned())
}

/// Refuse a stored folder path that is not in canonical form: folder
/// decisions are keyed by the canonical path, so it is corrupt.
pub(crate) fn validate_stored_folder(path: &str) -> Result<(), WatchedPathError> {
    let canonical = canonical_absolute_root(path)?;
    if canonical != path {
        return Err(WatchedPathError(format!(
            "stored folder is not its canonical spelling {canonical}: {path}"
        )));
    }
    Ok(())
}

/// The watched folder among `roots` that covers `folder`. Watched folders
/// never overlap, so at most one does.
pub(crate) fn covering_root<'a>(roots: &'a [String], folder: &str) -> Option<&'a str> {
    roots
        .iter()
        .map(String::as_str)
        .find(|root| Path::new(folder).starts_with(root))
}

/// `posix` as an absolute path on the running host (a `C:` prefix and `\` on
/// Windows, where [`canonical_absolute_root`] refuses a `/`-rooted path).
///
/// Only the rooting changes: a trailing or doubled separator, `.`, or `..`
/// stays in place, so tests of those spellings still exercise them.
#[cfg(test)]
pub(crate) fn host_root(posix: &str) -> String {
    #[cfg(windows)]
    {
        format!("C:{}", posix.replace('/', "\\"))
    }
    #[cfg(not(windows))]
    {
        posix.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_uses_the_full_path_as_name() {
        let folder = WatchedFolder::from_path("/".to_string());
        assert_eq!(folder.name, "/");
    }

    /// The forward-slash spelling a `bae://import` link or `file://` drop gives
    /// on Windows for `host_root("/music/rips")`.
    #[cfg(windows)]
    const URL_SPELLINGS: &[&str] = &["C:/music/rips"];
    #[cfg(not(windows))]
    const URL_SPELLINGS: &[&str] = &[];

    /// Every spelling a picker, drop, or link gives for one folder is accepted
    /// and stored as the same string.
    #[test]
    fn a_root_has_one_stored_spelling_however_it_was_written() {
        let canonical = host_root("/music/rips");
        let spellings = [
            canonical.clone(),
            host_root("/music/rips/"),
            host_root("/music//rips"),
            host_root("/music/./rips"),
        ];

        for spelling in spellings
            .iter()
            .map(String::as_str)
            .chain(URL_SPELLINGS.iter().copied())
        {
            assert_eq!(
                canonical_absolute_root(spelling).unwrap(),
                canonical,
                "{spelling}"
            );
        }
    }

    /// On Windows a leading separator means the current drive, so the path
    /// names a different folder depending on the process.
    #[cfg(windows)]
    #[test]
    fn a_drive_relative_root_is_refused() {
        for rooted in ["/music/rips", r"\music\rips"] {
            let error = canonical_absolute_root(rooted).unwrap_err();
            assert!(error.to_string().contains("absolute"), "{rooted}: {error}");
        }
    }

    /// Network share and verbatim roots keep their prefix; only what follows
    /// is rejoined.
    #[cfg(windows)]
    #[test]
    fn unc_and_verbatim_roots_keep_their_prefix() {
        for (written, stored) in [
            (r"\\storage\share\Music\", r"\\storage\share\Music"),
            (r"\\?\C:\Music", r"\\?\C:\Music"),
        ] {
            assert_eq!(
                canonical_absolute_root(written).unwrap(),
                stored,
                "{written}"
            );
        }
    }

    /// Canonicalizing is stable, so [`validate_absolute_root`] accepts every
    /// root this stores.
    #[test]
    fn canonicalizing_a_canonical_root_changes_nothing() {
        // A bare share root's canonical form keeps a trailing separator
        // (`\\storage\share\`); re-reading it must not change it.
        #[cfg(windows)]
        const SHARE_ROOTS: &[&str] = &[r"\\storage\share"];
        #[cfg(not(windows))]
        const SHARE_ROOTS: &[&str] = &[];

        let spellings = [
            host_root("/music/rips/"),
            host_root("/music//rips"),
            host_root("/music/./rips"),
        ];

        for written in spellings
            .iter()
            .map(String::as_str)
            .chain(URL_SPELLINGS.iter().copied())
            .chain(SHARE_ROOTS.iter().copied())
        {
            let stored = canonical_absolute_root(written).unwrap();
            assert_eq!(
                canonical_absolute_root(&stored).unwrap(),
                stored,
                "{written}"
            );
            validate_absolute_root(&stored).unwrap_or_else(|e| panic!("{written}: {e}"));
        }
    }

    /// `..` is refused rather than resolved: resolving it without reading the
    /// filesystem is wrong when a symlink is in the path.
    #[test]
    fn a_root_climbing_out_of_itself_is_refused() {
        let error = canonical_absolute_root(&host_root("/music/../rips")).unwrap_err();
        assert!(error.to_string().contains(".."), "{error}");
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
