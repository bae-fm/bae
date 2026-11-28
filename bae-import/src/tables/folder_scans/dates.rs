//! When a folder was found, kept by its path on disk: when a scan first saw
//! it, the date its filesystem gives, and whether a scan has read it as a
//! release or as broken.

use super::*;
use crate::folder_scanner::FolderDate;

/// A folder's date as `folder_discovery` stores it: the time, and which of
/// the filesystem's dates it is.
fn date_columns(date: FolderDate) -> (i64, &'static str) {
    match date {
        FolderDate::AddedToDirectory(at) => (at, "added_to_directory"),
        FolderDate::Created(at) => (at, "created"),
    }
}

/// The date [`date_columns`] stored for `folder`, or none when none was.
fn stored_date(
    folder: &Path,
    at: Option<i64>,
    kind: Option<&str>,
) -> Result<Option<FolderDate>, DbError> {
    match (at, kind) {
        (None, None) => Ok(None),
        (Some(at), Some("added_to_directory")) => Ok(Some(FolderDate::AddedToDirectory(at))),
        (Some(at), Some("created")) => Ok(Some(FolderDate::Created(at))),
        stored => Err(DbError::unreadable(
            format!("the stored folder date of {}", coven::path_text(folder)),
            stored,
        )),
    }
}

coven::write!(
    StoreFolderDiscovery(
        folder: Vec<u8>,
        first_seen_at: i64,
        settled: bool,
        source_date: Option<i64>,
        source_date_kind: Option<&'static str>,
    ),
    r#"
    INSERT INTO folder_discovery
        (folder, first_seen_at, settled, source_date, source_date_kind)
    VALUES (:folder, :first_seen_at, :settled, :source_date, :source_date_kind)
    ON CONFLICT (folder) DO UPDATE SET
        first_seen_at = excluded.first_seen_at,
        settled = MAX(settled, excluded.settled),
        source_date = excluded.source_date,
        source_date_kind = excluded.source_date_kind
    WHERE first_seen_at IS NOT excluded.first_seen_at
        OR settled < excluded.settled
        OR source_date IS NOT excluded.source_date
        OR source_date_kind IS NOT excluded.source_date_kind
"#
);

coven::read!(
    writer StoredDiscovery(folder: Vec<u8>) { settled: bool },
    r#"
    SELECT discovery.first_seen_at, discovery.settled, discovery.source_date,
           discovery.source_date_kind
    FROM folder_discovery AS discovery
    WHERE discovery.folder = :folder
"#
);

/// What is known about when a folder was found, kept by its path on disk
/// rather than with the scan row of whichever watched folder covers it.
pub(super) struct FolderDiscovery {
    folder: PathBuf,
    first_seen_at: i64,
    folder_date: Option<FolderDate>,
    /// Whether a scan had read the folder as a release or as broken before.
    settled: bool,
}

impl FolderDiscovery {
    pub(super) fn observe(
        sql: &CovenWrite<'_>,
        folder: &Path,
        observed: Option<FolderDate>,
        now: i64,
    ) -> Result<Self, DbError> {
        let folder = folder.to_path_buf();
        let stored = bae_store::statement_optional(
            sql,
            &StoredDiscoveryParams { folder: coven::path_bytes(&folder) },
        )?;
        let (first_seen_at, settled, stored_date) = match stored {
            Some(stored) => (
                stored.first_seen_at,
                stored.settled,
                stored_date(&folder, stored.source_date, stored.source_date_kind.as_deref())?,
            ),
            None => (now, false, None),
        };
        Ok(Self {
            folder,
            first_seen_at,
            // A date the filesystem could not give now does not erase one it
            // gave before.
            folder_date: observed.or(stored_date),
            settled,
        })
    }

    /// Whether a scan had read the folder as a release or as broken before
    /// this write.
    pub(super) fn settled(&self) -> bool {
        self.settled
    }

    /// Store what is known, marking the folder read as a release or as broken
    /// when `settles`. Writes nothing when nothing changed.
    pub(super) fn store(&self, sql: &CovenWrite<'_>, settles: bool) -> Result<(), DbError> {
        let date = self.folder_date.map(date_columns);
        sql.run(&StoreFolderDiscoveryParams {
            folder: coven::path_bytes(&self.folder),
            first_seen_at: self.first_seen_at,
            settled: self.settled || settles,
            source_date: date.map(|(at, _)| at),
            source_date_kind: date.map(|(_, kind)| kind),
        })?;
        Ok(())
    }
}

coven::write!(
    ForgetDiscoveryAtOrBelow(folder: &'p [u8], below_first: &'p [u8], below_past: &'p [u8]),
    r#"
    DELETE FROM folder_discovery
    WHERE folder = :folder OR (folder >= :below_first AND folder < :below_past)
"#
);

/// Forget when every folder at or below `folders` was found.
pub(crate) fn forget_discovery_at_or_below(
    sql: &CovenWrite<'_>,
    folders: &crate::tables::watched_folders::FoldersAtOrBelow,
) -> Result<(), DbError> {
    sql.run(&ForgetDiscoveryAtOrBelowParams {
        folder: &folders.folder,
        below_first: &folders.below_first,
        below_past: &folders.below_past,
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "dates_tests.rs"]
mod tests;
