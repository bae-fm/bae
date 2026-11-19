//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the Activity panel it opens.

use super::*;
use bae_diagnostics::UiError;
use bae_records::records::query::RELEASE_STORAGE_PINNED;
use bae_records::records::queued_uploads::{upload_queue_wait_in_row, UPLOAD_QUEUE_HOLD};
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
    /// What holds the upload queue back, shown in place of the count.
    pub wait: Option<bae_records::UploadQueueWait>,
    /// The person paused the upload queue: its line ends in Resume, not
    /// Pause. `wait` reads Paused then too, and also while sync isn't
    /// running, which Resume doesn't change.
    pub paused: bool,
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

/// What the activity reads of the upload queue in one query: the Uploading
/// line's batch, how many files this process is sending now, what holds the
/// queue back, and whether the person paused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueCounts {
    pub batch: TransferBatch,
    pub sending: u32,
    pub wait: Option<bae_records::UploadQueueWait>,
    pub paused: bool,
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
            uploads: uploads.batch.holding().map(|batch| UploadQueueActivity {
                batch,
                wait: uploads.wait.clone(),
                paused: uploads.paused,
            }),
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

/// The activity as one read sees it: the queues' counts, and the sync status
/// the activity takes its failure and state from.
fn sync_activity_on(sql: &coven::SqlReadContext<'_>) -> Result<SyncActivity, bae_store::DbError> {
    let uploads = upload_queue_counts_on(sql)?;
    let downloads = download_queue_counts_on(sql)?;
    let status = bae_sync_status::sync_status_on(sql)?;
    Ok(SyncActivity::new(&status, &uploads, &downloads))
}

/// The SQL creating the Uploading line's batch and its number, and the
/// triggers that keep them.
pub(super) const SQL: &str = include_str!("sync_activity.sql");

/// The Uploading line's batch, how many files this process is sending now,
/// what holds the queue back, and whether the person paused it. The bytes the files move are not read, so
/// a confirmed part reruns nothing.
fn upload_queue_counts_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<UploadQueueCounts, bae_store::DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT (SELECT COUNT(*) FROM upload_batch) AS batch_total, \
                 (SELECT COUNT(*) FROM upload_batch AS batch \
                  JOIN release_storage AS storage ON storage.release_id = batch.release_id \
                  WHERE storage.stored = 1 AND storage.queued_count = 0) AS batch_done, \
                 (SELECT COUNT(*) FROM coven_sending) AS uploads_sending, \
                 hold.upload_wait, hold.upload_wait_at, hold.uploads_paused \
             FROM ({UPLOAD_QUEUE_HOLD}) AS hold"
        ),
        [],
        |row| {
            Ok(UploadQueueCounts {
                batch: transfer_batch_in_row(row)?,
                sending: bae_store::read::count_column(row, "uploads_sending")?,
                wait: upload_queue_wait_in_row(row)?,
                paused: row.get("uploads_paused")?,
            })
        },
    )?)
}

/// The download queue's batches of pins and moves-to-this-device, beside its counts.
fn download_queue_counts_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<DownloadQueueCounts, bae_store::DbError> {
    Ok(DownloadQueueCounts {
        pins: download_batch_on(sql, &DownloadLine::PINS)?,
        moving_to_this_device: download_batch_on(sql, &DownloadLine::MOVING_TO_THIS_DEVICE)?,
        queue: bae_transfers::tables::release_queue_counts_on::<bae_transfers::DownloadRequest>(
            sql,
        )?,
    })
}

/// One kind of download's line: the kind `download_batch` and
/// `download_queue` name it by, and what a release's storage row `storage`
/// says once the kind's work on it is done.
pub(crate) struct DownloadLine {
    pub(crate) kind: &'static str,
    stored_done: &'static str,
}

impl DownloadLine {
    /// The Pinning line: a release is done once every file is pinned.
    pub(crate) const PINS: Self = Self { kind: "pin", stored_done: RELEASE_STORAGE_PINNED };
    /// The Moving to this device line: a release is done once it is no longer stored
    /// in the cloud.
    pub(crate) const MOVING_TO_THIS_DEVICE: Self =
        Self { kind: "move_to_this_device", stored_done: "storage.stored = 0" };

    /// Whether the release of `batch`, a `download_batch` row joined to its
    /// storage row as `storage`, is done: no entry of the line's kind for it
    /// is in the queue, and its storage row says the work is done. A release
    /// the library no longer holds has no storage row, so the condition is
    /// NULL for it: never done.
    pub(crate) fn done(&self) -> String {
        format!(
            "{} AND NOT EXISTS ( \
                 SELECT 1 FROM download_queue AS queue \
                 WHERE queue.release_id = batch.release_id AND queue.kind = batch.kind)",
            self.stored_done
        )
    }
}

/// The batch of `line` in `download_batch`, a release done as
/// [`DownloadLine::done`] says.
fn download_batch_on(
    sql: &coven::SqlReadContext<'_>,
    line: &DownloadLine,
) -> Result<TransferBatch, bae_store::DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT COUNT(*) AS batch_total, \
                 COUNT(*) FILTER (WHERE {}) AS batch_done \
             FROM download_batch AS batch \
             LEFT JOIN release_storage AS storage ON storage.release_id = batch.release_id \
             WHERE batch.kind = ?1",
            line.done()
        ),
        [line.kind],
        transfer_batch_in_row,
    )?)
}

/// A batch as its `batch_total` and `batch_done` columns count it.
fn transfer_batch_in_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TransferBatch> {
    Ok(TransferBatch {
        done: bae_store::read::count_column(row, "batch_done")?,
        total: bae_store::read::count_column(row, "batch_total")?,
    })
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
