//! Watched folders, and the rules a watched root or folder path must meet
//! before it is stored or compared.

use super::relative_folder::{RelativeFolder, WatchedPathError};
use std::path::{Component, Path, PathBuf};
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

impl From<WatchedPathError> for crate::import::ImportError {
    fn from(error: WatchedPathError) -> Self {
        Self::WatchedFolder { detail: error.0 }
    }
}

/// The one spelling of `path` this device stores for the folder it names.
///
/// Stored roots and folders are compared by their bytes, so each folder needs
/// one spelling however it arrived (a picker, a `file://` drop or
/// `bae://import` link, which on Windows gives `C:/Music`, or typed with a
/// trailing separator). Rejoining the path's [`Component`]s uses the host's
/// separator, collapses repeated ones, and drops `.` and trailing separators.
///
/// Refused rather than rewritten:
///
/// - `..`: resolving it without reading the filesystem is wrong when a symlink
///   is above it, and reading the filesystem fails for an offline folder.
/// - A path that is not absolute by the host's rule. On Windows `\music` is
///   relative to the current drive, so the same text can name different
///   folders.
pub(crate) fn canonical_absolute_root(path: &Path) -> Result<PathBuf, WatchedPathError> {
    let refuse = |reason: &str| {
        Err(WatchedPathError(format!(
            "watched folder {reason}: {}",
            coven::path_text(path)
        )))
    };
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return refuse("must not contain `..`");
    }
    let canonical: PathBuf = path.components().collect();
    if !canonical.is_absolute() {
        return refuse("must be an absolute path");
    }
    Ok(canonical)
}

/// Refuse a stored path that is not in canonical form: rows are keyed by its
/// bytes, so it is corrupt, and rewriting it would orphan them. Compares the
/// spellings, not the components, which `Path`'s equality compares: it calls
/// `/a/b/` and `/a/b` equal.
fn validate_canonical(path: &Path, what: &str) -> Result<(), WatchedPathError> {
    let canonical = canonical_absolute_root(path)?;
    if canonical.as_os_str() != path.as_os_str() {
        return Err(WatchedPathError(format!(
            "stored {what} is not its canonical spelling {}: {}",
            coven::path_text(&canonical),
            coven::path_text(path)
        )));
    }
    Ok(())
}

/// Refuse a stored root that is not in canonical form.
pub(crate) fn validate_absolute_root(path: &Path) -> Result<(), WatchedPathError> {
    validate_canonical(path, "watched folder")
}

/// The folder `candidate_path` is below the watched folder at
/// `watched_folder_path`.
pub(crate) fn candidate_relative_path(
    watched_folder_path: &Path,
    candidate_path: &Path,
) -> Result<RelativeFolder, crate::import::ImportError> {
    let relative = candidate_path
        .strip_prefix(watched_folder_path)
        .map_err(|error| crate::import::ImportError::WatchedFolder {
            detail: format!(
                "{} is outside watched folder {}: {error}",
                coven::path_text(candidate_path),
                coven::path_text(watched_folder_path)
            ),
        })?;
    Ok(RelativeFolder::of(relative)?)
}

pub(crate) fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// Refuse a stored folder path that is not in canonical form: folder
/// decisions are keyed by the canonical path, so it is corrupt.
pub(crate) fn validate_stored_folder(path: &Path) -> Result<(), WatchedPathError> {
    validate_canonical(path, "folder")
}

/// `posix` as an absolute path on the running host (a `C:` prefix and `\` on
/// Windows, where [`canonical_absolute_root`] refuses a `/`-rooted path).
///
/// Only the rooting changes: a trailing or doubled separator, `.`, or `..`
/// stays in place, so tests of those spellings still exercise them.
#[cfg(test)]
pub(crate) fn host_root(posix: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!("C:{}", posix.replace('/', "\\")))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(posix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_path_uses_the_full_path_as_name() {
        let folder = WatchedFolder::from_path(PathBuf::from("/"));
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
            .map(PathBuf::as_path)
            .chain(URL_SPELLINGS.iter().map(Path::new))
        {
            assert_eq!(
                canonical_absolute_root(spelling).unwrap().as_os_str(),
                canonical.as_os_str(),
                "{}",
                spelling.display()
            );
        }
    }

    /// On Windows a leading separator means the current drive, so the path
    /// names a different folder depending on the process.
    #[cfg(windows)]
    #[test]
    fn a_drive_relative_root_is_refused() {
        for rooted in ["/music/rips", r"\music\rips"] {
            let error = canonical_absolute_root(Path::new(rooted)).unwrap_err();
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
                canonical_absolute_root(Path::new(written))
                    .unwrap()
                    .as_os_str(),
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
            .map(PathBuf::as_path)
            .chain(URL_SPELLINGS.iter().map(Path::new))
            .chain(SHARE_ROOTS.iter().map(Path::new))
        {
            let stored = canonical_absolute_root(written).unwrap();
            assert_eq!(
                canonical_absolute_root(&stored).unwrap().as_os_str(),
                stored.as_os_str(),
                "{}",
                written.display()
            );
            validate_absolute_root(&stored)
                .unwrap_or_else(|e| panic!("{}: {e}", written.display()));
        }
    }

    /// `..` is refused rather than resolved: resolving it without reading the
    /// filesystem is wrong when a symlink is in the path.
    #[test]
    fn a_root_climbing_out_of_itself_is_refused() {
        let error = canonical_absolute_root(&host_root("/music/../rips")).unwrap_err();
        assert!(error.to_string().contains(".."), "{error}");
    }

    /// A stored root spelled with a trailing separator names the same
    /// components as its canonical spelling, but not the same bytes, so the
    /// rows keyed by it would miss: it is refused.
    #[test]
    fn a_stored_root_with_a_trailing_separator_is_refused() {
        let error = validate_absolute_root(&host_root("/music/rips/")).unwrap_err();
        assert!(error.to_string().contains("canonical"), "{error}");
    }
}
