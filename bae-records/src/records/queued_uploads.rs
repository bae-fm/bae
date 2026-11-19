//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;
use crate::UploadProgress;
use bae_store::UPLOADS_PAUSED;

/// How many of the release `?1`'s files wait in this device's upload queue.
pub(super) const RELEASE_QUEUED_FILE_COUNT: &str = "SELECT COUNT(*) FROM release_files AS f \
     JOIN coven_uploads AS upload ON upload.file = f.file \
     WHERE f.release_id = ?1";

/// Whether any of the release `?1`'s files waits in this device's upload
/// queue.
#[cfg(any(test, feature = "test-utils"))]
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

/// The counts of a release's upload, beside `release_storage storage` joined
/// to [`RELEASES_SENDING`] as `sending`: how many of its files are queued,
/// how many of those only wait and how many are being sent, and how many are
/// stored. [`release_upload_progress_in_row`] and [`release_upload_in_row`]
/// map them.
macro_rules! release_upload_count_columns {
    () => {
        "storage.queued_count AS queued_count, \
         storage.queued_count - COALESCE(sending.files, 0) AS waiting_files, \
         COALESCE(sending.files, 0) AS sending_files, \
         storage.file_count - storage.queued_count AS stored_files"
    };
}

/// `release_upload_count_columns!` alone, for a read that shows only how
/// many of a release's files wait and how many are being sent.
pub const RELEASE_UPLOAD_COUNT_COLUMNS: &str = release_upload_count_columns!();

/// The columns a release's upload reads beside `release_storage storage`
/// joined to [`RELEASES_SENDING`] as `sending`, which
/// [`release_upload_in_row`] maps: its counts
/// ([`RELEASE_UPLOAD_COUNT_COLUMNS`]), and the name of the release file whose
/// missing original a move of the release waits for, which the release's
/// storage row stores. How far the files being sent have got is the
/// release's transfer.
pub const RELEASE_UPLOAD_COLUMNS: &str =
    concat!(release_upload_count_columns!(), ", storage.missing_original_file");

/// What holds the upload queue back, as one row: `upload_wait` names the
/// line ([`upload_queue_wait_in_row`] maps it), `upload_wait_at` is the
/// earliest time coven gives a file's wait to end, and `uploads_paused` is
/// whether the person paused uploads. Each file's wait is
/// coven's, as `coven_upload_phases` gives it, read once. The first that
/// holds names the line: `paused` while the person paused uploads or sync
/// isn't running; nothing while a file is being sent; `offline` while coven
/// says the device has no usable network; `metered` while a file waits for
/// an unmetered network; `retrying` while a file waits for its retry or a
/// provider's cooldown to end. NULL when nothing holds the queue.
pub const UPLOAD_QUEUE_HOLD: &str = "SELECT CASE \
         WHEN pause.paused OR phases.not_syncing THEN 'paused' \
         WHEN phases.sending THEN NULL \
         WHEN (SELECT usable FROM coven_network) IS 0 THEN 'offline' \
         WHEN phases.metered THEN 'metered' \
         WHEN phases.wait_at IS NOT NULL THEN 'retrying' \
     END AS upload_wait, phases.wait_at AS upload_wait_at, pause.paused AS uploads_paused \
     FROM coven_upload_pause AS pause, (SELECT MAX(wait = 'not_syncing') AS not_syncing, \
             MAX(phase = 'uploading') AS sending, \
             MAX(wait = 'metered') AS metered, \
             MIN(at) AS wait_at \
         FROM coven_upload_phases) AS phases";

/// What holds the upload queue back, as [`UPLOAD_QUEUE_HOLD`] names it.
pub fn upload_queue_wait_in_row(row: &Row<'_>) -> rusqlite::Result<Option<UploadQueueWait>> {
    let Some(wait) = row.get::<_, Option<String>>("upload_wait")? else {
        return Ok(None);
    };
    Ok(Some(match wait.as_str() {
        "paused" => UploadQueueWait::Paused,
        "offline" => UploadQueueWait::Offline,
        "metered" => UploadQueueWait::Metered,
        "retrying" => UploadQueueWait::Retrying { at_ms: row.get("upload_wait_at")? },
        _ => {
            return Err(bae_store::read::column_conversion_error(
                row,
                "upload_wait",
                format!("{wait:?} names no upload queue wait"),
            ))
        }
    }))
}

/// How many of a release's files wait, are being sent and are stored, as
/// [`RELEASE_UPLOAD_COUNT_COLUMNS`] read them; `None` when none of its files
/// is queued. A count below zero, or past a `u32`, is a column-conversion
/// error.
fn upload_counts_in_row(row: &Row<'_>) -> rusqlite::Result<Option<(u32, u32, u32)>> {
    let queued: i64 = row.get("queued_count")?;
    if queued == 0 {
        return Ok(None);
    }
    Ok(Some((
        bae_store::read::count_column(row, "waiting_files")?,
        bae_store::read::count_column(row, "sending_files")?,
        bae_store::read::count_column(row, "stored_files")?,
    )))
}

