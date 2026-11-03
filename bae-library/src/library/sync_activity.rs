//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the list it opens.

use crate::library::{PendingItem, SyncStatusSnapshot};
use bae_diagnostics::UiError;
use bae_transfers::CountLabel;
use bae_transfers::ReleaseQueueCounts;
use bae_transfers::ReleaseQueueProgress;

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
    /// A sync pass is running.
    pub syncing: bool,
    /// The files this device is sending now, if any.
    pub uploads: Option<UploadsInFlight>,
    /// The releases this device is downloading to keep offline, while any is.
    pub downloads: Option<DownloadsInFlight>,
    /// Retry Now is offered: a retry would run a pass, and a failure that
    /// pass can clear holds work back (`SyncStatusSnapshot::retry_offered`).
    pub retry_offered: bool,
}

/// The uploads this device is sending, out of its whole queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadsInFlight {
    /// Files being sent now.
    pub sending: u32,
    /// Files queued behind them.
    pub waiting: u32,
}

/// How many files wait in this device's upload queue, how many of them this
/// process is sending now, and whether the person paused uploads, read from
/// coven's queue in one query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadQueueCounts {
    pub queued: u32,
    pub sending: u32,
    pub paused: bool,
}

/// The releases being downloaded to keep offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadsInFlight {
    /// Releases downloading now.
    pub active: u32,
    /// Releases queued behind them.
    pub queued: u32,
}

impl UploadsInFlight {
    /// The list's line: how many files are being sent, then how many wait
    /// behind them, a zero dropped.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        ReleaseQueueProgress { queued: self.waiting, active: self.sending, failed: 0 }
            .summary_parts("core.queue.uploading")
    }
}

impl DownloadsInFlight {
    /// The list's line: how many releases are downloading, then how many
    /// wait behind them, a zero dropped.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        ReleaseQueueProgress { queued: self.queued, active: self.active, failed: 0 }
            .summary_parts("core.queue.downloading")
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
        let uploads = (uploads.sending > 0).then_some(UploadsInFlight {
            sending: uploads.sending,
            waiting: if uploads.paused { 0 } else { uploads.queued - uploads.sending },
        });
        let download_total = &downloads.total;
        let downloads = (download_total.active > 0).then_some(DownloadsInFlight {
            active: download_total.active,
            queued: if downloads.paused { 0 } else { download_total.queued },
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
    /// spins while they do. A pass alone doesn't count: one runs every half
    /// minute while idle, and would blink the indicator for nothing.
    pub fn moving(&self) -> bool {
        self.uploads.is_some() || self.downloads.is_some()
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

/// Everything sync is doing, as one read of the sync status's tables, the
/// upload queue and the download queue sees it: the counts are read first,
/// since the retry effect's read takes the read whole.
pub(crate) fn sync_activity_on(sql: coven::SqlReadContext<'_>) -> coven::CovenResult<SyncActivity> {
    let uploads = upload_queue_counts_on(&sql)?;
    let downloads = bae_transfers::tables::release_queue_counts_on::<()>(&sql)?;
    let status = crate::library::manager::sync_status_on(sql)?;
    Ok(SyncActivity::new(&status, &uploads, &downloads))
}

/// The upload queue's counts: every queued file, those being sent, and
/// whether uploads are paused, each read by index or in one row.
const UPLOAD_QUEUE_COUNTS: &str = "SELECT \
     (SELECT COUNT(*) FROM coven_upload_queue), \
     (SELECT COUNT(*) FROM coven_sending), \
     (SELECT paused FROM coven_upload_pause)";

/// How many files wait in this device's upload queue, how many of them this
/// process is sending now, and whether uploads are paused. The bytes the
/// files move are not read, so a confirmed part reruns nothing.
fn upload_queue_counts_on(
    sql: &coven::SqlReadContext<'_>,
) -> Result<UploadQueueCounts, bae_db::DbError> {
    Ok(sql.query_row(UPLOAD_QUEUE_COUNTS, [], |row| {
        Ok(UploadQueueCounts { queued: row.get(0)?, sending: row.get(1)?, paused: row.get(2)? })
    })?)
}
