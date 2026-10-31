//! coven's pending records (coven §19.1) as the sync status lists them: each
//! record from coven's `coven_pending` view, beside what the library holds
//! for the row a file's record is about.

use super::file_row::{FileRowColumns, FILE_ROW_COLUMNS, FILE_ROW_JOINS};
use super::*;

/// One pending record, read with the row its file is the file of.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingRow {
    /// The record, without the error behind it: only this process holds
    /// that, and [`Database::subscribe_pending_causes`] gives it.
    pub(crate) record: coven::PendingRecord,
    /// For a file's record, the row on this device whose file it is; for a
    /// move's, the release file whose original it names. `None` for any
    /// other record, or a file no row here names.
    pub(crate) file_row: Option<FileRow>,
}

impl Database {
    /// Every pending record with the row its file is the file of, live, in
    /// the order the records arose.
    pub(crate) fn subscribe_pending_rows(&self) -> coven::LiveQuery<Vec<PendingRow>> {
        let query = pending_rows_query();
        self.inner.handle.subscribe(move |sql| {
            let rows = sql.query(&query, [], |row| {
                Ok((
                    row.get::<_, coven::PendingRecord>(0)?,
                    FileRowColumns::read(row, 1)?,
                ))
            })?;
            rows.into_iter()
                .map(|(record, columns)| {
                    let file_row = match &record.reason {
                        coven::PendingReason::Original { path, .. } => {
                            moved_original_on(&sql, path)?
                        }
                        _ => columns.file_row()?,
                    };
                    Ok(PendingRow { record, file_row })
                })
                .collect::<Result<_, DbError>>()
                .map_err(CovenError::from)
        })
    }

    /// The errors behind pending records while this process holds them, by
    /// record. A new value follows whenever the error a record would show
    /// appears, changes or ends, which a record's own row doesn't show: a
    /// retry failing for the same reason keeps its record as it was.
    pub(crate) fn subscribe_pending_causes(
        &self,
    ) -> tokio::sync::watch::Receiver<coven::PendingCauses> {
        self.inner.handle.subscribe_pending_causes()
    }
}

/// Every pending record with the columns naming its file's row.
pub(super) fn pending_rows_query() -> String {
    format!(
        "SELECT file.record, {FILE_ROW_COLUMNS} FROM coven_pending AS file {FILE_ROW_JOINS} \
         ORDER BY file.position"
    )
}

/// The release file whose original coven records on this device at `path`,
/// as a move's record names it: what a move of a release's files is about.
/// `None` when no release file here has it.
fn moved_original_on(
    sql: &SqlReadContext<'_>,
    path: &std::path::Path,
) -> Result<Option<FileRow>, DbError> {
    let named = sql
        .query_row(
            "SELECT f.release_id, f.original_filename, album.title \
             FROM coven_user_files AS u \
             JOIN release_files AS f ON f.file = u.file \
             JOIN releases AS r ON r.id = f.release_id \
             JOIN albums AS album ON album.id = r.album_id \
             WHERE u.path = ?1",
            [coven::path_bytes(path)],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    Ok(named.map(
        |(release_id, file_name, album_title)| FileRow::ReleaseFile {
            release_id,
            file_name,
            album_title,
        },
    ))
}
