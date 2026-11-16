//! Each Activity transfer line's bar as the library's tables give it: the
//! share of the line's batch whose bytes are done, and each file or release
//! whose bytes are on the move now with its share of the batch. How far a
//! file or release on the move has got changes with every byte: it travels on
//! its transfer's stream under the key the bar names, and the view fills each
//! share to it.

use crate::sync_activity::DownloadLine;
use bae_records::records::queued_uploads::RELEASE_BYTES_STORED;
use bae_store::DbError;

/// A transfer line's bar: the share of its batch's bytes done, then each
/// file or release on the move with its own share, keyed as its transfer's
/// stream keys it.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchBar<Key> {
    /// The share of the batch's bytes done, from 0 to 1. A batch of no bytes
    /// has done none of them.
    pub done: f64,
    /// Each file or release whose bytes are on the move now, in the order the
    /// bar draws them after the bytes done.
    pub moving: Vec<MovingShare<Key>>,
}

impl<Key> Default for BatchBar<Key> {
    /// The bar of a batch with nothing done and nothing on the move.
    fn default() -> Self {
        Self { done: 0.0, moving: Vec::new() }
    }
}

/// A file or release on the move: where its segment of the bar starts, and
/// its share of its batch's bytes, which its segment spans once it is done.
#[derive(Debug, Clone, PartialEq)]
pub struct MovingShare<Key> {
    pub key: Key,
    /// Where its segment starts, from 0 to 1: after the bytes done and the
    /// shares of those drawn before it.
    pub start: f64,
    /// Its size over the batch's, from 0 to 1.
    pub share: f64,
}

/// Each transfer line's bar, as one read finds it. Bytes moving change none
/// of it, so they rerun nothing; a batch growing, a release's files stored or
/// pinned, a file starting or ending its upload, and a download starting or
/// ending do. A line whose batch holds no release has an empty bar.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActivityBarRows {
    /// The Uploading line's: the files this process is sending now, by file.
    pub uploads: BatchBar<coven::FileId>,
    /// The Pinning line's: the release downloading now, by release.
    pub pins: BatchBar<String>,
    /// The Moving to this device line's: the release downloading now, by
    /// release.
    pub moving_to_this_device: BatchBar<String>,
}

/// Every transfer line's bar.
pub(crate) fn activity_bars_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<ActivityBarRows, DbError> {
    Ok(ActivityBarRows {
        uploads: upload_bar_on(sql)?,
        pins: download_bar_on(sql, &DownloadLine::PINS)?,
        moving_to_this_device: download_bar_on(sql, &DownloadLine::MOVING_TO_THIS_DEVICE)?,
    })
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

/// The Uploading line's bar: of its batch's whole size, the share of its
/// releases' files stored, and each release file this process is sending
/// now with its size's share. A cover's or a picture's file belongs to no
/// release of the batch.
fn upload_bar_on(sql: &coven::SqlReadContext<'_>) -> Result<BatchBar<coven::FileId>, DbError> {
    let batch_bytes = upload_batch_bytes();
    let done = sql.query_row(
        &format!(
            "WITH {batch_bytes} \
             SELECT COALESCE(CAST(done AS REAL) / NULLIF(total, 0), 0.0) AS done FROM batch_bytes"
        ),
        [],
        |row| row.get("done"),
    )?;
    let moving = sql.query(
        &format!(
            "WITH {batch_bytes}, \
             moving AS (SELECT sending.file AS file, f.release_id AS release_id, \
                 CAST(coven.size AS REAL) / batch_bytes.total AS share \
             FROM coven_sending AS sending \
             JOIN release_files AS f ON f.file = sending.file \
             JOIN upload_batch AS batch ON batch.release_id = f.release_id \
             JOIN coven_files AS coven ON coven.id = sending.file \
             JOIN batch_bytes ON batch_bytes.total > 0) \
             SELECT file, share, CAST(batch_bytes.done AS REAL) / batch_bytes.total \
                 + COALESCE(SUM(share) OVER (ORDER BY release_id, file \
                     ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0.0) AS start \
             FROM moving JOIN batch_bytes \
             ORDER BY release_id, file"
        ),
        [],
        |row| {
            Ok(MovingShare {
                key: row.get("file")?,
                start: row.get("start")?,
                share: row.get("share")?,
            })
        },
    )?;
    Ok(BatchBar { done, moving })
}

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

/// `line`'s bar: of its batch's whole size, the share of its done releases,
/// and the release downloading as the line's kind now with its size's share.
fn download_bar_on(
    sql: &coven::SqlReadContext<'_>,
    line: &DownloadLine,
) -> Result<BatchBar<String>, DbError> {
    let batch_bytes = download_batch_bytes(line);
    let done = sql.query_row(
        &format!(
            "WITH {batch_bytes} \
             SELECT COALESCE(CAST(done AS REAL) / NULLIF(total, 0), 0.0) AS done FROM batch_bytes"
        ),
        [line.kind],
        |row| row.get("done"),
    )?;
    let moving = sql.query(
        &format!(
            "WITH {batch_bytes}, \
             moving AS (SELECT batch.release_id AS release_id, \
                 CAST(storage.total_size AS REAL) / batch_bytes.total AS share \
             FROM download_batch AS batch \
             JOIN release_storage AS storage ON storage.release_id = batch.release_id \
             JOIN download_queue AS queue ON queue.release_id = batch.release_id \
                 AND queue.kind = batch.kind AND queue.state = 'active' \
             JOIN batch_bytes ON batch_bytes.total > 0 \
             WHERE batch.kind = ?1) \
             SELECT release_id, share, CAST(batch_bytes.done AS REAL) / batch_bytes.total \
                 + COALESCE(SUM(share) OVER (ORDER BY release_id \
                     ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING), 0.0) AS start \
             FROM moving JOIN batch_bytes \
             ORDER BY release_id"
        ),
        [line.kind],
        |row| {
            Ok(MovingShare {
                key: row.get("release_id")?,
                start: row.get("start")?,
                share: row.get("share")?,
            })
        },
    )?;
    Ok(BatchBar { done, moving })
}

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