/// The release's upload progress as [`RELEASE_UPLOAD_COUNT_COLUMNS`] read
/// it, from a statement that left-joins `release_storage storage` only when
/// the release has queued files. `None` when the left join finds no row, so
/// `queued_count` is NULL, or when none of the release's files waits.
pub fn release_upload_progress_in_row(row: &Row<'_>) -> rusqlite::Result<Option<UploadProgress>> {
    if row.get::<_, Option<i64>>("queued_count")?.is_none() {
        return Ok(None);
    }
    Ok(upload_counts_in_row(row)?
        .map(|(waiting, sending, _)| UploadProgress { queued: waiting, uploading: sending }))
}

/// The release's upload as [`RELEASE_UPLOAD_COLUMNS`] read it; `None` when
/// none of its files waits.
pub fn release_upload_in_row(row: &Row<'_>) -> rusqlite::Result<Option<DbReleaseUpload>> {
    let Some((waiting, sending, stored)) = upload_counts_in_row(row)? else {
        return Ok(None);
    };
    let missing = if sending > 0 { None } else { row.get("missing_original_file")? };
    Ok(Some(DbReleaseUpload { waiting, sending, stored, missing }))
}

/// What of the release `release_id` waits in this device's upload queue;
/// `None` when none of its files waits.
pub fn release_upload_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Option<DbReleaseUpload>, DbError> {
    Ok(sql
        .query_row(
            &format!(
                "SELECT {RELEASE_UPLOAD_COLUMNS} FROM release_storage storage \
                 LEFT JOIN ({RELEASES_SENDING}) sending \
                     ON sending.release_id = storage.release_id \
                 WHERE storage.release_id = ?1 AND storage.queued_count > 0"
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

/// Of the release whose `release_storage` row is `storage`, the bytes of its
/// files stored: its whole size less the sizes of its files still queued to
/// upload.
pub const RELEASE_BYTES_STORED: &str = "storage.total_size - COALESCE(( \
         SELECT SUM(coven.size) FROM release_files AS f \
         JOIN coven_upload_queue AS queued ON queued.file = f.file \
         JOIN coven_files AS coven ON coven.id = f.file \
         WHERE f.release_id = storage.release_id), 0)";

/// Each release with files this process is sending now: of its whole size,
/// the bytes of its files stored. Each release is found by its files from
/// the files in flight.
fn releases_sending_bytes() -> String {
    format!(
        "SELECT storage.release_id AS release_id, \
             {RELEASE_BYTES_STORED} AS bytes_stored, \
             storage.total_size AS bytes_total \
         FROM release_storage AS storage \
         WHERE storage.release_id IN (SELECT f.release_id FROM coven_sending AS sending \
             JOIN release_files AS f ON f.file = sending.file)"
    )
}

/// Each file this process is sending now, a cover's or a picture's among
/// them, and of a release's file its release and size.
pub fn sending_files_on(sql: &impl coven::SqlRead) -> Result<Vec<DbSendingFile>, DbError> {
    Ok(sql.query(FILES_SENDING, [], sending_file_in_row)?)
}

/// A file [`FILES_SENDING`] read; a size below zero is a column-conversion
/// error.
pub(super) fn sending_file_in_row(row: &Row<'_>) -> rusqlite::Result<DbSendingFile> {
    Ok(DbSendingFile {
        file: row.get("file")?,
        release_id: row.get("release_id")?,
        file_size: bae_store::read::optional_count_column(row, "file_size")?,
    })
}

/// A release [`releases_sending_bytes`] read; a byte count below zero is a
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
    #[cfg(any(test, feature = "test-utils"))]
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
    ) -> coven::LiveQuery<bool, DbError> {
        let release_id = release_id.to_string();
        self.store.subscribe(move |sql| {
            Ok(sql.query_row(RELEASE_HAS_QUEUED_FILE, [&release_id], |row| row.get(0))?)
        })
    }

    /// The files this process is sending now and the releases they belong
    /// to with the bytes of each stored, live: it reruns as files start and
    /// stop being sent or the person pauses uploads. How far each file has
    /// got is coven's upload watch.
    pub fn subscribe_sending_uploads(&self) -> coven::LiveQuery<DbSendingUploads, DbError> {
        self.store.subscribe(|sql| {
            let files = sending_files_on(&sql)?;
            let releases = sql.query(&releases_sending_bytes(), [], sending_release_in_row)?;
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
