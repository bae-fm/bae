//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the Activity panel it opens.

use super::*;
use bae_diagnostics::UiError;
use bae_sync_status::SyncStatusSnapshot;
use bae_transfers::{QueueWait, ReleaseQueueCounts};

/// What sync is doing now: sync's failure and the transfer lines, each
/// counting the releases of its batch. Each line's bar is not part of it:
/// the bar changes with each part a provider confirms or a download
/// receives, and travels on the Activity bars' stream.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncActivity {
    /// Sync's failure, such as being offline, which the panel lists under
    /// Needs attention.
    pub failure: Option<UiError>,
    /// coven's sync state is syncing: a pass is moving, or a file is being
    /// sent or downloaded.
    pub syncing: bool,
    /// Bytes are moving: this process is sending a file, a cover or picture
    /// among them, or the download queue is downloading a release. The
    /// indicator spins while they do, not while a queue only holds work. A
    /// pass alone doesn't count: one runs every half minute while idle, and
    /// would blink the indicator for nothing.
    pub moving: bool,
    /// The Uploading line, while its batch holds a release.
    pub uploads: Option<UploadQueueActivity>,
    /// The Pinning line, while its batch holds a release.
    pub pins: Option<DownloadQueueActivity>,
    /// The Moving to this device line, while its batch holds a release. A phone
    /// makes nothing local, so it never has one.
    pub moving_to_this_device: Option<DownloadQueueActivity>,
}

/// The releases a transfer line's batch holds, and how many of them are
/// done. A batch is the releases its line has held since the line last had
/// none waiting or moving, so its total only grows until it starts over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferBatch {
    pub done: u32,
    pub total: u32,
}

impl TransferBatch {
    /// The batch, while it holds a release: its line shows only then.
    fn holding(self) -> Option<Self> {
        (self.total > 0).then_some(self)
    }
}

/// The Uploading line: the releases with files queued to upload since none
/// had any, a release done once every file is stored and none is queued. A
/// cover, artist picture or avatar belongs to no release and counts in no
/// line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueActivity {
    pub batch: TransferBatch,
    /// What holds the upload queue back, shown in place of the count. The
    /// line ends in Resume, not Pause, while the person paused the queue.
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// The Pinning or the Moving to this device line: the releases queued as that kind
/// since none of it waited or downloaded. A pin is done once no pin entry of
/// it is in the queue and every file is pinned; a move-to-this-device once no
/// move-to-this-device entry of it is in the queue and it is no longer stored in the
/// cloud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadQueueActivity {
    pub batch: TransferBatch,
    /// What holds the download queue back, shown in place of the count. Pins
    /// and moves-to-this-device share the queue, so both lines show the same hold.
    pub wait: Option<QueueWait>,
}

impl DownloadQueueActivity {
    /// The person paused the download queue: the line ends in Resume, not
    /// Pause. Pins and moves-to-this-device share the queue, so both lines say so.
    pub fn paused(&self) -> bool {
        self.wait == Some(QueueWait::Paused)
    }
}

/// What the activity reads of the upload queue: the Uploading line's batch,
/// how many files this process is sending now, and what holds the queue back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueCounts {
    pub batch: TransferBatch,
    pub sending: u32,
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// What the activity reads of the download queue: the Pinning and Making
/// local lines' batches beside the queue's counts and what holds it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadQueueCounts {
    pub pins: TransferBatch,
    pub moving_to_this_device: TransferBatch,
    pub queue: ReleaseQueueCounts,
}

impl SyncActivity {
    /// Assemble the activity from the sync status and what the upload and
    /// download queues hold.
    pub fn new(
        status: &SyncStatusSnapshot,
        uploads: &UploadQueueCounts,
        downloads: &DownloadQueueCounts,
    ) -> Self {
        let download_line = |batch: TransferBatch| {
            batch.holding().map(|batch| DownloadQueueActivity { batch, wait: downloads.queue.wait })
        };
        Self {
            failure: status.failure(),
            syncing: status.syncing(),
            moving: uploads.sending > 0 || downloads.queue.total.active > 0,
            uploads: uploads
                .batch
                .holding()
                .map(|batch| UploadQueueActivity { batch, wait: uploads.wait.clone() }),
            pins: download_line(downloads.pins),
            moving_to_this_device: download_line(downloads.moving_to_this_device),
        }
    }

