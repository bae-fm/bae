//! The download (pin) queue's types. The queue is the `download_queue`
//! table: the Downloads pane and a release's download status read it, and the
//! bytes the active download moves travel apart, keyed by release.

use super::release_queue::{
    ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot,
    ReleaseQueueState,
};

use super::LibraryError;

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
}

impl DownloadTransferProgress {
    pub fn new(release_id: &str, bytes_done: u64, bytes_total: u64) -> Result<Self, LibraryError> {
        if bytes_total == 0 {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: byte total is zero"
            )));
        }
        if bytes_done > bytes_total {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: bytes done exceeds byte total"
            )));
        }
        if bytes_total > i64::MAX as u64 {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: byte total exceeds display range"
            )));
        }
        Ok(Self {
            bytes_done,
            bytes_total,
            fraction: bytes_done as f64 / bytes_total as f64,
        })
    }
}

/// A release to queue for download.
pub type DownloadEntry = ReleaseQueueEntry<()>;
pub type DownloadProgress = ReleaseQueueProgress;
/// A download as the Downloads pane lists it: its phase without the active
/// one's bytes, which travel apart, keyed by release
/// ([`DownloadTransfersWatch`](super::DownloadTransfersWatch)).
pub type QueuedDownload = ReleaseQueueOp<(), ()>;
pub type DownloadPhase = ReleaseQueueState<()>;
pub type DownloadRow = ReleaseQueueRow<(), ()>;
/// The download queue as the Downloads pane shows it: its entries and their
/// phases, each beside its release. It changes as entries come, go, start or
/// fail, not as bytes arrive.
pub type DownloadSnapshot = ReleaseQueueSnapshot<(), ()>;
