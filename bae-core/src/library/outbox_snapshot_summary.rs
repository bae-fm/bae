//! Read-only release and summary projections over the cloud-outbox queue.

use std::collections::HashMap;

use super::outbox_snapshot::{OutboxPauseState, OutboxQueue, OutboxState, UploadProgress};

/// The upload queue as a reader that draws its state, not its files, is told
/// it: which releases have uploads queued and how many, and whether it is
/// paused. The files are read for one release at a time, by the Storage
/// Manager's inspector.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxSummary {
    /// The queue's [`OutboxState::revision`], so a reader that skipped
    /// values can still tell an enqueue it was told about has finished.
    pub revision: u64,
    /// Each release with uploads queued, and how many wait and are being
    /// sent. A release leaves when coven has none remaining for it.
    pub per_release: HashMap<String, UploadProgress>,
    pub pause_state: OutboxPauseState,
}

impl OutboxState {
    pub fn summary(&self) -> OutboxSummary {
        OutboxSummary {
            revision: self.revision,
            per_release: self.queue.per_release_progress(),
            pause_state: self.queue.pause_state,
        }
    }
}

impl OutboxQueue {
    pub fn transitioning_release_ids(&self) -> Vec<String> {
        self.upload_groups
            .iter()
            .map(|group| group.release_id.clone())
            .collect()
    }

    pub fn per_release_progress(&self) -> HashMap<String, UploadProgress> {
        self.upload_groups
            .iter()
            .map(|group| (group.release_id.clone(), group.progress.clone()))
            .collect()
    }
}
