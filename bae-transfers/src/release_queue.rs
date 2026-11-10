//! What the transfer queues hold and show. Downloading for offline and
//! exporting or saving each work through one release at a time from a queue
//! that can be paused, cancelled and retried. Each queue is a table of this
//! device's ([`bae_store`]): its entries in the order queued, each one's
//! phase, and whether the queue is paused. How far the active entry has got
//! moves too often to be written, and travels apart from the table, keyed by
//! release (the library's release transfers watch).
//!
//! `Extra` is what an entry carries beyond its release: nothing for a
//! download, the target folder and what to write there for an output.
//!
//! An entry names its release by id only. What the pane shows of the release —
//! its title, file count, and size — is read with the queue, so an edit to the
//! release shows on its row while it waits.

use bae_types::count_label::CountLabel;
use std::collections::BTreeMap;

/// Where an entry stands. `Progress` is how far an active one has got:
/// `()` where a reader takes progress apart from the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseQueueState<Progress> {
    Queued,
    Active { progress: Progress },
    Failed { error: String },
}

/// A release to queue, with what its queue carries for it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueEntry<Extra> {
    pub release_id: String,
    /// When it was queued, in milliseconds since the epoch.
    pub created_at: i64,
    pub payload: Extra,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueOp<Extra, Progress> {
    pub release_id: String,
    pub created_at: i64,
    pub payload: Extra,
    pub state: ReleaseQueueState<Progress>,
}

/// What the library holds for a queued release, as the pane shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRelease {
    /// The release's album title.
    pub title: String,
    pub file_count: u64,
    /// Total size in bytes across the release's files.
    pub total_size: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// What keeps a queue from taking its next entry. A pause comes before the
/// network: a paused queue reads Paused whatever the network does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueWait {
    /// The person paused the queue.
    Paused,
    /// The device has no usable network, as coven says.
    Offline,
}

impl QueueWait {
    /// What holds a queue that is `paused` (or not) when coven says the
    /// network is `network_usable` (`None` when it cannot tell, which holds
    /// nothing back).
    pub fn of(paused: bool, network_usable: Option<bool>) -> Option<Self> {
        if paused {
            Some(Self::Paused)
        } else if network_usable == Some(false) {
            Some(Self::Offline)
        } else {
            None
        }
    }
}

/// How many entries a queue holds in each phase, and what holds it back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueCounts {
    pub total: ReleaseQueueProgress,
    pub wait: Option<QueueWait>,
}

impl ReleaseQueueCounts {
    /// Whether the person paused the queue.
    pub fn paused(&self) -> bool {
        self.wait == Some(QueueWait::Paused)
    }
}

/// One count of a release transfer's summary line, and where the files it
/// counts stand: on the move, waiting their turn, or done. Core decides which
/// counts appear, in what order, and that a zero drops out
/// (`transfer_summary`); the UI words each and draws it by its stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferCount {
    pub label: CountLabel,
    pub stage: TransferStage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferStage {
    Moving,
    Waiting,
    Done,
}

/// A release transfer's summary from its counts, in order, each with its
/// catalog key and stage, each dropped when its count is zero.
pub fn transfer_summary<const N: usize>(
    counts: [(&str, u32, TransferStage); N],
) -> Vec<TransferCount> {
    counts
        .into_iter()
        .filter_map(|(key, count, stage)| {
            CountLabel::nonzero(key, count).map(|label| TransferCount { label, stage })
        })
        .collect()
}

impl ReleaseQueueProgress {
    /// The summary parts for a release queue: its active verb (`active_key`,
    /// e.g. pinning vs uploading), then failed, then queued (`queued_key`) —
    /// each part dropped when its count is zero. The order and the drop rule
    /// are the decision; only the two verbs differ by queue, so the caller
    /// names them.
    pub fn summary_parts(&self, active_key: &str, queued_key: &str) -> Vec<CountLabel> {
        [(active_key, self.active), ("core.queue.failed", self.failed), (queued_key, self.queued)]
            .into_iter()
            .filter_map(|(key, count)| CountLabel::nonzero(key, count))
            .collect()
    }

    /// The Pinning screen's line: the summary parts of a queue of pins, in
    /// pinning's words.
    pub fn pin_summary_parts(&self) -> Vec<CountLabel> {
        self.summary_parts("core.queue.pinning", "core.queue.queued_to_pin")
    }
}

/// One queue entry as the pane shows it: the entry, and its release as the
/// library holds it now — `None` once the library no longer has it, since the
/// entry is still in the queue to cancel.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueRow<Extra, Progress> {
    pub op: ReleaseQueueOp<Extra, Progress>,
    pub release: Option<QueuedRelease>,
}

/// The queue as the pane shows it: every entry beside its release, the
/// per-state counts, and whether the queue is paused.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueSnapshot<Extra, Progress> {
    pub rows: Vec<ReleaseQueueRow<Extra, Progress>>,
    pub total: ReleaseQueueProgress,
    pub paused: bool,
}

/// How many entries a queue holds, and the windows its pane draws of them in
/// the order queued, each beside its release.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueWindows<Extra> {
    pub windows: Vec<bae_store::LibraryBrowseWindow<ReleaseQueueRow<Extra, ()>>>,
    pub total_count: u64,
}

/// One release's entries in the download and export queues: what the
/// storage inspector shows of the release.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueEntries {
    pub download: Option<crate::DownloadRow>,
    pub output: Option<crate::OutputPhaseRow>,
}

impl<Extra> ReleaseQueueSnapshot<Extra, ()> {
    /// The same rows with the active one's progress from `progress`, which
    /// holds an entry for every release the queue read as active. `None` when
    /// it lacks one.
    pub fn with_progress<Progress: Clone>(
        self,
        progress: &BTreeMap<String, Progress>,
    ) -> Option<ReleaseQueueSnapshot<Extra, Progress>> {
        let rows = self
            .rows
            .into_iter()
            .map(|ReleaseQueueRow { op, release }| {
                let state = match op.state {
                    ReleaseQueueState::Queued => ReleaseQueueState::Queued,
                    ReleaseQueueState::Active { progress: () } => ReleaseQueueState::Active {
                        progress: progress.get(&op.release_id)?.clone(),
                    },
                    ReleaseQueueState::Failed { error } => ReleaseQueueState::Failed { error },
                };
                Some(ReleaseQueueRow {
                    op: ReleaseQueueOp {
                        release_id: op.release_id,
                        created_at: op.created_at,
                        payload: op.payload,
                        state,
                    },
                    release,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ReleaseQueueSnapshot { rows, total: self.total, paused: self.paused })
    }
}

#[cfg(test)]
#[path = "release_queue_tests.rs"]
mod tests;