    /// A queue is held and no bytes move: a line shows what holds its queue
    /// in its count's place, whatever holds it (a pause, the network, a
    /// metered network, a retry's time). The indicator shows a pause symbol
    /// while it is, so the hold stays in reach.
    pub fn held(&self) -> bool {
        let uploads_held = self.uploads.as_ref().is_some_and(|uploads| uploads.wait.is_some());
        let download_held = |line: &Option<DownloadQueueActivity>| {
            line.as_ref().is_some_and(|line| line.wait.is_some())
        };
        let downloads_held =
            download_held(&self.pins) || download_held(&self.moving_to_this_device);
        !self.moving && (uploads_held || downloads_held)
    }
}

#[cfg(test)]
#[path = "sync_activity_tests.rs"]
pub(crate) mod tests;

// The Uploading line's batch: the releases it holds, how many of them are
// done (every file stored and none queued), and how many files this process
// is sending now, a cover or a picture among them. The bytes the files move
// are not read, so a confirmed part reruns nothing.
sqlive_compose::fragment!(
    UploadBatchCounts { upload_total: u32, upload_done: u32, uploads_sending: u32 },
    r#"
    SELECT (SELECT COUNT(*) FROM upload_batch) AS upload_total,
           (SELECT COUNT(*)
            FROM upload_batch AS batch
            CROSS JOIN @bae_records::release_storage::ReleaseStorage AS storage
                ON storage.release_id = batch.release_id
            WHERE storage.stored = 1 AND storage.queued_count = 0) AS upload_done,
           (SELECT COUNT(*) FROM coven_sending) AS uploads_sending
"#
);

// The batches of the Pinning and the Moving to this device lines: the
// releases each holds, and how many of them are done.
sqlive_compose::fragment!(
    DownloadBatchCounts { pin_total: u32, pin_done: u32, move_total: u32, move_done: u32 },
    r#"
    SELECT COUNT(*) FILTER (WHERE batched.kind = 'pin') AS pin_total,
           COUNT(*) FILTER (WHERE batched.kind = 'pin' AND batched.done) AS pin_done,
           COUNT(*) FILTER (WHERE batched.kind = 'move_to_this_device') AS move_total,
           COUNT(*) FILTER (WHERE batched.kind = 'move_to_this_device' AND batched.done)
               AS move_done
    FROM @crate::download_batches::DownloadBatchRelease AS batched
"#
);

// The transfer lines' batches, beside what holds the upload queue back and
// the download queue's counts: one row.
sqlive_compose::read!(
    TransferLines,
    r#"
    SELECT uploads.*, downloads.*, held.*, queue.*
    FROM @crate::sync_activity::UploadBatchCounts AS uploads
    JOIN @crate::sync_activity::DownloadBatchCounts AS downloads ON 1
    JOIN @bae_records::records::queued_uploads::UploadQueueHeld AS held ON 1
    JOIN @bae_transfers::queue_counts::StoredDownloadCounts AS queue ON 1
"#
);

/// The activity as one read sees it: the transfer lines' batches and queues,
/// and the sync status the activity takes its failure and state from.
fn sync_activity_on(sql: &coven::SqlReadContext<'_>) -> Result<SyncActivity, bae_store::DbError> {
    let TransferLinesRow { uploads, downloads, held, queue } =
        bae_store::statement_row(sql, &TransferLinesParams {})?;
    let upload_counts = UploadQueueCounts {
        batch: TransferBatch { done: uploads.upload_done, total: uploads.upload_total },
        sending: uploads.uploads_sending,
        wait: held.wait()?,
    };
    let download_counts = DownloadQueueCounts {
        pins: TransferBatch { done: downloads.pin_done, total: downloads.pin_total },
        moving_to_this_device: TransferBatch {
            done: downloads.move_done,
            total: downloads.move_total,
        },
        queue: queue.into_counts(),
    };
    let status = bae_sync_status::sync_status_on(sql)?;
    Ok(SyncActivity::new(&status, &upload_counts, &download_counts))
}

impl LibraryViews {
    /// Follow everything sync is doing: one live query of coven's sync state
    /// joined to the library's rows. Bytes moving change none of what it
    /// reads, so they send nothing.
    pub fn subscribe_sync_activity(
        &self,
    ) -> bae_store::live_results::LiveResults<
        coven::LiveQuery<crate::SyncActivity, bae_store::DbError>,
    > {
        self.store.results(self.store.subscribe(|sql| sync_activity_on(&sql)))
    }
}
