//! The rows that bae-library builds each Activity transfer line's bar from: the share of the
//! line's batch whose bytes are done, and each file or release that bae is
//! uploading or downloading now with its share of the batch. How far a file
//! or release has got changes with every byte and is in no table: a watcher
//! in bae-library adds it to these rows and sends each line's bar.

use bae_store::DbError;
use bae_transfers::DownloadKind;

/// A transfer line's bar as the tables give it: the share of its batch's
/// bytes done, and each file or release that bae is transferring now with its
/// own share, under the key its transfer's progress uses.
#[derive(Debug, Clone, PartialEq)]
pub struct BatchBar<Key> {
    /// The share of the batch's bytes done, from 0 to 1. The share of a batch
    /// with no bytes is 0.
    pub done: f64,
    /// Each file or release that bae is uploading or downloading now.
    pub moving: Vec<MovingShare<Key>>,
}

/// A file or release that bae is transferring now, and its share of its
/// batch's bytes.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct MovingShare<Key> {
    pub key: Key,
    /// Its size divided by the batch's size, from 0 to 1.
    pub share: f64,
}

/// Each transfer line's bar, as one read finds it. Transferred bytes change
/// none of these rows, so coven does not rerun the query for them; coven
/// reruns it when a batch grows, bae stores or pins a release's files, a
/// file starts or ends its upload, or a download starts or ends. A line whose
/// batch holds no release has an empty bar.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityBarRows {
    /// The Uploading line's bar, with each file this process is sending now.
    pub uploads: BatchBar<coven::FileId>,
    /// The Pinning line's bar, with the release downloading now.
    pub pins: BatchBar<String>,
    /// The Moving to this device line's bar, with the release downloading
    /// now.
    pub moving_to_this_device: BatchBar<String>,
}

// The Uploading line's bar: the share of its releases' bytes that bae has
// stored, and each release file this process is sending now with its size's
// share of the batch, in file order. A release the library no longer holds
// counts none of its bytes, and a cover's or a picture's file belongs to no
// release of the batch.
sqlive_compose::fragment!(
    UploadBar {
        upload_done: f64,
        upload_moving: json<Vec<crate::activity_bars::MovingShare<coven::FileId>>>,
    },
    r#"
    SELECT COALESCE(CAST(bytes.done AS REAL) / NULLIF(bytes.total, 0), 0.0) AS upload_done,
           (SELECT json_group_array(json_object(
                       'key', sending.file,
                       'share', CAST(coven.size AS REAL) / bytes.total)
                       ORDER BY sending.file)
            FROM coven_sending AS sending
            JOIN @bae_records::records::spread_fragments::ReleaseFileRow AS file
                ON file.stored_file = sending.file
            JOIN upload_batch AS batch ON batch.release_id = file.release_id
            JOIN coven_files AS coven ON coven.id = sending.file
            WHERE bytes.total > 0) AS upload_moving
    FROM (SELECT SUM(storage.total_size) AS total, SUM(storage.bytes_stored) AS done
          FROM upload_batch AS batch
          CROSS JOIN @bae_records::release_storage::ReleaseStorage AS storage
              ON storage.release_id = batch.release_id) AS bytes
"#
);

// The bar of the download line of `:kind`: the share of its batch's bytes in
// done releases, and the release downloading as the kind now with its size's
// share of the batch.
sqlive_compose::fragment!(
    DownloadBar(kind: String) {
        done_share: f64,
        moving: json<Vec<crate::activity_bars::MovingShare<String>>>,
    },
    r#"
    SELECT COALESCE(CAST(bytes.done AS REAL) / NULLIF(bytes.total, 0), 0.0) AS done_share,
           (SELECT json_group_array(json_object(
                       'key', batched.release_id,
                       'share', CAST(batched.bytes AS REAL) / bytes.total)
                       ORDER BY batched.release_id)
            FROM @crate::download_batches::DownloadBatchRelease AS batched
            JOIN @bae_transfers::download_entry::QueuedDownloadEntry AS queued
                ON queued.release_id = batched.release_id
                    AND queued.kind = batched.kind AND queued.state = 'active'
            WHERE batched.kind = :kind AND batched.held AND bytes.total > 0) AS moving
    FROM (SELECT SUM(sized.bytes) AS total,
                 COALESCE(SUM(sized.bytes) FILTER (WHERE sized.done), 0) AS done
          FROM @crate::download_batches::DownloadBatchRelease AS sized
          WHERE sized.kind = :kind) AS bytes
"#
);

// Every transfer line's bar: one row.
sqlive_compose::read!(
    TransferBars(pins: String, moving_to_this_device: String),
    r#"
    SELECT uploads.*, pins.*, moves.*
    FROM @crate::activity_bars::UploadBar AS uploads
    JOIN @crate::activity_bars::DownloadBar(kind: :pins) AS pins ON 1
    JOIN @crate::activity_bars::DownloadBar(kind: :moving_to_this_device) AS moves ON 1
"#
);

/// Every transfer line's bar as the tables give it.
pub fn activity_bars_on(sql: &coven::SqlReadContext<'_>) -> Result<ActivityBarRows, DbError> {
    let TransferBarsRow { uploads, pins, moves } = bae_store::statement_row(
        sql,
        &TransferBarsParams {
            pins: DownloadKind::Pin.key().to_string(),
            moving_to_this_device: DownloadKind::MoveToThisDevice.key().to_string(),
        },
    )?;
    Ok(ActivityBarRows {
        uploads: BatchBar { done: uploads.upload_done, moving: uploads.upload_moving },
        pins: pins.into_bar(),
        moving_to_this_device: moves.into_bar(),
    })
}

impl DownloadBar {
    fn into_bar(self) -> BatchBar<String> {
        BatchBar { done: self.done_share, moving: self.moving }
    }
}

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
