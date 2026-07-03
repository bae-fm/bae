//! The in-memory export queue snapshot. Single source of truth for the Storage
//! Manager's Exporting pane while a release is being copied out to a user
//! directory.

use std::path::PathBuf;

use super::release_queue::{
    build_release_queue_snapshot, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueSnapshot,
    ReleaseQueueState,
};

pub type ExportState = ReleaseQueueState;
pub type ExportOp = ReleaseQueueOp<PathBuf>;
pub type ExportProgress = ReleaseQueueProgress;
pub type ExportSnapshot = ReleaseQueueSnapshot<PathBuf>;

pub fn build_export_snapshot(exports: &[ExportOp], paused: bool) -> ExportSnapshot {
    build_release_queue_snapshot(exports, paused)
}
