//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the list it opens.

use super::*;
use bae_diagnostics::UiError;
use bae_records::records::queued_uploads::{upload_queue_wait_in_row, UPLOAD_QUEUE_WAIT_COLUMNS};
use bae_sync_status::{PendingItem, SyncStatusRows, SyncStatusSnapshot};
use bae_transfers::ReleaseQueueCounts;
use bae_transfers::ReleaseQueueProgress;
use bae_types::count_label::CountLabel;

/// What sync is doing now: the work that needs the person and the transfers
/// moving bytes. How many bytes the uploads have moved is not part of it: it
/// changes with each part a provider confirms, and travels with the upload
/// queue's transfers.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncActivity {
    /// The sync failure the status line shows, such as being offline,
    /// unless a pending record in `attention` shows it already.
    pub failure: Option<UiError>,
    /// The pending records that need the person.
    pub attention: Vec<PendingItem>,
    /// coven's sync state is syncing: a pass is moving, or a file is being
    /// sent or downloaded.
    pub syncing: bool,
    /// The files in this device's upload queue, while any waits or is sent.
    pub uploads: Option<UploadQueueActivity>,
    /// The releases in this device's download queue, while any waits or is
    /// downloading.
    pub downloads: Option<DownloadQueueActivity>,
    /// Retry now is offered: a retry would run a pass, and a failure that
    /// pass can clear holds work back (`SyncStatusSnapshot::retry_offered`).
    pub retry_offered: bool,
}

/// The files in this device's upload queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueActivity {
    /// Files being sent now.
    pub sending: u32,
    /// Files queued behind them.
    pub waiting: u32,
    /// What holds the queue back.
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// How many files wait in this device's upload queue, how many of them this
/// process is sending now, and what holds the queue back, read from coven's
/// queue in one query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueCounts {
    pub queued: u32,
    pub sending: u32,
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// The releases in this device's download queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadQueueActivity {
    /// Releases downloading now.
    pub active: u32,
    /// Releases queued behind them.
    pub queued: u32,
    /// What holds the queue back.
    pub wait: Option<bae_transfers::QueueWait>,
}

impl UploadQueueActivity {
    /// The list's line: how many files are being sent, then how many wait
    /// behind them, a zero dropped.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        ReleaseQueueProgress { queued: self.waiting, active: self.sending, failed: 0 }
            .summary_parts("core.queue.uploading", "core.queue.queued_to_upload")
    }
}

impl DownloadQueueActivity {
    /// The list's line: how many releases are pinning, then how many wait
    /// behind them, a zero dropped.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        ReleaseQueueProgress { queued: self.queued, active: self.active, failed: 0 }
            .download_summary_parts()
    }
}

impl SyncActivity {
    /// Assemble the activity from the sync status, the upload queue's counts
    /// and the offline downloads.
    pub fn new(
        status: &SyncStatusSnapshot,
        uploads: &UploadQueueCounts,
        downloads: &ReleaseQueueCounts,
    ) -> Self {
        let uploads = (uploads.queued > 0).then(|| UploadQueueActivity {
            sending: uploads.sending,
            waiting: uploads.queued - uploads.sending,
            wait: uploads.wait.clone(),
        });
        let download_total = &downloads.total;
        let downloads =
            (download_total.active + download_total.queued > 0).then_some(DownloadQueueActivity {
                active: download_total.active,
                queued: download_total.queued,
                wait: downloads.wait,
            });
        Self {
            failure: status.shown_failure(),
            attention: status.pending.clone(),
            syncing: status.syncing(),
            uploads,
            downloads,
            retry_offered: status.retry_offered(),
        }
    }

    /// Bytes are moving: something uploads or downloads. The indicator
    /// spins while they do, not while a queue only holds work. A pass alone
    /// doesn't count: one runs every half minute while idle, and would blink
    /// the indicator for nothing.
    pub fn moving(&self) -> bool {
        self.uploads.as_ref().is_some_and(|uploads| uploads.sending > 0)
            || self.downloads.as_ref().is_some_and(|downloads| downloads.active > 0)
    }

    /// Something needs the person: sync failed, or a pending record waits
    /// on them. The indicator shows a warning while it does.
    pub fn needs_attention(&self) -> bool {
        self.failure.is_some() || !self.attention.is_empty()
    }
}

#[cfg(test)]
#[path = "sync_activity_tests.rs"]
mod tests;

/// What the activity reads of the library's tables in one go, beside
/// coven's sync state: the upload and download queues' counts and the sync
/// status's rows.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncActivityRows {
    uploads: UploadQueueCounts,
    downloads: ReleaseQueueCounts,
    status: SyncStatusRows,
}

/// The activity's rows as one read sees them: the counts are read first,
/// since the retry effect's read takes the read whole.
fn sync_activity_rows_on(
    sql: coven::SqlReadContext<'_>,
) -> Result<SyncActivityRows, bae_store::DbError> {
    let uploads = upload_queue_counts_on(&sql)?;
    let downloads =
        bae_transfers::tables::release_queue_counts_on::<bae_transfers::DownloadRequest>(&sql)?;
    let status = bae_sync_status::sync_status_rows_on(sql)?;
    Ok(SyncActivityRows { uploads, downloads, status })
}

/// The activity from coven's sync state and the rows read beside it.
fn sync_activity(state: coven::SyncStatus, rows: SyncActivityRows) -> SyncActivity {
    let SyncActivityRows { uploads, downloads, status } = rows;
    SyncActivity::new(&SyncStatusSnapshot::read(state, status), &uploads, &downloads)
}

/// The SQL creating the count of queued files and the triggers that keep it.
pub(super) const SQL: &str = include_str!("sync_activity.sql");

/// How many files wait in this device's upload queue, how many of them this
/// process is sending now, and what holds the queue back. The bytes the
/// files move are not read, so a confirmed part reruns nothing.
fn upload_queue_counts_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<UploadQueueCounts, bae_store::DbError> {
    Ok(sql.query_row(
        &format!(
            "SELECT (SELECT queued FROM upload_queue_count) AS uploads_queued, \
                 {UPLOAD_QUEUE_WAIT_COLUMNS}"
        ),
        [],
        |row| {
            Ok(UploadQueueCounts {
                queued: row.get("uploads_queued")?,
                sending: bae_store::read::count_column(row, "uploads_sending")?,
                wait: upload_queue_wait_in_row(row)?,
            })
        },
    )?)
}

impl LibraryViews {
    /// Follow everything sync is doing: coven's sync state beside one read
    /// of the library's rows, each value made from the latest of both. Bytes
    /// moving change none of what it reads, so they send nothing.
    pub fn subscribe_sync_activity(
        &self,
    ) -> bae_store::WithSyncState<SyncActivityRows, crate::SyncActivity> {
        self.store.with_sync_state(sync_activity_rows_on, sync_activity)
    }
}
