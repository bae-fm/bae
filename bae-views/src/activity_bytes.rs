//! What each Activity transfer line's bar counts from the library's tables:
//! the bytes of the line's batch and how many of them the tables say are done,
//! with the files and releases whose bytes are still on the move. How far
//! those have got changes with every byte, and travels with the transfers.

use crate::sync_activity::DownloadLine;
use bae_records::records::queued_uploads::RELEASE_BYTES_STORED;
use bae_store::read::count_column;
use bae_store::DbError;

/// Each transfer line's batch in bytes, as one read finds it. Bytes moving
/// change none of it, so they rerun nothing; a batch growing, a release's
/// files stored or pinned, and a file starting or ending its upload do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityBytes {
    pub uploads: Option<UploadBytes>,
    pub pins: Option<DownloadBytes>,
    pub moving_to_this_device: Option<DownloadBytes>,
}

/// The Uploading line's batch while it holds a release. A release the library
/// no longer holds counts none of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadBytes {
    /// The whole size of the batch's releases.
    pub bytes_total: u64,
    /// Of that, the bytes of their files stored: those not queued to upload.
    pub bytes_done: u64,
    /// Each file of the batch's releases that this process is sending now.
    pub sending: Vec<SendingFile>,
}

/// A release file this process is sending now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendingFile {
    pub file: coven::FileId,
    pub size: u64,
}

/// The Pinning or Moving to this device line's batch while it holds a
/// release. A release the library no longer holds counts none of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadBytes {
    /// The whole size of the batch's releases.
    pub bytes_total: u64,
    /// Of that, the sizes of the releases that are done: no entry of the
    /// line's kind for them is in the download queue, and they are pinned
    /// whole, or for a move-to-this-device no longer stored in the cloud.
    pub bytes_done: u64,
    /// The releases not done, each with its whole size.
    pub waiting: Vec<WaitingRelease>,
}

/// A release of a download batch that is not done yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingRelease {
    pub release_id: String,
    pub bytes_total: u64,
}

/// Every transfer line's batch in bytes.
pub fn activity_bytes_on(sql: &coven::SqlReadContext<'_>) -> Result<ActivityBytes, DbError> {
    Ok(ActivityBytes {
        uploads: upload_bytes_on(sql)?,
        pins: download_bytes_on(sql, &DownloadLine::PINS)?,
        moving_to_this_device: download_bytes_on(sql, &DownloadLine::MOVING_TO_THIS_DEVICE)?,
    })
}

/// The Uploading line's batch: its size, the bytes stored, and the files being
/// sent. A cover's or a picture's file belongs to no release of the batch.
fn upload_bytes_on(sql: &coven::SqlReadContext<'_>) -> Result<Option<UploadBytes>, DbError> {
    let (releases, bytes_total, bytes_done): (u64, u64, u64) = sql.query_row(
        &format!(
            "SELECT COUNT(*) AS releases, COALESCE(SUM(storage.total_size), 0) AS bytes_total, \
                 COALESCE(SUM({RELEASE_BYTES_STORED}), 0) AS bytes_done \
             FROM upload_batch AS batch \
             LEFT JOIN release_storage AS storage ON storage.release_id = batch.release_id"
        ),
        [],
        batch_counts,
    )?;
    if releases == 0 {
        return Ok(None);
    }
    let sending = sql.query(
        "SELECT sending.file AS file, coven.size AS size \
         FROM coven_sending AS sending \
         JOIN release_files AS f ON f.file = sending.file \
         JOIN upload_batch AS batch ON batch.release_id = f.release_id \
         JOIN coven_files AS coven ON coven.id = sending.file",
        [],
        |row| Ok(SendingFile { file: row.get("file")?, size: count_column(row, "size")? }),
    )?;
    Ok(Some(UploadBytes { bytes_total, bytes_done, sending }))
}

/// A batch's release count, whole size and bytes done, as its aggregate
/// statement names them.
fn batch_counts(row: &rusqlite::Row<'_>) -> rusqlite::Result<(u64, u64, u64)> {
    Ok((
        count_column(row, "releases")?,
        count_column(row, "bytes_total")?,
        count_column(row, "bytes_done")?,
    ))
}

/// `line`'s batch: its size, the sizes of its done releases, and the releases
/// not done.
fn download_bytes_on(
    sql: &coven::SqlReadContext<'_>,
    line: &DownloadLine,
) -> Result<Option<DownloadBytes>, DbError> {
    let done = line.done();
    let (releases, bytes_total, bytes_done): (u64, u64, u64) = sql.query_row(
        &format!(
            "SELECT COUNT(*) AS releases, COALESCE(SUM(storage.total_size), 0) AS bytes_total, \
                 COALESCE(SUM(storage.total_size) FILTER (WHERE {done}), 0) AS bytes_done \
             FROM download_batch AS batch \
             LEFT JOIN release_storage AS storage ON storage.release_id = batch.release_id \
             WHERE batch.kind = ?1"
        ),
        [line.kind],
        batch_counts,
    )?;
    if releases == 0 {
        return Ok(None);
    }
    let waiting = sql.query(
        &format!(
            "SELECT batch.release_id AS release_id, storage.total_size AS bytes_total \
             FROM download_batch AS batch \
             JOIN release_storage AS storage ON storage.release_id = batch.release_id \
             WHERE batch.kind = ?1 AND NOT COALESCE({done}, 0) \
             ORDER BY batch.release_id"
        ),
        [line.kind],
        |row| {
            Ok(WaitingRelease {
                release_id: row.get("release_id")?,
                bytes_total: count_column(row, "bytes_total")?,
            })
        },
    )?;
    Ok(Some(DownloadBytes { bytes_total, bytes_done, waiting }))
}

#[cfg(test)]
#[path = "activity_bytes_tests.rs"]
mod tests;
