//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;

/// How many of the release `?1`'s files wait in this device's upload queue.
pub(super) const RELEASE_QUEUED_FILE_COUNT: &str = "SELECT COUNT(*) FROM release_files AS f \
     JOIN coven_uploads AS upload \
         ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
     WHERE f.release_id = ?1";

/// Whether any of the release `?1`'s files waits in this device's upload
/// queue.
pub(super) const RELEASE_HAS_QUEUED_FILE: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload \
         ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
     WHERE f.release_id = ?1)";

/// Whether the release file `?1` waits in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_FILE_QUEUED: &str =
    "SELECT EXISTS (SELECT 1 FROM coven_uploads AS upload \
     WHERE upload.table_name = 'release_files' AND upload.key = coven_key(?1))";

/// Each release with files this process is sending now: how many, and the
/// bytes the provider confirmed of them so far and in all. One row per
/// upload in flight is read, each found by its place in the queue.
pub(super) const RELEASES_SENDING: &str = "SELECT f.release_id, COUNT(*) AS files, \
         COALESCE(SUM(upload.bytes_confirmed), 0) AS bytes_confirmed, \
         COALESCE(SUM(upload.bytes_total), 0) AS bytes_total \
     FROM coven_sending AS sending \
     JOIN release_files AS f \
         ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key \
     JOIN coven_uploads AS upload ON upload.position = sending.position \
     GROUP BY f.release_id";

/// The columns a release's upload reads beside `release_listing listing`
/// joined to [`RELEASES_SENDING`] as `sending`, which
/// [`release_upload_in_row`] maps.
pub(super) const RELEASE_UPLOAD_COLUMNS: &str = "listing.queued_count AS queued_count, \
     COALESCE(sending.files, 0) AS sending_files, \
     COALESCE(sending.bytes_confirmed, 0) AS sending_bytes_confirmed, \
     COALESCE(sending.bytes_total, 0) AS sending_bytes_total";

/// The release's upload as [`RELEASE_UPLOAD_COLUMNS`] read it; `None` when
/// none of its files waits.
pub(super) fn release_upload_in_row(row: &Row<'_>) -> rusqlite::Result<Option<DbReleaseUpload>> {
    let queued: i64 = row.get("queued_count")?;
    if queued == 0 {
        return Ok(None);
    }
    Ok(Some(DbReleaseUpload {
        queued,
        sending: row.get("sending_files")?,
        bytes_confirmed: row.get("sending_bytes_confirmed")?,
        bytes_total: row.get("sending_bytes_total")?,
    }))
}

/// What of the release `release_id` waits in this device's upload queue.
pub(super) fn release_upload_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbReleaseUpload>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {RELEASE_UPLOAD_COLUMNS} FROM release_listing listing \
                 LEFT JOIN ({RELEASES_SENDING}) sending ON sending.release_id = listing.release_id \
                 WHERE listing.release_id = ?1"
            ),
            [release_id],
            release_upload_in_row,
        )
        .optional()?
        .flatten())
}

/// Each of the release `release_id`'s files waiting in this device's upload
/// queue, in queue order.
pub(super) fn release_file_uploads_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFileUpload>, DbError> {
    Ok(sql.query(
        "SELECT f.id, upload.file, upload.sending, upload.pending, \
             upload.bytes_confirmed, upload.bytes_total \
         FROM release_files AS f \
         JOIN coven_uploads AS upload \
             ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
         WHERE f.release_id = ?1 \
         ORDER BY upload.position",
        [release_id],
        |row| {
            Ok(DbFileUpload {
                release_file_id: row.get(0)?,
                file: row.get(1)?,
                sending: row.get(2)?,
                pending: row.get(3)?,
                bytes_confirmed: row.get(4)?,
                bytes_total: row.get(5)?,
            })
        },
    )?)
}

impl Database {
    /// How many of the release's files still wait to upload from this device.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_QUEUED_FILE_COUNT, [release_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_HAS_QUEUED_FILE, [release_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether the person paused uploads, live, whether or not anything is
    /// queued.
    #[cfg(desktop)]
    pub(crate) fn subscribe_uploads_paused(&self) -> coven::LiveQuery<bool> {
        self.inner.handle.subscribe(|sql| {
            Ok(
                sql.query_row("SELECT paused FROM coven_upload_pause", [], |row| {
                    row.get(0)
                })?,
            )
        })
    }

    /// Whether an attached release file still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        let file_id = file_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_FILE_QUEUED, [file_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }
}
