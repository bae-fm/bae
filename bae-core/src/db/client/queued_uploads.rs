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

/// The bytes of the release file `f` its upload `upload` has sent: its size
/// in the share of the upload session the provider confirmed. A file sent in
/// one request has no session, and nothing counts as sent until it lands; a
/// session records its total once the provider confirms its first part.
macro_rules! sent_bytes {
    () => {
        "CASE WHEN upload.bytes_total > 0 \
             THEN CAST(f.file_size * (CAST(upload.bytes_confirmed AS REAL) / upload.bytes_total) AS INTEGER) \
             ELSE 0 END"
    };
}

/// Each release with files this process is sending now: how many, and the
/// bytes of them sent so far. One row per upload in flight is read, each
/// found by its place in the queue.
pub(super) const RELEASES_SENDING: &str = concat!(
    "SELECT f.release_id, COUNT(*) AS files, SUM(",
    sent_bytes!(),
    ") AS bytes_sent \
     FROM coven_sending AS sending \
     JOIN release_files AS f \
         ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key \
     JOIN coven_uploads AS upload ON upload.position = sending.position \
     GROUP BY f.release_id"
);

/// The columns a release's upload reads beside `release_listing listing`
/// joined to [`RELEASES_SENDING`] as `sending`, which
/// [`release_upload_in_row`] maps: how many of its files wait, how many are
/// being sent and how many are stored, and of its whole size, the bytes of its files stored and
/// those sent of the files being sent. The files queued are read only for a
/// release with any.
pub(super) const RELEASE_UPLOAD_COLUMNS: &str = "listing.queued_count AS queued_count, \
     COALESCE(sending.files, 0) AS sending_files, \
     listing.file_count - listing.queued_count AS stored_files, \
     CASE WHEN listing.queued_count > 0 THEN listing.total_size \
         - (SELECT SUM(f.file_size) FROM release_files AS f \
            JOIN coven_upload_queue AS queued \
                ON queued.table_name = 'release_files' AND queued.key = coven_key(f.id) \
            WHERE f.release_id = listing.release_id) \
         + COALESCE(sending.bytes_sent, 0) END AS upload_bytes_done, \
     listing.total_size AS upload_bytes_total";

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
        bytes_done: row.get("upload_bytes_done")?,
        bytes_total: row.get("upload_bytes_total")?,
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
/// queue, in queue order, with its size and the bytes of it sent.
pub(super) fn release_file_uploads_on(
    sql: &SqlReadContext<'_>,
    release_id: &str,
) -> Result<Vec<DbFileUpload>, DbError> {
    Ok(sql.query(
        concat!(
            "SELECT f.id, upload.file, upload.sending, upload.pending, f.file_size, ",
            sent_bytes!(),
            " FROM release_files AS f \
             JOIN coven_uploads AS upload \
                 ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
             WHERE f.release_id = ?1 \
             ORDER BY upload.position"
        ),
        [release_id],
        |row| {
            Ok(DbFileUpload {
                release_file_id: row.get(0)?,
                file: row.get(1)?,
                sending: row.get(2)?,
                pending: row.get(3)?,
                file_size: row.get(4)?,
                bytes_sent: row.get(5)?,
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

/// Each file this process is sending now, its release when it is one of a
/// release's files, and the bytes its upload session has had confirmed. One
/// row per upload in flight is read, each found by its place in the queue.
const FILES_SENDING: &str = "SELECT sending.file, f.release_id, \
         upload.bytes_confirmed, upload.bytes_total \
     FROM coven_sending AS sending \
     JOIN coven_uploads AS upload ON upload.position = sending.position \
     LEFT JOIN release_files AS f \
         ON sending.table_name = 'release_files' AND coven_key(f.id) = sending.key";

/// The bytes confirmed of the files this process is sending now, together
/// and in all, and whether uploads are paused.
const SENDING_TOTAL: &str = "SELECT \
         COALESCE(SUM(upload.bytes_confirmed), 0), COALESCE(SUM(upload.bytes_total), 0), \
         (SELECT paused FROM coven_upload_pause) \
     FROM coven_sending AS sending \
     JOIN coven_uploads AS upload ON upload.position = sending.position";

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

    /// The files this process is sending now and the bytes confirmed of
    /// them, live: it reruns with each part a provider confirms, and as files
    /// start and stop being sent or the person pauses uploads.
    pub(crate) fn subscribe_sending_uploads(&self) -> coven::LiveQuery<DbSendingUploads> {
        self.inner.handle.subscribe(|sql| {
            let files = sql.query(FILES_SENDING, [], |row| {
                Ok(DbSendingFile {
                    file: row.get(0)?,
                    release_id: row.get(1)?,
                    bytes_confirmed: row.get(2)?,
                    bytes_total: row.get(3)?,
                })
            })?;
            Ok(sql.query_row(SENDING_TOTAL, [], |row| {
                Ok(DbSendingUploads {
                    files,
                    bytes_confirmed: row.get(0)?,
                    bytes_total: row.get(1)?,
                    paused: row.get(2)?,
                })
            })?)
        })
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
