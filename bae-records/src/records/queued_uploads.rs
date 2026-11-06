//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;
use bae_store::UPLOADS_PAUSED;

/// How many of the release `?1`'s files wait in this device's upload queue.
pub(super) const RELEASE_QUEUED_FILE_COUNT: &str = "SELECT COUNT(*) FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file \
     WHERE f.release_id = ?1";

/// Whether any of the release `?1`'s files waits in this device's upload
/// queue.
pub(super) const RELEASE_HAS_QUEUED_FILE: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file \
     WHERE f.release_id = ?1)";

/// Whether the release file `?1` waits in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_FILE_QUEUED: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file WHERE f.id = ?1)";

/// Each release with files this process is sending now, and how many. One
/// row per upload in flight is read, each release file found by its file.
pub const RELEASES_SENDING: &str = "SELECT f.release_id, COUNT(*) AS files \
     FROM coven_sending AS sending \
     JOIN release_files AS f ON f.file = sending.file \
     GROUP BY f.release_id";

/// The columns a release's upload reads beside `release_storage storage`
/// joined to [`RELEASES_SENDING`] as `sending`, which
/// [`release_upload_in_row`] maps: how many of its files are queued, how
/// many of those only wait and how many are being sent, and how many are
/// stored. How far the files being sent have got is the release's transfer.
pub const RELEASE_UPLOAD_COLUMNS: &str = "storage.queued_count AS queued_count, \
     storage.queued_count - COALESCE(sending.files, 0) AS waiting_files, \
     COALESCE(sending.files, 0) AS sending_files, \
     storage.file_count - storage.queued_count AS stored_files";

/// The release's upload as [`RELEASE_UPLOAD_COLUMNS`] read it; `None` when
/// none of its files waits. A count below zero, or past a `u32`, is a
/// column-conversion error.
pub fn release_upload_in_row(row: &Row<'_>) -> rusqlite::Result<Option<DbReleaseUpload>> {
    let queued: i64 = row.get("queued_count")?;
    if queued == 0 {
        return Ok(None);
    }
    Ok(Some(DbReleaseUpload {
        waiting: bae_store::read::count_column(row, "waiting_files")?,
        sending: bae_store::read::count_column(row, "sending_files")?,
        stored: bae_store::read::count_column(row, "stored_files")?,
    }))
}

/// What of the release `release_id` waits in this device's upload queue.
pub fn release_upload_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Option<DbReleaseUpload>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {RELEASE_UPLOAD_COLUMNS} FROM release_storage storage \
                 LEFT JOIN ({RELEASES_SENDING}) sending ON sending.release_id = storage.release_id \
                 WHERE storage.release_id = ?1"
            ),
            [release_id],
            release_upload_in_row,
        )
        .optional()?
        .flatten())
}

/// Each file this process is sending now and, when it is one of a release's
/// files, its release and size. One row per upload in flight is read.
const FILES_SENDING: &str = "SELECT sending.file AS file, f.release_id AS release_id, \
         CASE WHEN f.id IS NOT NULL THEN coven.size END AS file_size \
     FROM coven_sending AS sending \
     LEFT JOIN release_files AS f ON f.file = sending.file \
     LEFT JOIN coven_files AS coven ON coven.id = sending.file";

/// Each release with files this process is sending now: of its whole size,
/// the bytes of its files stored, those not queued. Each release is found by
/// its files from the files in flight.
const RELEASES_SENDING_BYTES: &str = "SELECT storage.release_id AS release_id, \
         storage.total_size - (SELECT SUM(coven.size) FROM release_files AS f \
             JOIN coven_upload_queue AS queued ON queued.file = f.file \
             JOIN coven_files AS coven ON coven.id = f.file \
             WHERE f.release_id = storage.release_id) AS bytes_stored, \
         storage.total_size AS bytes_total \
     FROM release_storage AS storage \
     WHERE storage.release_id IN (SELECT f.release_id FROM coven_sending AS sending \
         JOIN release_files AS f ON f.file = sending.file)";

/// A file [`FILES_SENDING`] read; a size below zero is a column-conversion
/// error.
pub(super) fn sending_file_in_row(row: &Row<'_>) -> rusqlite::Result<DbSendingFile> {
    Ok(DbSendingFile {
        file: row.get("file")?,
        release_id: row.get("release_id")?,
        file_size: bae_store::read::optional_count_column(row, "file_size")?,
    })
}

/// A release [`RELEASES_SENDING_BYTES`] read; a byte count below zero is a
/// column-conversion error.
pub(super) fn sending_release_in_row(row: &Row<'_>) -> rusqlite::Result<DbSendingRelease> {
    Ok(DbSendingRelease {
        release_id: row.get("release_id")?,
        bytes_stored: bae_store::read::count_column(row, "bytes_stored")?,
        bytes_total: bae_store::read::count_column(row, "bytes_total")?,
    })
}

impl Records {
    /// How many of the release's files still wait to upload from this device.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_QUEUED_FILE_COUNT, [release_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_HAS_QUEUED_FILE, [release_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }

    /// [`Self::has_pending_uploads_for_release`], live: it reruns as the
    /// release's files enter and leave the upload queue.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn subscribe_has_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> coven::LiveQuery<bool, coven::CovenError> {
        let release_id = release_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(RELEASE_HAS_QUEUED_FILE, [&release_id], |row| row.get(0))?)
        })
    }

    /// The files this process is sending now and the releases they belong
    /// to with the bytes of each stored, live: it reruns as files start and
    /// stop being sent or the person pauses uploads. How far each file has
    /// got is coven's upload watch.
    pub fn subscribe_sending_uploads(
        &self,
    ) -> coven::LiveQuery<DbSendingUploads, coven::CovenError> {
        self.store.subscribe(|sql| {
            let files = sql.query(FILES_SENDING, [], sending_file_in_row)?;
            let releases = sql.query(RELEASES_SENDING_BYTES, [], sending_release_in_row)?;
            Ok(DbSendingUploads {
                files,
                releases,
                paused: sql.query_row(UPLOADS_PAUSED, [], |row| row.get(0))?,
            })
        })
    }

    /// Whether a release file's coven file still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        let file_id = file_id.to_string();
        self.store
            .read(move |sql| {
                sql.query_row(RELEASE_FILE_QUEUED, [file_id], |row| row.get(0))
                    .map_err(DbError::from)
            })
            .await
    }
}

#[cfg(test)]
#[path = "queued_uploads_tests.rs"]
mod tests;
