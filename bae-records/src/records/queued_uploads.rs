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
/// ([`RELEASE_UPLOAD_COUNT_COLUMNS`]), and the pending records of the
/// release's moves, its failed move's among them, with its originals' paths
/// and names, which name the original a move waits for or failed on. How far
/// the files being sent have got is the release's transfer.
pub const RELEASE_UPLOAD_COLUMNS: &str = concat!(
    release_upload_count_columns!(),
    ", \
     CASE WHEN storage.queued_count > 0 OR storage.move_failed = 1 THEN \
         (SELECT json_group_array(json(CAST(pending.record AS TEXT))) \
          FROM coven_pending AS pending \
          WHERE pending.move IN (SELECT upload.move FROM release_files AS f \
              JOIN coven_uploads AS upload ON upload.file = f.file \
              WHERE f.release_id = storage.release_id \
              UNION \
              SELECT moves.move FROM release_files AS f \
              JOIN coven_moves AS moves ON moves.file = f.file \
              WHERE f.release_id = storage.release_id AND moves.state = 'failed')) \
     END AS upload_move_records, \
     CASE WHEN storage.queued_count > 0 OR storage.move_failed = 1 THEN \
         (SELECT json_group_array(json_object('path', hex(original.path), \
              'name', f.original_filename)) \
          FROM release_files AS f \
          JOIN coven_user_files AS original ON original.file = f.file \
          WHERE f.release_id = storage.release_id) END AS upload_original_names"
);

/// The columns the upload queue's hold reads: whether uploads are paused, how
/// many files are being sent, whether coven says the network is usable (NULL
/// when it cannot tell), whether any upload waits for an unmetered network,
/// and the earliest retry time. [`upload_queue_wait_in_row`] maps them.
pub const UPLOAD_QUEUE_WAIT_COLUMNS: &str =
    "(SELECT paused FROM coven_upload_pause) AS uploads_paused, \
     (SELECT COUNT(*) FROM coven_sending) AS uploads_sending, \
     (SELECT usable FROM coven_network) AS upload_network_usable, \
     (SELECT MAX(metered) FROM coven_uploads) AS uploads_metered, \
     (SELECT MIN(retry_at) FROM coven_uploads) AS uploads_retry_at";

/// What holds the upload queue back, as [`UPLOAD_QUEUE_WAIT_COLUMNS`] read
/// it.
pub fn upload_queue_wait_in_row(row: &Row<'_>) -> rusqlite::Result<Option<UploadQueueWait>> {
    Ok(UploadQueueWait::pick(
        row.get::<_, Option<bool>>("uploads_paused")?.unwrap_or(false),
        bae_store::read::count_column(row, "uploads_sending")?,
        row.get("upload_network_usable")?,
        row.get::<_, Option<bool>>("uploads_metered")?.unwrap_or(false),
        row.get("uploads_retry_at")?,
    ))
}

/// How many of a release's files wait, are being sent and are stored, as
/// [`RELEASE_UPLOAD_COUNT_COLUMNS`] read them; `None` when none of its files
/// is queued. A count below zero, or past a `u32`, is a column-conversion
/// error.
fn upload_counts_in_row(row: &Row<'_>) -> rusqlite::Result<Option<(u32, u32, u32)>> {
    let queued: Option<i64> = row.get("queued_count")?;
    if queued.unwrap_or(0) == 0 {
        return Ok(None);
    }
    Ok(Some((
        bae_store::read::count_column(row, "waiting_files")?,
        bae_store::read::count_column(row, "sending_files")?,
        bae_store::read::count_column(row, "stored_files")?,
    )))
}

/// The release's upload progress as [`RELEASE_UPLOAD_COUNT_COLUMNS`] read
/// it; `None` when none of its files waits.
pub fn release_upload_progress_in_row(row: &Row<'_>) -> rusqlite::Result<Option<UploadProgress>> {
    Ok(upload_counts_in_row(row)?
        .map(|(waiting, sending, _)| UploadProgress { queued: waiting, uploading: sending }))
}

/// The release's upload as [`RELEASE_UPLOAD_COLUMNS`] read it; `None` when
/// none of its files waits.
pub fn release_upload_in_row(row: &Row<'_>) -> rusqlite::Result<Option<DbReleaseUpload>> {
    let Some((waiting, sending, stored)) = upload_counts_in_row(row)? else {
        return Ok(None);
    };
    let missing =
        if sending > 0 { None } else { original_in_row(row, coven::FileSourceFailure::Missing)? };
    Ok(Some(DbReleaseUpload { waiting, sending, stored, missing }))
}

