//! What the transfer queues hold and show. Downloading for offline and
//! exporting or saving each work through one release at a time from a queue
//! that can be paused, cancelled and retried. Each queue is a table of this
//! device's ([`crate::db`]): its entries in the order queued, each one's
//! phase, and whether the queue is paused. How far the active entry has got
//! moves too often to be written, and travels apart from the table, keyed by
//! release ([`crate::library::ReleaseTransfersWatch`]).
//!
//! `Extra` is what an entry carries beyond its release: nothing for a
//! download, the target folder and what to write there for an output.
//!
//! An entry names its release by id only. What the pane shows of the release —
//! its title, file count, and size — is read with the queue, so an edit to the
//! release shows on its row while it waits.

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
    pub file_count: i64,
    /// Total size in bytes across the release's files.
    pub total_size: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// How many entries a queue holds in each phase, and whether it is paused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseQueueCounts {
    pub total: ReleaseQueueProgress,
    pub paused: bool,
}

/// One part of a queue summary line: a catalog key and the count to render it
/// with (e.g. `core.queue.downloading` × 2). Core decides which parts appear, in
/// what order, and that a zero drops out; the UI resolves each key against its
/// catalog and joins the parts. Written five times across the apps before this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountLabel {
    pub key: String,
    pub count: u32,
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
pub(crate) fn transfer_summary<const N: usize>(
    counts: [(&str, u32, TransferStage); N],
) -> Vec<TransferCount> {
    counts
        .into_iter()
        .filter(|(_, count, _)| *count > 0)
        .map(|(key, count, stage)| TransferCount {
            label: CountLabel {
                key: key.to_string(),
                count,
            },
            stage,
        })
        .collect()
}

impl ReleaseQueueProgress {
    /// The summary parts for a release queue: its active verb (`active_key`,
    /// e.g. downloading vs uploading), then failed, then queued — each part
    /// dropped when its count is zero. The order and the drop rule are the
    /// decision; only the active verb differs by queue, so the caller names it.
    pub fn summary_parts(&self, active_key: &str) -> Vec<CountLabel> {
        [
            (active_key, self.active),
            ("core.queue.failed", self.failed),
            ("core.queue.queued", self.queued),
        ]
        .into_iter()
        .filter(|(_, count)| *count > 0)
        .map(|(key, count)| CountLabel {
            key: key.to_string(),
            count,
        })
        .collect()
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
    pub windows: Vec<super::LibraryBrowseWindow<ReleaseQueueRow<Extra, ()>>>,
    pub total_count: u64,
}

/// One release's entries in the download and export queues: what the
/// storage inspector shows of the release.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseQueueEntries {
    pub download: Option<super::DownloadRow>,
    pub output: Option<super::OutputPhaseRow>,
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
        Some(ReleaseQueueSnapshot {
            rows,
            total: self.total,
            paused: self.paused,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The summary parts read active-then-failed-then-queued, drop any zero, and
    /// take the active verb the caller names — so every queue line shares this
    /// one decision and differs only in that verb.
    /// A release transfer's summary keeps its order and leaves out every
    /// count that is zero, so a release with nothing being sent reads only
    /// what waits and what is done.
    #[test]
    fn a_transfer_summary_leaves_out_zero_counts() {
        let summary = transfer_summary([
            ("core.queue.uploading", 0, TransferStage::Moving),
            ("core.queue.queued", 4, TransferStage::Waiting),
            ("core.queue.stored", 105, TransferStage::Done),
        ]);
        assert_eq!(
            summary
                .iter()
                .map(|count| (count.label.key.as_str(), count.label.count, count.stage))
                .collect::<Vec<_>>(),
            vec![
                ("core.queue.queued", 4, TransferStage::Waiting),
                ("core.queue.stored", 105, TransferStage::Done),
            ]
        );
    }

    #[test]
    fn summary_parts_order_drop_and_active_verb() {
        let progress = ReleaseQueueProgress {
            queued: 3,
            active: 2,
            failed: 0,
        };
        assert_eq!(
            progress.summary_parts("core.queue.downloading"),
            vec![
                CountLabel {
                    key: "core.queue.downloading".to_string(),
                    count: 2,
                },
                CountLabel {
                    key: "core.queue.queued".to_string(),
                    count: 3,
                },
            ],
            "failed is zero and drops; the active verb is the caller's"
        );
        assert!(
            ReleaseQueueProgress::default()
                .summary_parts("core.queue.uploading")
                .is_empty(),
            "an idle queue has no parts"
        );
    }

    fn row(release_id: &str, state: ReleaseQueueState<()>) -> ReleaseQueueRow<(), ()> {
        ReleaseQueueRow {
            op: ReleaseQueueOp {
                release_id: release_id.to_string(),
                created_at: 0,
                payload: (),
                state,
            },
            release: None,
        }
    }

    /// The active row takes its progress from the map; the others keep their
    /// phase. A map without the active release's progress gives nothing.
    #[test]
    fn the_active_row_takes_its_progress_from_the_map() {
        let snapshot = ReleaseQueueSnapshot {
            rows: vec![
                row("rel-a", ReleaseQueueState::Active { progress: () }),
                row("rel-b", ReleaseQueueState::Queued),
            ],
            total: ReleaseQueueProgress {
                queued: 1,
                active: 1,
                failed: 0,
            },
            paused: false,
        };
        let with = snapshot
            .clone()
            .with_progress(&BTreeMap::from([("rel-a".to_string(), 42u8)]))
            .unwrap();
        assert_eq!(
            with.rows[0].op.state,
            ReleaseQueueState::Active { progress: 42 }
        );
        assert_eq!(with.rows[1].op.state, ReleaseQueueState::Queued);
        assert!(snapshot.with_progress::<u8>(&BTreeMap::new()).is_none());
    }
}
