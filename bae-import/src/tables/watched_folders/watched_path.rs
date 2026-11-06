//! Watched folders' paths as the store keys them: the one spelling of a
//! watched root, a folder below a watched folder as the bytes the store keys
//! it by, and the absolute path a scan spells for it, which a stored reading
//! of the folder is keyed by. The schema reads them through
//! `bae_folder_below` and `bae_folder_relative`, so they hold wherever the
//! library opens.

use std::path::{Component, Path, PathBuf};

use rusqlite::types::Value;

use bae_store::SchemaFunction;

/// A watched root or folder path the store refuses to key by: relative,
/// containing `..`, or not in canonical form.
#[derive(Debug, thiserror::Error)]
#[error("watched folder: {0}")]
pub struct WatchedPathError(String);

/// A folder below a watched folder: coven's bytes for each of its names
/// (`coven::path_bytes`), outermost first, joined by `/`; empty for the
/// watched folder itself. The bytes key the folder, so a name that isn't
/// valid Unicode names its own folder; [`Self::text`] only shows it. Every
/// name is one ordinary name on this platform, which [`Self::from_bytes`]
/// checks of bytes it did not make.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelativeFolder(Vec<u8>);

impl RelativeFolder {
    /// The watched folder itself.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// The folder at `relative`, a path of ordinary names below a watched
    /// folder, as a walk or `strip_prefix` gives one.
    pub fn of(relative: &Path) -> Result<Self, WatchedPathError> {
        let names = relative
            .components()
            .map(|component| match component {
                Component::Normal(name) => Ok(coven::path_bytes(Path::new(name))),
                _ => Err(WatchedPathError(format!(
                    "folder is not a path of names below its watched folder: {}",
                    coven::path_text(relative)
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(names.join(&b'/')))
    }

    /// The folder whose bytes are `bytes`, as a row or the app holds them.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, WatchedPathError> {
        let folder = Self(bytes);
        for name in folder.names() {
            let path = coven::path_from_bytes(name.to_vec()).map_err(|error| {
                WatchedPathError(format!("a folder's name is no name here: {error}"))
            })?;
            let mut components = path.components();
            let one_name = matches!(
                (components.next(), components.next()),
                (Some(Component::Normal(only)), None) if only == path.as_os_str()
            );
            if !one_name {
                return Err(WatchedPathError(format!(
                    "a folder's name is not one ordinary name: {}",
                    coven::path_text(&path)
                )));
            }
        }
        Ok(folder)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The text that shows it: each name's escape (`coven::path_text`),
    /// joined by `/`, as SQL's `coven_path_text` shows the bytes.
    pub fn text(&self) -> String {
        self.names()
            .map(|name| {
                coven::path_text(&coven::path_from_bytes(name.to_vec()).expect(
                    "each name of a relative folder is a path here, checked as it was made",
                ))
            })
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Each name's bytes, outermost first; none for the root.
    fn names(&self) -> impl Iterator<Item = &[u8]> {
        self.0.split(|byte| *byte == b'/').filter(|_| !self.0.is_empty())
    }
}

impl rusqlite::types::ToSql for RelativeFolder {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(rusqlite::types::ValueRef::Blob(&self.0)))
    }
}

impl rusqlite::types::FromSql for RelativeFolder {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Self::from_bytes(value.as_blob()?.to_vec())
            .map_err(|error| rusqlite::types::FromSqlError::Other(Box::new(error)))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl RelativeFolder {
    /// The folder a test spells as `folder`, `/`-separated names.
    pub fn test(folder: &str) -> Self {
        Self::of(Path::new(folder)).expect("a test spells a folder as names")
    }
}

/// The absolute path of `relative` under `root`, spelled as a scan spells it,
/// which is the key a decision about that folder is stored under.
pub fn folder_below(root: &Path, relative: &RelativeFolder) -> PathBuf {
    let mut folder = root.to_path_buf();
    for name in relative.names() {
        folder.push(
            coven::path_from_bytes(name.to_vec())
                .expect("each name of a relative folder is a path here, checked as it was made"),
        );
    }
    folder
}

/// The folder `folder` is below `root`, the inverse of [`folder_below`]: the
/// root for `root` itself, `None` for a folder outside it.
pub(crate) fn folder_relative_below(
    root: &Path,
    folder: &Path,
) -> Result<Option<RelativeFolder>, WatchedPathError> {
    match folder.strip_prefix(root) {
        Ok(relative) => RelativeFolder::of(relative).map(Some),
        Err(_) => Ok(None),
    }
}

/// `bae_folder_below`: the absolute path's bytes of a folder below a watched
/// folder, from the root's bytes and the folder's [`RelativeFolder`] bytes,
/// spelled as a scan spells it, which is how a stored reading names the
/// folder it reads. Takes BLOBs only: rusqlite refuses TEXT for a `Vec<u8>`,
/// so a path bound as text fails rather than matching nothing.
pub static FOLDER_BELOW: SchemaFunction = SchemaFunction {
    name: "bae_folder_below",
    arguments: 2,
    version: 2,
    compute: |arguments| {
        let root = coven::path_from_bytes(arguments.get::<Vec<u8>>(0)?)
            .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        let relative = RelativeFolder::from_bytes(arguments.get::<Vec<u8>>(1)?)
            .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        Ok(Value::Blob(coven::path_bytes(&folder_below(&root, &relative))))
    },
};

/// `bae_folder_relative`: the [`RelativeFolder`] bytes of a folder below a
/// watched folder, empty for the watched folder itself, NULL for one outside
/// it. Takes BLOBs only, as `bae_folder_below` does.
pub static FOLDER_RELATIVE: SchemaFunction = SchemaFunction {
    name: "bae_folder_relative",
    arguments: 2,
    version: 2,
    compute: |arguments| {
        let to_error =
            |error: coven::PathBytesError| rusqlite::Error::UserFunctionError(Box::new(error));
        let root = coven::path_from_bytes(arguments.get::<Vec<u8>>(0)?).map_err(to_error)?;
        let folder = coven::path_from_bytes(arguments.get::<Vec<u8>>(1)?).map_err(to_error)?;
        let relative = folder_relative_below(&root, &folder)
            .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
        Ok(match relative {
            Some(relative) => Value::Blob(relative.as_bytes().to_vec()),
            None => Value::Null,
        })
    },
};

/// The range of paths below `folder`, as stored bytes: from its path and a
/// separator up to, not including, its path and the byte after the
/// separator.
#[cfg(desktop)]
pub fn paths_below(folder: &Path) -> (Vec<u8>, Vec<u8>) {
    let separator = std::path::MAIN_SEPARATOR as u8;
    let mut first = coven::path_bytes(folder);
    if first.last() != Some(&separator) {
        first.push(separator);
    }
    let mut past = first.clone();
    *past.last_mut().expect("the range starts with a separator") = separator + 1;
    (first, past)
}

#[cfg(desktop)]
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
pub fn canonical_absolute_root(path: &Path) -> Result<PathBuf, WatchedPathError> {
    let refuse = |reason: &str| {
        Err(WatchedPathError(format!("watched folder {reason}: {}", coven::path_text(path))))
    };
    if path.components().any(|component| matches!(component, Component::ParentDir)) {
        return refuse("must not contain `..`");
    }
    let canonical: PathBuf = path.components().collect();
    if !canonical.is_absolute() {
        return refuse("must be an absolute path");
    }
    Ok(canonical)
}

#[cfg(desktop)]
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

#[cfg(desktop)]
/// Refuse a stored root that is not in canonical form.
pub fn validate_absolute_root(path: &Path) -> Result<(), WatchedPathError> {
    validate_canonical(path, "watched folder")
}

#[cfg(desktop)]
/// The folder `candidate_path` is below the watched folder at
/// `watched_folder_path`.
pub fn candidate_relative_path(
    watched_folder_path: &Path,
    candidate_path: &Path,
) -> Result<RelativeFolder, WatchedPathError> {
    let relative = candidate_path.strip_prefix(watched_folder_path).map_err(|error| {
        WatchedPathError(format!(
            "{} is outside watched folder {}: {error}",
            coven::path_text(candidate_path),
            coven::path_text(watched_folder_path)
        ))
    })?;
    RelativeFolder::of(relative)
}

#[cfg(desktop)]
/// Refuse a stored folder path that is not in canonical form: folder
/// decisions are keyed by the canonical path, so it is corrupt.
pub fn validate_stored_folder(path: &Path) -> Result<(), WatchedPathError> {
    validate_canonical(path, "folder")
}

/// `posix` as an absolute path on the running host (a `C:` prefix and `\` on
/// Windows, where [`canonical_absolute_root`] refuses a `/`-rooted path).
///
/// Only the rooting changes: a trailing or doubled separator, `.`, or `..`
/// stays in place, so tests of those spellings still exercise them.
#[cfg(any(test, feature = "test-utils"))]
pub fn host_root(posix: &str) -> PathBuf {
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
#[path = "watched_path_tests.rs"]
mod tests;
