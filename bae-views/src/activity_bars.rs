//! Each Activity transfer line's bar as the library's tables give it: the
//! share of the line's batch whose bytes are done, and each file or release
//! that is uploading or downloading now with its share of the batch. How far
//! a transferring file or release has got changes with every byte: core sends
//! it on its transfer's stream under the key the bar names, and the view
//! fills each share to it.

use crate::sync_activity::DownloadLine;
use bae_records::records::queued_uploads::RELEASE_BYTES_STORED;
use bae_store::DbError;

/// A transfer line's bar: the share of its batch's bytes done, then each
/// file or release transferring now with its own share, under the key its
/// transfer's stream uses.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchBar<Key> {
    /// The share of the batch's bytes done, from 0 to 1. A batch of no bytes
    /// has done none of them.
    pub done: f64,
    /// Each file or release that is uploading or downloading now, in the
    /// order the bar draws them after the bytes done.
    pub moving: Vec<MovingShare<Key>>,
}

impl<Key> Default for BatchBar<Key> {
    /// The bar of a batch with nothing done and nothing transferring.
    fn default() -> Self {
        Self { done: 0.0, moving: Vec::new() }
    }
}

/// A file or release that is transferring now: where its segment of the bar
/// starts, and its share of its batch's bytes, which its segment spans once
/// it is done.
#[derive(Debug, Clone, PartialEq)]
pub struct MovingShare<Key> {
    pub key: Key,
    /// Where its segment starts, from 0 to 1: after the bytes done and the
    /// shares of those drawn before it.
    pub start: f64,
    /// Its size over the batch's, from 0 to 1.
    pub share: f64,
}

/// Each transfer line's bar, as one read finds it. Transferred bytes change
/// none of these rows, so coven does not rerun the query for them; coven
/// reruns it when a batch grows, a release's files are stored or pinned, a
/// file starts or ends its upload, or a download starts or ends. A line whose
/// batch holds no release has an empty bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActivityBarRows {
    /// The Uploading line's bar, with one segment for each file this process
    /// is sending now.
    pub uploads: BatchBar<coven::FileId>,
    /// The Pinning line's bar, with a segment for the release downloading
    /// now.
    pub pins: BatchBar<String>,
    /// The Moving to this device line's bar, with a segment for the release
    /// downloading now.
    pub moving_to_this_device: BatchBar<String>,
}

/// Every transfer line's bar.
pub(crate) fn activity_bars_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<ActivityBarRows, DbError> {
    let pins = &DownloadLine::PINS;
    let moving_to_this_device = &DownloadLine::MOVING_TO_THIS_DEVICE;
    Ok(ActivityBarRows {
        uploads: batch_bar_on(sql, &upload_batch_bytes(), UPLOAD_MOVING, &[])?,
        pins: batch_bar_on(sql, &download_batch_bytes(pins), DOWNLOAD_MOVING, &[&pins.kind])?,
        moving_to_this_device: batch_bar_on(
            sql,
            &download_batch_bytes(moving_to_this_device),
            DOWNLOAD_MOVING,
            &[&moving_to_this_device.kind],
        )?,
    })
}

/// A line's bar: of its batch's whole size, the share done, then each key
/// that `moving` names with its size's share, in the order of its release and
/// key. `batch_bytes` is a `batch_bytes` table of the batch's `total` and
/// `done` bytes; `moving` is a `moving` table of each `key`, its
/// `release_id` and its `share` of the batch. `params` are both tables'.
fn batch_bar_on<Key: rusqlite::types::FromSql>(
    sql: &coven::SqlReadContext<'_>,
    batch_bytes: &str,
    moving: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<BatchBar<Key>, DbError> {
    let done = sql.query_row(
        &format!(
            "WITH {batch_bytes} \
             SELECT COALESCE(CAST(done AS REAL) / NULLIF(total, 0), 0.0) AS done FROM batch_bytes"
        ),
        params,
        |row| row.get("done"),
    )?;
    let moving = sql.query(
        &format!(
            "WITH {batch_bytes}, {moving} \
             SELECT key, share, CAST(batch_bytes.done AS REAL) / batch_bytes.total \
                 + COALESCE(SUM(share) OVER (ORDER BY release_id, key \
                     ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0.0) AS start \
             FROM moving JOIN batch_bytes \
             ORDER BY release_id, key"
        ),
        params,
        |row| {
            Ok(MovingShare {
                key: row.get("key")?,
                start: row.get("start")?,
                share: row.get("share")?,
            })
        },
    )?;
    Ok(BatchBar { done, moving })
}

/// The Uploading line's batch's bytes: its releases' whole size and the
/// bytes of their files stored. A release the library no longer holds counts
/// none of its bytes.
fn upload_batch_bytes() -> String {
    format!(
        "batch_bytes AS (SELECT SUM(storage.total_size) AS total, \
             SUM({RELEASE_BYTES_STORED}) AS done \
         FROM upload_batch AS batch \
         JOIN release_storage AS storage ON storage.release_id = batch.release_id)"
    )
}

/// The Uploading line's moving files: each release file this process is
/// sending now, keyed by file, with its size's share of the batch. A cover's
/// or a picture's file belongs to no release of the batch.
const UPLOAD_MOVING: &str = "moving AS (SELECT sending.file AS key, \
         f.release_id AS release_id, \
         CAST(coven.size AS REAL) / batch_bytes.total AS share \
     FROM coven_sending AS sending \
     JOIN release_files AS f ON f.file = sending.file \
     JOIN upload_batch AS batch ON batch.release_id = f.release_id \
     JOIN coven_files AS coven ON coven.id = sending.file \
     JOIN batch_bytes ON batch_bytes.total > 0)";

/// `line`'s batch's bytes: its releases' whole size and the sizes of its done
/// releases. A release the library no longer holds counts none of its bytes.
fn download_batch_bytes(line: &DownloadLine) -> String {
    format!(
        "batch_bytes AS (SELECT SUM(storage.total_size) AS total, \
             COALESCE(SUM(storage.total_size) FILTER (WHERE {}), 0) AS done \
         FROM download_batch AS batch \
         JOIN release_storage AS storage ON storage.release_id = batch.release_id \
         WHERE batch.kind = ?1)",
        line.done()
    )
}

/// A download line's moving release: the release downloading as the line's
/// kind (`?1`) now, keyed by release, with its size's share of the batch.
const DOWNLOAD_MOVING: &str = "moving AS (SELECT batch.release_id AS key, \
         batch.release_id AS release_id, \
         CAST(storage.total_size AS REAL) / batch_bytes.total AS share \
     FROM download_batch AS batch \
     JOIN release_storage AS storage ON storage.release_id = batch.release_id \
     JOIN download_queue AS queue ON queue.release_id = batch.release_id \
         AND queue.kind = batch.kind AND queue.state = 'active' \
     JOIN batch_bytes ON batch_bytes.total > 0 \
     WHERE batch.kind = ?1)";

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
