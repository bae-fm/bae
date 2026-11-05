//! A file's path on this device as the library stores it wherever something
//! reopens the file: coven's bytes for it (`coven::path_bytes`) in a BLOB
//! column, so a name that isn't UTF-8 opens again. Text that shows a stored
//! path comes from the same bytes (`coven::path_text`, or `coven_path_text`
//! in SQL) and never opens a file.
//!
//! One reader for each way a column reaches Rust: its bytes in a row, or its
//! `hex()` inside a JSON document. A query matching the column against a
//! list of paths takes them as JSON text from [`hex_list`] and `unhex`es each.

use crate::DbError;
use std::path::PathBuf;

/// The path a column's bytes record. Bytes this platform never records are a
/// store nothing here wrote.
pub fn decode(bytes: Vec<u8>) -> Result<PathBuf, DbError> {
    coven::path_from_bytes(bytes).map_err(|error| {
        DbError::Message(format!("a stored path does not open on this device: {error}"))
    })
}

/// The path column `index` of `row` records. Bytes this platform never
/// records fail the read: the row is one nothing here wrote.
pub fn get<I: rusqlite::RowIndex>(row: &rusqlite::Row<'_>, index: I) -> rusqlite::Result<PathBuf> {
    Ok(row.get::<_, StoredPath>(index)?.0)
}

/// [`get`] of a column that may be NULL.
pub fn get_optional<I: rusqlite::RowIndex>(
    row: &rusqlite::Row<'_>,
    index: I,
) -> rusqlite::Result<Option<PathBuf>> {
    Ok(row.get::<_, Option<StoredPath>>(index)?.map(|path| path.0))
}

/// A path column's value, read as its bytes. A TEXT value is refused, so a
/// path bound as text by mistake fails rather than reading back.
struct StoredPath(PathBuf);

impl rusqlite::types::FromSql for StoredPath {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        coven::path_from_bytes(value.as_blob()?.to_vec())
            .map(StoredPath)
            .map_err(|error| rusqlite::types::FromSqlError::Other(Box::new(error)))
    }
}

/// The path a column's `hex()` records, as a JSON document holds it.
#[cfg(desktop)]
pub fn from_hex<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let hex: String = serde::Deserialize::deserialize(deserializer)?;
    let bytes = hex::decode(&hex).map_err(D::Error::custom)?;
    coven::path_from_bytes(bytes).map_err(D::Error::custom)
}

/// `paths` as a JSON array of their bytes in hex, for a query that matches a
/// column with `unhex(value)` over `json_each`.
#[cfg(desktop)]
pub fn hex_list<'a>(paths: impl Iterator<Item = &'a std::path::Path>) -> String {
    serde_json::to_string(
        &paths.map(|path| hex::encode(coven::path_bytes(path))).collect::<Vec<_>>(),
    )
    .expect("hex text serializes as JSON")
}
