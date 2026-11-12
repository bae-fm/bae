//! What a release in the import queue is keyed by: its folder's path as
//! coven's bytes for it (`coven::path_bytes`), the key of the grouping that
//! reads several folders as one release (`grouping:` and an id), or a library
//! release being identified again (`reidentify:` and its id). Bytes,
//! so a folder whose name isn't valid Unicode keys its release apart from
//! every other; the text that shows a key is its escape (`coven::path_text`)
//! and keys nothing.

use std::fmt;
use std::path::Path;

/// A release's key in the import queue, stored as a BLOB.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandidateKey(Vec<u8>);

/// What a grouping's key starts with. A folder's key is an absolute path,
/// which never does.
const GROUPING: &[u8] = b"grouping:";

/// What the key of a library release identified again starts with.
const REIDENTIFY: &[u8] = b"reidentify:";

impl CandidateKey {
    /// The key of the release read from `folder`, an absolute path.
    pub fn folder(folder: &Path) -> Self {
        Self(coven::path_bytes(folder))
    }

    /// The key of the grouping named by `id`.
    pub fn grouping(id: &str) -> Self {
        Self([GROUPING, id.as_bytes()].concat())
    }

    /// The key of the library release `release_id` while it is identified
    /// again, which no folder holds.
    pub fn reidentify(release_id: &str) -> Self {
        Self([REIDENTIFY, release_id.as_bytes()].concat())
    }

    /// Whether this is the key of a library release identified again, which
    /// no folder holds.
    pub(crate) fn names_release(&self) -> bool {
        self.0.starts_with(REIDENTIFY)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The key whose bytes an app holds.
impl From<Vec<u8>> for CandidateKey {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// The key's bytes, as an app holds them.
impl From<CandidateKey> for Vec<u8> {
    fn from(key: CandidateKey) -> Self {
        key.0
    }
}

/// The key as text: a folder's escaped path, or a grouping's or re-identified
/// release's key, whose bytes are its text.
impl fmt::Display for CandidateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match coven::path_from_bytes(self.0.clone()) {
            Ok(path) => f.write_str(&coven::path_text(&path)),
            Err(error) => write!(f, "{} ({error})", hex::encode(&self.0)),
        }
    }
}

impl fmt::Debug for CandidateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CandidateKey({self})")
    }
}

impl rusqlite::types::ToSql for CandidateKey {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::Borrowed(rusqlite::types::ValueRef::Blob(&self.0)))
    }
}

impl rusqlite::types::FromSql for CandidateKey {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        value.as_blob().map(|bytes| Self(bytes.to_vec()))
    }
}

#[cfg(test)]
impl CandidateKey {
    /// The key of the release read from the folder a test spells as `folder`.
    pub(crate) fn test(folder: &str) -> Self {
        Self::folder(Path::new(folder))
    }

    /// The key of the release read from the folder a test spells as the
    /// POSIX path `posix`, rooted for the host the test runs on as
    /// `host_root` roots a watched folder: a scan of `host_root("/watch")`
    /// reads the key `test_rooted("/watch/a")` below it.
    pub(crate) fn test_rooted(posix: &str) -> Self {
        Self::folder(&crate::tables::watched_path::host_root(posix))
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl CandidateKey {
    /// The folder a folder's key names.
    pub fn test_folder(&self) -> std::path::PathBuf {
        coven::path_from_bytes(self.0.clone()).expect("a folder's key is its path")
    }
}
