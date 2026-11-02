//! Watched folders' paths as the store keys them: the one spelling of a
//! watched root, a folder below a watched folder as the bytes the store keys
//! it by, and the absolute path a scan spells for it, which a stored reading
//! of the folder is keyed by. The schema reads them through
//! `bae_folder_below` and `bae_folder_relative`, so they hold wherever the
//! library opens.

use std::path::{Component, Path, PathBuf};

/// A watched root or folder path the store refuses to key by: relative,
/// containing `..`, or not in canonical form.
#[derive(Debug, thiserror::Error)]
#[error("watched folder: {0}")]
pub struct WatchedPathError(String);

impl From<WatchedPathError> for crate::db::DbError {
    fn from(error: WatchedPathError) -> Self {
        Self::Message(error.to_string())
    }
}

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

#[cfg(test)]
impl RelativeFolder {
    /// The folder a test spells as `folder`, `/`-separated names.
    pub(crate) fn test(folder: &str) -> Self {
        Self::of(Path::new(folder)).expect("a test spells a folder as names")
    }
}

/// The absolute path of `relative` under `root`, spelled as a scan spells it,
/// which is the key a decision about that folder is stored under.
pub(crate) fn folder_below(root: &Path, relative: &RelativeFolder) -> PathBuf {
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

/// The range of paths below `folder`, as stored bytes: from its path and a
/// separator up to, not including, its path and the byte after the
/// separator.
#[cfg(desktop)]
pub(crate) fn paths_below(folder: &Path) -> (Vec<u8>, Vec<u8>) {
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
pub(crate) fn canonical_absolute_root(path: &Path) -> Result<PathBuf, WatchedPathError> {
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
pub(crate) fn validate_absolute_root(path: &Path) -> Result<(), WatchedPathError> {
    validate_canonical(path, "watched folder")
}

#[cfg(desktop)]
/// The folder `candidate_path` is below the watched folder at
/// `watched_folder_path`.
pub(crate) fn candidate_relative_path(
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

    /// Bytes a row or the app hands over are a folder below a watched folder
    /// only when each name between the `/`s is one ordinary name.
    #[test]
    fn bytes_that_are_not_names_below_a_root_are_refused() {
        for refused in [&b"a//b"[..], b"a/./b", b"a/../b", b"/a", b"a/"] {
            assert!(
                RelativeFolder::from_bytes(refused.to_vec()).is_err(),
                "{}",
                String::from_utf8_lossy(refused)
            );
        }
        assert!(RelativeFolder::from_bytes(Vec::new()).unwrap().is_root());
        assert_eq!(
            RelativeFolder::from_bytes(b"a/b".to_vec()).unwrap(),
            RelativeFolder::of(Path::new("a/b")).unwrap()
        );
    }

    /// A folder spelled from a scan's relative path goes back to the same
    /// absolute folder, and shows as its names joined by `/`.
    #[test]
    fn a_folder_below_a_root_round_trips() {
        let root = host_root("/music");
        let folder = root.join("Artist").join("Album");
        let relative = folder_relative_below(&root, &folder).unwrap().unwrap();
        assert_eq!(relative.text(), "Artist/Album");
        assert_eq!(folder_below(&root, &relative).as_os_str(), folder.as_os_str());
        assert_eq!(folder_relative_below(&root, &root).unwrap(), Some(RelativeFolder::root()));
        let beside = host_root("/other");
        assert_eq!(folder_relative_below(&root, &beside).unwrap(), None);
    }

    /// A name that isn't valid UTF-8 keys its folder by its bytes and shows
    /// escaped, as SQL's `coven_path_text` shows the same bytes.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_utf8_keeps_its_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"Caf\xE9");
        let relative = RelativeFolder::of(&Path::new("Artist").join(name)).unwrap();
        assert_eq!(relative.as_bytes(), b"Artist/Caf\xE9");
        assert_eq!(relative.text(), "Artist/Caf\\xE9");
        assert_eq!(RelativeFolder::from_bytes(relative.as_bytes().to_vec()).unwrap(), relative);
    }

    /// The range of paths below a folder holds what is inside it and nothing
    /// beside it whose name merely starts the same.
    #[cfg(desktop)]
    #[test]
    fn the_paths_below_a_folder_are_one_byte_range() {
        let folder = host_root("/music/Album");
        let (first, past) = paths_below(&folder);
        let inside = coven::path_bytes(&folder.join("Disc 1"));
        let beside = coven::path_bytes(&host_root("/music/Album 2"));
        assert!(first <= inside && inside < past);
        assert!(!(first <= beside && beside < past));
        assert!(!(first <= coven::path_bytes(&folder) && coven::path_bytes(&folder) < past));
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

        for spelling in
            spellings.iter().map(PathBuf::as_path).chain(URL_SPELLINGS.iter().map(Path::new))
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
                canonical_absolute_root(Path::new(written)).unwrap().as_os_str(),
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

        let spellings =
            [host_root("/music/rips/"), host_root("/music//rips"), host_root("/music/./rips")];

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
