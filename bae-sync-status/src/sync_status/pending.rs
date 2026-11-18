//! coven's pending records (coven §19.1) as the sync status lists them: each
//! record from coven's `coven_pending` table, with the error behind it when
//! coven keeps one, beside what the library holds for the row a file's
//! record is about, or for the release file whose original a move's record
//! names, read in the status query's one statement.

use super::file_row::{file_row_columns, release_file_columns, FileRowColumns, FILE_ROW_JOINS};
use super::*;

/// One pending record, read with the row its file is the file of.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingRow {
    /// The record, with the text of the error behind it as `coven_pending`
    /// keeps it: written by the run that met the error, cleared at open.
    pub record: coven::PendingRecord,
    /// For a file's record, the row on this device whose file it is; for a
    /// move's, the release file whose original it names. `None` for any
    /// other record, or a file no row here names.
    pub file_row: Option<FileRow>,
}

/// The pending record a row of the status query carries, as its columns
/// hold it: the record, its cause, then the columns naming its file's row.
pub(super) struct PendingColumns {
    record: Option<coven::PendingRecord>,
    file: FileRowColumns,
}

impl PendingColumns {
    /// Read the columns from `first` on.
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        let mut record = row.get::<_, Option<coven::PendingRecord>>(first)?;
        if let Some(record) = &mut record {
            record.cause = row.get(first + 1)?;
        }
        Ok(Self { record, file: FileRowColumns::read(row, first + 2)? })
    }

    /// The record with the row its file is the file of. `None` when the row
    /// carries no record, as the one row of a status with nothing pending.
    pub(super) fn row(self) -> Result<Option<PendingRow>, DbError> {
        let Some(record) = self.record else {
            return Ok(None);
        };
        let file_row = match record.reason {
            coven::PendingReason::Original { .. } => self.file.original_release_file(),
            _ => self.file.file_row()?,
        };
        Ok(Some(PendingRow { record, file_row }))
    }
}

/// The status query's columns for a pending record, in the order
/// [`PendingColumns`] reads them: the record and its cause, then the file's.
pub(super) fn pending_columns() -> String {
    format!("file.record, file.cause, {}", file_row_columns())
}

/// The status query's column for the name of the release file that the
/// pending record named by `cause_subject` is about: the same release file
/// the pending list shows for that record. It is `NULL` when no pending
/// record is the cause or the list shows no release file for it.
pub(super) fn cause_file_column() -> String {
    let file_name = release_file_columns().file_name;
    format!(
        "MAX(CASE WHEN CAST(file.record AS TEXT)->'$.subject' = shown.cause_subject \
             THEN {file_name} END) OVER ()"
    )
}

/// The joins [`pending_columns`] reads, after `coven_pending AS file`.
pub(super) fn pending_joins() -> String {
    format!("{FILE_ROW_JOINS} {ORIGINAL_JOINS}")
}

/// The joins the original release file's columns read, each by an index. More than one
/// coven file can record the same original path, so the subquery picks the
/// release file with the smallest id, and each record stays one row.
const ORIGINAL_JOINS: &str = "\
     LEFT JOIN release_files AS original_file ON original_file.id = ( \
         SELECT original_release_file.id \
         FROM coven_user_files AS original_user_file \
         JOIN release_files AS original_release_file \
             ON original_release_file.file = original_user_file.file \
         WHERE original_user_file.path = file.original_path \
         ORDER BY original_release_file.id LIMIT 1) \
     LEFT JOIN releases AS original_release ON original_release.id = original_file.release_id \
     LEFT JOIN albums AS original_album ON original_album.id = original_release.album_id";
