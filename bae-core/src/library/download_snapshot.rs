//! The in-memory download (pin) queue snapshot. Single source of truth for the
//! Storage Manager's Downloads pane and the per-release "Downloading..." badge
//! when a pin is queued or in flight.

use super::release_queue::{
    build_release_queue_snapshot, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueSnapshot,
    ReleaseQueueState,
};

pub type DownloadState = ReleaseQueueState;
pub type DownloadOp = ReleaseQueueOp<()>;
pub type DownloadProgress = ReleaseQueueProgress;
pub type DownloadSnapshot = ReleaseQueueSnapshot<()>;

pub fn build_download_snapshot(downloads: &[DownloadOp], paused: bool) -> DownloadSnapshot {
    build_release_queue_snapshot(downloads, paused)
}
