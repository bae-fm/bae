//! The export and save queue's types. The queue is the `output_queue` table:
//! the Storage Manager's inspector and the automation surface's export status
//! read it while a release is copied or saved out to a folder, and the
//! percent the active one has reached travels apart, keyed by release.

use super::release_queue::{
    ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot,
    ReleaseQueueState,
};
use std::path::PathBuf;

pub type OutputState = ReleaseQueueState<u8>;
pub type OutputOp = ReleaseQueueOp<OutputRequest, u8>;
/// A release to queue for export or save.
pub type OutputEntry = ReleaseQueueEntry<OutputRequest>;
pub type OutputProgress = ReleaseQueueProgress;
pub type OutputRow = ReleaseQueueRow<OutputRequest, u8>;
pub type OutputSnapshot = ReleaseQueueSnapshot<OutputRequest, u8>;
/// An export's phase without its percent, which travels as its transfer.
pub type OutputPhase = ReleaseQueueState<()>;
pub type OutputPhaseRow = ReleaseQueueRow<OutputRequest, ()>;
/// The export queue as a reader that takes each export's percent apart sees
/// it: it changes only as entries come, go, start or fail, the queue pauses,
/// or a release it names changes.
pub type OutputPhaseSnapshot = ReleaseQueueSnapshot<OutputRequest, ()>;

#[derive(Debug, Clone, PartialEq)]
pub struct OutputRequest {
    pub target_dir: PathBuf,
    pub kind: OutputKind,
}

/// What a queued release-level output produces at its target dir. `Export`
/// reconstructs the imported file set verbatim; `Save` renders the release
/// through the captured preset. The preset is captured whole at enqueue time so
/// a config edit or delete after enqueue can't change or break a queued save —
/// the worker never re-reads config.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputKind {
    Export,
    Save { preset: crate::config::SavePreset },
}
