//! This device's abandoned attachments (coven §17.2): files it attached whose
//! rows took a newer change before their uploads completed, so no row ever
//! named them.

use super::file_row::{FileRowColumns, FILE_ROW_COLUMNS, FILE_ROW_JOINS};
use super::*;

/// One abandoned attachment, read with the row it was attached to.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AbandonedRow {
    /// The file, which dismissing forgets.
    pub(crate) file: coven::FileId,
    pub(crate) reason: coven::AbandonReason,
    /// The row it was attached to, as the library holds it now.
    pub(crate) file_row: FileRow,
}

impl Database {
    /// This device's abandoned attachments, each with its row, live, in the
    /// order they were abandoned.
    pub(crate) fn subscribe_abandoned_rows(&self) -> coven::LiveQuery<Vec<AbandonedRow>> {
        let query = abandoned_rows_query();
        self.inner.handle.subscribe(move |sql| {
            let rows = sql.query(&query, [], |row| {
                Ok((
                    row.get::<_, coven::FileId>(0)?,
                    row.get::<_, coven::AbandonReason>(1)?,
                    FileRowColumns::read(row, 2)?,
                ))
            })?;
            rows.into_iter()
                .map(|(file, reason, columns)| {
                    let file_row = columns.file_row()?.ok_or_else(|| {
                        DbError::Message(format!("abandoned file {file:?} names no row"))
                    })?;
                    Ok(AbandonedRow {
                        file,
                        reason,
                        file_row,
                    })
                })
                .collect::<Result<_, DbError>>()
                .map_err(CovenError::from)
        })
    }

    /// Forget the records of these abandoned files; ones already gone are
    /// ignored.
    pub(crate) async fn dismiss_abandoned_attachments(
        &self,
        files: &[coven::FileId],
    ) -> Result<(), DbError> {
        if files.is_empty() {
            return Ok(());
        }
        self.inner
            .handle
            .dismiss_abandoned_attachments(files)
            .await
            .map_err(DbError::from)
    }

    /// The files a write changing these rows' files will abandon: this
    /// device's uploads still queued on them. The abandonment is the person's
    /// own change, which [`Self::dismiss_abandoned_attachments`] takes off
    /// the list once the write commits.
    pub(crate) async fn own_abandonments(
        &self,
        rows: &[(&str, String)],
    ) -> Result<Vec<coven::FileId>, DbError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self
            .uploads()
            .await?
            .files
            .into_iter()
            .filter(|upload| {
                rows.iter().any(|(table, key)| {
                    upload.file.table() == *table
                        && upload.file.key() == &coven::RowKey::from(key.as_str())
                })
            })
            .map(|upload| upload.file.id())
            .collect())
    }

    /// Dismiss the records a committed write of the person's own left. The
    /// write stands either way: a record that stays listed is still true, and
    /// the person can dismiss it there.
    pub(crate) async fn dismiss_own_abandonments(&self, files: &[coven::FileId]) {
        if let Err(error) = self.dismiss_abandoned_attachments(files).await {
            tracing::error!(%error, ?files, "could not dismiss the abandonments a write of this device's made");
        }
    }
}

/// Every abandoned attachment with the columns naming its row.
pub(super) fn abandoned_rows_query() -> String {
    format!(
        "SELECT file.file, file.reason, {FILE_ROW_COLUMNS} \
         FROM coven_abandoned AS file {FILE_ROW_JOINS} ORDER BY file.position"
    )
}
