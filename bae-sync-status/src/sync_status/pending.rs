//! coven's pending records (coven §19.1) as the sync status lists them: each
//! record from coven's `coven_pending` table, with the error behind it when
//! coven keeps one, beside what the library holds for the row a file's
//! record is about, or for the release file whose original a move's record
//! names, read in the status query's one statement.

use super::file_row::{FileRowColumns, FILE_ROW_COLUMNS, FILE_ROW_JOINS};
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
/// hold it: the record, its cause, the columns naming its moved original's
/// release file, then those naming its file's row.
pub(super) struct PendingColumns {
    record: Option<coven::PendingRecord>,
    original: (Option<String>, Option<String>, Option<String>),
    file: FileRowColumns,
}

impl PendingColumns {
    /// Read the columns from `first` on.
    pub(super) fn read(row: &Row<'_>, first: usize) -> rusqlite::Result<Self> {
        let mut record = row.get::<_, Option<coven::PendingRecord>>(first)?;
        if let Some(record) = &mut record {
            record.cause = row.get(first + 1)?;
        }
        Ok(Self {
            record,
            original: (row.get(first + 2)?, row.get(first + 3)?, row.get(first + 4)?),
            file: FileRowColumns::read(row, first + 5)?,
        })
    }

    /// The record with the row its file is the file of. `None` when the row
    /// carries no record, as the one row of a status with nothing pending.
    pub(super) fn row(self) -> Result<Option<PendingRow>, DbError> {
        let Some(record) = self.record else {
            return Ok(None);
        };
        let file_row = match (&record.reason, self.original) {
            (
                coven::PendingReason::Original { .. },
                (Some(release_id), Some(file_name), Some(album_title)),
            ) => Some(FileRow::ReleaseFile { release_id, file_name, album_title }),
            (coven::PendingReason::Original { .. }, _) => None,
            _ => self.file.file_row()?,
        };
        Ok(Some(PendingRow { record, file_row }))
    }
}

/// The status query's columns for a pending record, in the order
/// [`PendingColumns`] reads them: the record and its cause, then the
/// original's and the file's.
pub(super) fn pending_columns() -> String {
    format!("file.record, file.cause, {ORIGINAL_COLUMNS}, {FILE_ROW_COLUMNS}")
}

/// The status query's column for the name of the release file that the
/// pending record named by `cause_subject` is about: the original of a
/// move's record, or the file of any other file record. It is `NULL` when
/// no pending record is the cause or no release file here is its file.
pub(super) fn cause_file_column() -> String {
    "MAX(CASE WHEN CAST(file.record AS TEXT)->'$.subject' = shown.cause_subject \
         THEN CASE file.reason_kind WHEN 'original' THEN original_file.original_filename \
             ELSE named_file.original_filename END END) OVER ()"
        .to_string()
}

/// The joins [`pending_columns`] reads, after `coven_pending AS file`.
pub(super) fn pending_joins() -> String {
    format!("{FILE_ROW_JOINS} {ORIGINAL_JOINS}")
}

/// The columns naming the release file whose original a record names in
/// `coven_pending.original_path`: the file's release, its name and its
/// album's title. A move's record names its original there, as the path's
/// bytes that `coven_user_files.path` holds.
const ORIGINAL_COLUMNS: &str =
    "original_file.release_id, original_file.original_filename, original_album.title";

/// The joins [`ORIGINAL_COLUMNS`] reads, each by an index. More than one
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
