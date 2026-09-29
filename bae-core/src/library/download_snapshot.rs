//! The in-memory download (pin) queue's types. Its snapshot is the single
//! source of truth for the Downloads pane and the per-release
//! "Downloading..." badge when a pin is queued or in flight.

use super::release_queue::{
    ReleaseQueueContents, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow,
    ReleaseQueueSnapshot, ReleaseQueueState,
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

pub type DownloadState = ReleaseQueueState<DownloadTransferProgress>;
pub type DownloadOp = ReleaseQueueOp<(), DownloadTransferProgress>;
pub type DownloadProgress = ReleaseQueueProgress;
/// The download queue as it stands, before its releases are read.
pub type DownloadQueueContents = ReleaseQueueContents<(), DownloadTransferProgress>;
pub type DownloadRow = ReleaseQueueRow<(), DownloadTransferProgress>;
pub type DownloadSnapshot = ReleaseQueueSnapshot<(), DownloadTransferProgress>;