/// The name of the release file whose original changed and failed the
/// release's move, as [`RELEASE_UPLOAD_COLUMNS`] read it.
pub fn changed_original_in_row(row: &Row<'_>) -> rusqlite::Result<Option<String>> {
    original_in_row(row, coven::FileSourceFailure::Changed)
}

/// The name of the release file whose original a move of the release waits
/// for (`Missing`) or failed on (`Changed`), as the move's pending record
/// names the original by its path.
fn original_in_row(
    row: &Row<'_>,
    wanted: coven::FileSourceFailure,
) -> rusqlite::Result<Option<String>> {
    let Some(records) = row.get::<_, Option<String>>("upload_move_records")? else {
        return Ok(None);
    };
    let records: Vec<coven::PendingRecord> = serde_json::from_str(&records).map_err(|error| {
        bae_store::read::column_conversion_error(
            row,
            row.as_ref().column_index("upload_move_records").unwrap_or(0),
            format!("a pending record: {error}"),
        )
    })?;
    let paths: Vec<String> = records
        .iter()
        .filter_map(|record| match &record.reason {
            coven::PendingReason::Original { path, failure } if *failure == wanted => {
                Some(hex::encode(coven::path_bytes(path)))
            }
            _ => None,
        })
        .collect();
    if paths.is_empty() {
        return Ok(None);
    }
    let names: Vec<OriginalName> =
        serde_json::from_str(&row.get::<_, String>("upload_original_names")?).map_err(|error| {
            bae_store::read::column_conversion_error(
                row,
                row.as_ref().column_index("upload_original_names").unwrap_or(0),
                format!("the release's originals: {error}"),
            )
        })?;
    Ok(names
        .into_iter()
        .find(|original| paths.contains(&original.path.to_lowercase()))
        .map(|original| original.name))
}

/// A release file and the path of the original coven holds of it, as
/// `upload_original_names` lists them.
#[derive(serde::Deserialize)]
struct OriginalName {
    /// The path's bytes in hex.
    path: String,
    name: String,
}

/// What of the release `release_id` waits in this device's upload queue.
pub fn release_upload_on(
    sql: &impl coven::SqlRead,
    release_id: &str,
) -> Result<Option<DbReleaseUpload>, DbError> {
    Ok(release_uploads_on(sql, &[release_id.to_string()])?.remove(release_id))
}

/// What of each of `release_ids` waits in this device's upload queue, in one
/// statement. A release none of whose files waits has no entry.
pub fn release_uploads_on(
    sql: &impl coven::SqlRead,
    release_ids: &[String],
) -> Result<HashMap<String, DbReleaseUpload>, DbError> {
    release_rows_on(sql, RELEASE_UPLOAD_COLUMNS, release_ids, release_upload_in_row)
}

/// How many files of each of `release_ids` wait and are being sent, in one
/// statement that reads neither the pending records nor the originals. A
/// release none of whose files waits has no entry.
pub fn release_upload_progress_on(
    sql: &impl coven::SqlRead,
    release_ids: &[String],
) -> Result<HashMap<String, UploadProgress>, DbError> {
    release_rows_on(sql, RELEASE_UPLOAD_COUNT_COLUMNS, release_ids, release_upload_progress_in_row)
}

/// The releases among `release_ids` with a file waiting in the upload queue,
/// each by its id, its `columns` mapped by `map`.
fn release_rows_on<T>(
    sql: &impl coven::SqlRead,
    columns: &str,
    release_ids: &[String],
    map: fn(&Row<'_>) -> rusqlite::Result<Option<T>>,
) -> Result<HashMap<String, T>, DbError> {
    let rows = sql.query(
        &format!(
            "SELECT storage.release_id AS release_id, {columns} \
             FROM release_storage storage \
             LEFT JOIN ({RELEASES_SENDING}) sending ON sending.release_id = storage.release_id \
             WHERE storage.release_id IN (SELECT value FROM json_each(?1)) \
               AND storage.queued_count > 0"
        ),
        [serde_json::json!(release_ids).to_string()],
        |row| {
            let release_id: String = row.get("release_id")?;
            let value = map(row)?.ok_or_else(|| {
                bae_store::read::column_conversion_error(
                    row,
                    "queued_count",
                    format!("release {release_id} waits to upload and has no upload"),
                )
            })?;
            Ok((release_id, value))
        },
    )?;
    Ok(rows.into_iter().collect())
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
