//! The in-memory export queue snapshot. Single source of truth for the Storage
//! Manager's Exporting pane while a release is being copied out to a user
//! directory.

use super::release_queue::{
    build_release_queue_snapshot, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueSnapshot,
    ReleaseQueueState,
};
use std::path::PathBuf;

pub type OutputState = ReleaseQueueState<u8>;
pub type OutputOp = ReleaseQueueOp<OutputRequest, u8>;
pub type OutputProgress = ReleaseQueueProgress;
pub type OutputSnapshot = ReleaseQueueSnapshot<OutputRequest, u8>;

#[derive(Debug, Clone)]
pub struct OutputRequest {
    pub target_dir: PathBuf,
    pub kind: OutputKind,
}

/// What a queued release-level output produces at its target dir. `Export`
/// reconstructs the imported file set verbatim; `Save` renders the release
/// through the captured preset. The preset is captured whole at enqueue time so
/// a config edit or delete after enqueue can't change or break a queued save —
/// the worker never re-reads config.
#[derive(Debug, Clone)]
pub enum OutputKind {
    Export,
    Save { preset: crate::config::SavePreset },
}

pub fn build_output_snapshot(exports: &[OutputOp], paused: bool) -> OutputSnapshot {
    build_release_queue_snapshot(exports, paused)
}
