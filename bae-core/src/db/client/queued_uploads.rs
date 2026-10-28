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

/// Each release with files this process is sending now, and how many. One
/// row per upload in flight is read, each file found by its key.
pub(super) const RELEASES_SENDING: &str = "SELECT f.release_id, COUNT(*) AS files \
     FROM coven_sending AS sending \
     JOIN release_files AS f \
         ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key \
     GROUP BY f.release_id";

/// The columns a release's upload reads beside `release_listing listing`
/// joined to [`RELEASES_SENDING`] as `sending`, which
/// [`release_upload_in_row`] maps: how many of its files wait, how many are
/// being sent and how many are stored. How far the files being sent have
/// got is the release's transfer.
pub(super) const RELEASE_UPLOAD_COLUMNS: &str = "listing.queued_count AS queued_count, \
     COALESCE(sending.files, 0) AS sending_files, \
     listing.file_count - listing.queued_count AS stored_files";

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
        stored: row.get("stored_files")?,
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
/// queue, in queue order. How far one being sent has got is its file's
/// transfer.
pub(super) fn release_file_uploads_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFileUpload>, DbError> {
    Ok(sql.query(
        "SELECT f.id, upload.file, upload.sending, upload.pending \
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
            })
        },
    )?)
}

/// The upload queue's counts: every queued file, those being sent, and
/// whether uploads are paused, each read by index or in one row.
pub(super) const UPLOAD_QUEUE_COUNTS: &str = "SELECT \
     (SELECT COUNT(*) FROM coven_upload_queue), \
     (SELECT COUNT(*) FROM coven_sending), \
     (SELECT paused FROM coven_upload_pause)";

/// Whether the person paused uploads, as coven's one row says.
const UPLOADS_PAUSED: &str = "SELECT paused FROM coven_upload_pause";

/// Each file this process is sending now and, when it is one of a release's
/// files, its release and size. One row per upload in flight is read.
const FILES_SENDING: &str = "SELECT sending.file, f.release_id, f.file_size \
     FROM coven_sending AS sending \
     LEFT JOIN release_files AS f \
         ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key";

/// Each release with files this process is sending now: of its whole size,
/// the bytes of its files stored, those not queued. Each release is found by
/// its key from the files in flight.
const RELEASES_SENDING_BYTES: &str = "SELECT listing.release_id, \
         listing.total_size - (SELECT SUM(f.file_size) FROM release_files AS f \
             JOIN coven_upload_queue AS queued \
                 ON queued.table_name = 'release_files' AND queued.key = coven_key(f.id) \
             WHERE f.release_id = listing.release_id), \
         listing.total_size \
     FROM release_listing AS listing \
     WHERE listing.release_id IN (SELECT f.release_id FROM coven_sending AS sending \
         JOIN release_files AS f \
             ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key)";

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

    /// How many files wait in this device's upload queue, how many of them
    /// this process is sending now, and whether uploads are paused, live. The
    /// bytes the files move are not read, so a confirmed part reruns nothing.
    pub(crate) fn subscribe_upload_counts(
        &self,
    ) -> coven::LiveQuery<crate::library::UploadQueueCounts> {
        self.inner.handle.subscribe(|sql| {
            Ok(sql.query_row(UPLOAD_QUEUE_COUNTS, [], |row| {
                Ok(crate::library::UploadQueueCounts {
                    queued: row.get(0)?,
                    sending: row.get(1)?,
                    paused: row.get(2)?,
                })
            })?)
        })
    }

    /// The files this process is sending now and the releases they belong
    /// to with the bytes of each stored, live: it reruns as files start and
    /// stop being sent or the person pauses uploads. How far each file has
    /// got is coven's upload watch.
    pub(crate) fn subscribe_sending_uploads(&self) -> coven::LiveQuery<DbSendingUploads> {
        self.inner.handle.subscribe(|sql| {
            let files = sql.query(FILES_SENDING, [], |row| {
                Ok(DbSendingFile {
                    file: row.get(0)?,
                    release_id: row.get(1)?,
                    file_size: row.get(2)?,
                })
            })?;
            let releases = sql.query(RELEASES_SENDING_BYTES, [], |row| {
                Ok(DbSendingRelease {
                    release_id: row.get(0)?,
                    bytes_stored: row.get(1)?,
                    bytes_total: row.get(2)?,
                })
            })?;
            Ok(DbSendingUploads {
                files,
                releases,
                paused: sql.query_row(UPLOADS_PAUSED, [], |row| row.get(0))?,
            })
        })
    }

    /// How far each file this process is sending has got, by file, from
    /// coven's upload watch.
    pub(crate) fn subscribe_upload_transfers(&self) -> coven::TransferWatch {
        self.inner.handle.subscribe_upload_transfers()
    }

    /// Whether the person paused uploads, live, whether or not anything is
    /// queued.
    pub(crate) fn subscribe_uploads_paused(&self) -> coven::LiveQuery<bool> {
        self.inner
            .handle
            .subscribe(|sql| Ok(sql.query_row(UPLOADS_PAUSED, [], |row| row.get(0))?))
    }

    /// Whether the person paused uploads now.
    pub(crate) async fn uploads_paused(&self) -> Result<bool, DbError> {
        self.read(|sql| {
            sql.query_row(UPLOADS_PAUSED, [], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
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
