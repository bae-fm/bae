//! This device's abandoned attachments (coven §17.2): files it attached whose
//! rows took a newer change before their uploads completed, so no row ever
//! named them.

use super::*;

/// What an abandoned attachment's row names, read when its record is listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AbandonedRow {
    /// A release file whose row is still in the library.
    ReleaseFile {
        file_name: String,
        album_title: String,
    },
    /// A release's cover whose release is still in the library.
    Cover { album_title: String },
    /// A release file or cover whose release is gone.
    DeletedReleaseFile,
    /// An artist's image.
    ArtistImage,
    /// A person's profile picture.
    Avatar,
}

impl Database {
    /// This device's abandoned attachments, live, oldest first.
    pub(crate) fn subscribe_abandoned_attachments(
        &self,
    ) -> coven::LiveQuery<Vec<coven::AbandonedAttachment>> {
        self.inner.handle.subscribe_abandoned_attachments()
    }

    /// Forget these records; ones already gone are ignored.
    pub(crate) async fn dismiss_abandoned_attachments(
        &self,
        records: &[coven::AbandonedAttachment],
    ) -> Result<(), DbError> {
        if records.is_empty() {
            return Ok(());
        }
        self.inner
            .handle
            .dismiss_abandoned_attachments(records)
            .await
            .map_err(DbError::from)
    }

    /// The records a write changing these rows' files will leave for this
    /// device's uploads still queued on them: the person's own change, which
    /// [`Self::dismiss_abandoned_attachments`] takes off the list once the
    /// write commits.
    pub(crate) async fn own_abandonments(
        &self,
        rows: &[(&str, String)],
        reason: coven::AbandonReason,
    ) -> Result<Vec<coven::AbandonedAttachment>, DbError> {
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
            .map(|upload| coven::AbandonedAttachment {
                table: upload.file.table().to_string(),
                key: upload.file.key().clone(),
                file: upload.file.id(),
                reason,
            })
            .collect())
    }

    /// Dismiss the records a committed write of the person's own left. The
    /// write stands either way: a record that stays listed is still true, and
    /// the person can dismiss it there.
    pub(crate) async fn dismiss_own_abandonments(&self, records: &[coven::AbandonedAttachment]) {
        if let Err(error) = self.dismiss_abandoned_attachments(records).await {
            tracing::error!(%error, ?records, "could not dismiss the abandonments a write of this device's made");
        }
    }

    /// What each record's row names, in order. A table bae declares no file
    /// on is a broken record.
    pub(crate) async fn abandoned_rows(
        &self,
        records: &[coven::AbandonedAttachment],
    ) -> Result<Vec<AbandonedRow>, DbError> {
        let keys = records
            .iter()
            .map(|record| {
                Ok((
                    record.table.clone(),
                    row_key_text(&record.table, &record.key)?,
                ))
            })
            .collect::<Result<Vec<_>, DbError>>()?;
        self.read(move |sql| {
            keys.iter()
                .map(|(table, key)| abandoned_row_on(&sql, table, key))
                .collect()
        })
        .await
    }
}

/// The text key of a row in one of bae's file tables, each keyed by one id.
fn row_key_text(table: &str, key: &coven::RowKey) -> Result<String, DbError> {
    match key.values() {
        [rusqlite::types::Value::Text(id)] => Ok(id.clone()),
        key => Err(DbError::Message(format!(
            "abandoned attachment on {table} has unexpected key {key:?}"
        ))),
    }
}

fn abandoned_row_on(
    sql: &SqlReadContext<'_>,
    table: &str,
    key: &str,
) -> Result<AbandonedRow, DbError> {
    Ok(match table {
        "release_files" => sql
            .query_row(
                "SELECT f.original_filename, a.title FROM release_files f \
                 JOIN releases r ON r.id = f.release_id \
                 JOIN albums a ON a.id = r.album_id WHERE f.id = ?",
                [key],
                |row| {
                    Ok(AbandonedRow::ReleaseFile {
                        file_name: row.get(0)?,
                        album_title: row.get(1)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(AbandonedRow::DeletedReleaseFile),
        "covers" => sql
            .query_row(
                "SELECT a.title FROM releases r JOIN albums a ON a.id = r.album_id \
                 WHERE r.id = ?",
                [key],
                |row| {
                    Ok(AbandonedRow::Cover {
                        album_title: row.get(0)?,
                    })
                },
            )
            .optional()?
            .unwrap_or(AbandonedRow::DeletedReleaseFile),
        "artist_images" => AbandonedRow::ArtistImage,
        "people" => AbandonedRow::Avatar,
        other => {
            return Err(DbError::Message(format!(
                "abandoned attachment on {other}, which carries no file"
            )))
        }
    })
}
