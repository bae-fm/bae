//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the list it opens.

use crate::library::release_queue::ReleaseQueueProgress;
use crate::library::CountLabel;
use crate::library::{
    Arrivals, DownloadQueueContents, OutboxPauseState, OutboxSnapshot, PendingItem,
    SyncStatusSnapshot,
};
use crate::ui::UiError;

/// What sync is doing now: the work that needs the person, the transfers
/// moving bytes, and the files still arriving from other devices.
#[derive(Debug, Clone)]
pub struct SyncActivity {
    /// The sync failure the status line shows, such as being offline.
    pub failure: Option<UiError>,
    /// The pending records that need the person; `None` until first read.
    pub attention: Option<Result<Vec<PendingItem>, UiError>>,
    /// A sync pass is running.
    pub syncing: bool,
    /// The files this device is sending now, if any.
    pub uploads: Option<UploadsInFlight>,
    /// The releases this device is downloading to keep offline, while any is.
    pub downloads: Option<DownloadsInFlight>,
    /// The files still arriving from other devices; `None` until first read.
    pub arrivals: Option<Result<Arrivals, UiError>>,
}

/// The uploads this device is sending, out of its whole queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadsInFlight {
    /// Files being sent now.
    pub sending: u32,
    /// Files queued behind them.
    pub waiting: u32,
    /// Provider-confirmed bytes of the files being sent in parts.
    pub bytes_done: u64,
    pub bytes_total: u64,
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
        ReleaseQueueProgress {
            queued: self.waiting,
            active: self.sending,
            failed: 0,
        }
        .summary_parts("core.queue.uploading")
    }
}

impl DownloadsInFlight {
    /// The list's line: how many releases are downloading, then how many
    /// wait behind them, a zero dropped.
    pub fn summary_parts(&self) -> Vec<CountLabel> {
        ReleaseQueueProgress {
            queued: self.queued,
            active: self.active,
            failed: 0,
        }
        .summary_parts("core.queue.downloading")
    }
}

impl SyncActivity {
    /// Assemble the activity from the sync status, the upload queue and the
    /// offline downloads.
    pub fn new(
        status: &SyncStatusSnapshot,
        outbox: &OutboxSnapshot,
        downloads: &DownloadQueueContents,
    ) -> Self {
        let total = &outbox.total;
        let uploads = (total.uploading > 0).then_some(UploadsInFlight {
            sending: total.uploading,
            waiting: match outbox.pause_state {
                OutboxPauseState::Running => total.queued,
                OutboxPauseState::Paused => 0,
            },
            bytes_done: total.upload_bytes_done,
            bytes_total: total.upload_bytes_total,
        });
        let download_total = downloads.total();
        let downloads = (download_total.active > 0).then_some(DownloadsInFlight {
            active: download_total.active,
            queued: if downloads.paused {
                0
            } else {
                download_total.queued
            },
        });
        Self {
            failure: status.failure(),
            attention: status.pending.clone(),
            syncing: status.syncing(),
            uploads,
            downloads,
            arrivals: status.arrivals.clone(),
        }
    }

    /// Bytes are moving: something uploads or downloads, or files are still
    /// arriving from another device. The indicator spins while they do. A
    /// pass alone doesn't count: one runs every half minute while idle, and
    /// would blink the indicator for nothing.
    pub fn moving(&self) -> bool {
        self.uploads.is_some()
            || self.downloads.is_some()
            || matches!(&self.arrivals, Some(Ok(arrivals)) if !arrivals.is_empty())
    }

    /// Something needs the person: sync failed, a pending record waits on
    /// them, or a list couldn't be read. The indicator shows a warning while
    /// it does.
    pub fn needs_attention(&self) -> bool {
        self.failure.is_some()
            || matches!(&self.attention, Some(Ok(items)) if !items.is_empty())
            || matches!(&self.attention, Some(Err(_)))
            || matches!(&self.arrivals, Some(Err(_)))
    }
}

/// The sync activity, live: a new value whenever the sync status, the
/// upload queue or the offline downloads change.
pub struct SyncActivityStream {
    manager: crate::library::LibraryManager,
    status: tokio::sync::watch::Receiver<SyncStatusSnapshot>,
    outbox: tokio::sync::watch::Receiver<Option<Result<OutboxSnapshot, String>>>,
    downloads: tokio::sync::watch::Receiver<DownloadQueueContents>,
    /// The first value was returned; later ones wait for a change.
    started: bool,
}

impl SyncActivityStream {
    pub(crate) fn new(manager: crate::library::LibraryManager) -> Self {
        Self {
            status: manager.subscribe_sync_status_values(),
            outbox: manager.subscribe_outbox_values(),
            downloads: manager.subscribe_download_queue(),
            started: false,
            manager,
        }
    }

    /// The first value at once, then the next after a change. A source that
    /// fails to read fails the value; a later change reads it again.
    pub async fn next(&mut self) -> Result<SyncActivity, UiError> {
        if self.started {
            tokio::select! {
                changed = self.status.changed() => changed.expect("the sync status outlives its readers"),
                changed = self.outbox.changed() => changed.expect("the upload queue outlives its readers"),
                changed = self.downloads.changed() => changed.expect("the download queue outlives its readers"),
            }
        }
        self.started = true;
        let outbox = self.outbox.borrow_and_update().clone();
        let outbox = match outbox {
            Some(Ok(outbox)) => outbox,
            Some(Err(error)) => return Err(UiError::internal(error)),
            // Nothing has published the queue yet; read it as the upload
            // list does.
            None => self
                .manager
                .outbox_snapshot()
                .await
                .map_err(|error| UiError::diagnostic(error.category(), error))?,
        };
        let status = self.status.borrow_and_update().clone();
        let downloads = self.downloads.borrow_and_update().clone();
        Ok(SyncActivity::new(&status, &outbox, &downloads))
    }
}

#[cfg(test)]
#[path = "sync_activity_tests.rs"]
mod tests;
