//! Everything sync is doing on this device at once, for the activity
//! indicator in the app's title bar and the list it opens.

use crate::library::release_queue::ReleaseQueueProgress;
use crate::library::CountLabel;
use crate::library::{AbandonedItem, PendingItem, ReleaseQueueCounts, SyncStatusSnapshot};
use crate::ui::UiError;

/// What sync is doing now: the work that needs the person and the transfers
/// moving bytes. How many bytes the uploads have moved is not part of it: it
/// changes with each part a provider confirms, and travels with the upload
/// queue's transfers.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncActivity {
    /// The sync failure the status line shows, such as being offline,
    /// unless a pending record in `attention` shows it already.
    pub failure: Option<UiError>,
    /// The pending records that need the person; `None` until first read.
    pub attention: Option<Result<Vec<PendingItem>, UiError>>,
    /// The files this device attached that no row will name, until the
    /// person dismisses them; `None` until first read.
    pub abandoned: Option<Result<Vec<AbandonedItem>, UiError>>,
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
    /// Assemble the activity from the sync status, the upload queue's counts
    /// and the offline downloads.
    pub fn new(
        status: &SyncStatusSnapshot,
        uploads: &UploadQueueCounts,
        downloads: &ReleaseQueueCounts,
    ) -> Self {
        let uploads = (uploads.sending > 0).then_some(UploadsInFlight {
            sending: uploads.sending,
            waiting: if uploads.paused {
                0
            } else {
                uploads.queued - uploads.sending
            },
        });
        let download_total = &downloads.total;
        let downloads = (download_total.active > 0).then_some(DownloadsInFlight {
            active: download_total.active,
            queued: if downloads.paused {
                0
            } else {
                download_total.queued
            },
        });
        Self {
            failure: status.shown_failure(),
            attention: status.pending.clone(),
            abandoned: status.abandoned.clone(),
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

    /// Something needs the person: sync failed, a pending record waits on
    /// them, an upload was abandoned, or a list couldn't be read. The
    /// indicator shows a warning while it does.
    pub fn needs_attention(&self) -> bool {
        self.failure.is_some()
            || matches!(&self.attention, Some(Ok(items)) if !items.is_empty())
            || matches!(&self.attention, Some(Err(_)))
            || matches!(&self.abandoned, Some(Ok(items)) if !items.is_empty())
            || matches!(&self.abandoned, Some(Err(_)))
    }
}

/// The sync activity, live: a new value whenever a change to the sync
/// status, the upload queue's counts or the offline downloads changes it.
/// Bytes moving change none of those, so they send nothing.
pub struct SyncActivityStream {
    status: tokio::sync::watch::Receiver<SyncStatusSnapshot>,
    uploads: coven::LiveQuery<UploadQueueCounts>,
    /// The upload queue's counts as last read; `None` before the first read
    /// and after a failed one.
    upload_counts: Option<UploadQueueCounts>,
    downloads: coven::LiveQuery<ReleaseQueueCounts>,
    /// The download queue's counts as last read; `None` before the first
    /// read and after a failed one.
    download_counts: Option<ReleaseQueueCounts>,
    /// The first value was returned; later ones wait for a change.
    started: bool,
    /// The value last returned; `None` before the first and after a failure.
    returned: Option<SyncActivity>,
}

impl SyncActivityStream {
    pub(crate) fn new(manager: crate::library::LibraryManager) -> Self {
        Self {
            status: manager.subscribe_sync_status_values(),
            uploads: manager.subscribe_upload_counts(),
            upload_counts: None,
            downloads: manager.subscribe_download_counts(),
            download_counts: None,
            started: false,
            returned: None,
        }
    }

    /// The first value at once, then the next that differs from the last
    /// one returned. A source that fails to read fails the value; a later
    /// change reads it again.
    pub async fn next(&mut self) -> Result<SyncActivity, UiError> {
        loop {
            if self.started && self.download_counts.is_some() && self.upload_counts.is_some() {
                tokio::select! {
                    changed = self.status.changed() => changed.expect("the sync status outlives its readers"),
                    counts = self.uploads.next() => self.take_upload_counts(counts)?,
                    counts = self.downloads.next() => self.take_download_counts(counts)?,
                }
            } else {
                if self.download_counts.is_none() {
                    let counts = self.downloads.next().await;
                    self.take_download_counts(counts)?;
                }
                if self.upload_counts.is_none() {
                    let counts = self.uploads.next().await;
                    self.take_upload_counts(counts)?;
                }
            }
            self.started = true;
            let (Some(uploads), Some(downloads)) = (&self.upload_counts, &self.download_counts)
            else {
                unreachable!("both counts are read before the activity");
            };
            let activity = SyncActivity::new(&self.status.borrow_and_update(), uploads, downloads);
            if self.returned.as_ref() != Some(&activity) {
                self.returned = Some(activity.clone());
                return Ok(activity);
            }
        }
    }

    /// Hold the upload queue's counts as their live read delivered them; a
    /// failed read fails the value, and so does the read ending with the
    /// library.
    fn take_upload_counts(
        &mut self,
        counts: Option<coven::CovenResult<UploadQueueCounts>>,
    ) -> Result<(), UiError> {
        match counts {
            Some(Ok(counts)) => {
                self.upload_counts = Some(counts);
                Ok(())
            }
            Some(Err(error)) => {
                self.upload_counts = None;
                self.returned = None;
                let error = crate::library::LibraryError::from(error);
                Err(UiError::diagnostic(error.category(), error))
            }
            None => {
                self.upload_counts = None;
                self.returned = None;
                Err(UiError::internal(
                    "the upload queue's read ended with its library".to_string(),
                ))
            }
        }
    }

    /// Hold the download queue's counts as its live read delivered them; a
    /// failed read fails the value, and so does the read ending with the
    /// library.
    fn take_download_counts(
        &mut self,
        counts: Option<coven::CovenResult<ReleaseQueueCounts>>,
    ) -> Result<(), UiError> {
        match counts {
            Some(Ok(counts)) => {
                self.download_counts = Some(counts);
                Ok(())
            }
            Some(Err(error)) => {
                self.download_counts = None;
                self.returned = None;
                let error = crate::library::LibraryError::from(error);
                Err(UiError::diagnostic(error.category(), error))
            }
            None => {
                self.download_counts = None;
                self.returned = None;
                Err(UiError::internal(
                    "the download queue's read ended with its library".to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "sync_activity_tests.rs"]
mod tests;
