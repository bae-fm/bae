//! The active downloads' byte progress for a reader that draws each one: all
//! of it first, then each release whose progress moved.

use super::{DownloadQueueContents, DownloadTransferProgress};
use std::collections::BTreeMap;
use tokio::sync::watch;

/// A change to one release's download progress, or every release's.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadTransferChange {
    Updated {
        release_id: String,
        progress: DownloadTransferProgress,
    },
    /// The release is no longer downloading.
    Removed { release_id: String },
    /// Every downloading release's progress, as a reader first reads it.
    Reset(BTreeMap<String, DownloadTransferProgress>),
}

/// The progress of every downloading release, keyed by release.
pub struct DownloadTransfersWatch {
    contents: watch::Receiver<DownloadQueueContents>,
    /// What the reader was last told; `None` before the first read.
    told: Option<BTreeMap<String, DownloadTransferProgress>>,
}

impl DownloadTransfersWatch {
    pub(crate) fn of(contents: watch::Receiver<DownloadQueueContents>) -> Self {
        Self {
            contents,
            told: None,
        }
    }

    /// Every downloading release's progress as one `Reset` on the first
    /// call; after that, wait for progress to change and name each release
    /// it changed for. `None` once the queue is gone.
    pub async fn next(&mut self) -> Option<Vec<DownloadTransferChange>> {
        let Some(told) = &mut self.told else {
            let current = transfers(&self.contents.borrow_and_update());
            self.told = Some(current.clone());
            return Some(vec![DownloadTransferChange::Reset(current)]);
        };
        loop {
            self.contents.changed().await.ok()?;
            let next = transfers(&self.contents.borrow_and_update());
            let changes = changes_between(told, &next);
            *told = next;
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }
}

/// Each active entry's progress, by release.
fn transfers(contents: &DownloadQueueContents) -> BTreeMap<String, DownloadTransferProgress> {
    contents
        .ops
        .iter()
        .filter_map(|op| match &op.state {
            super::DownloadState::Active { progress } => {
                Some((op.release_id.clone(), progress.clone()))
            }
            super::DownloadState::Queued | super::DownloadState::Failed { .. } => None,
        })
        .collect()
}

/// What a reader told `told` must hear to hold `next`.
fn changes_between(
    told: &BTreeMap<String, DownloadTransferProgress>,
    next: &BTreeMap<String, DownloadTransferProgress>,
) -> Vec<DownloadTransferChange> {
    let mut changes: Vec<DownloadTransferChange> = told
        .keys()
        .filter(|release_id| !next.contains_key(*release_id))
        .map(|release_id| DownloadTransferChange::Removed {
            release_id: release_id.clone(),
        })
        .collect();
    changes.extend(
        next.iter()
            .filter(|(release_id, progress)| told.get(*release_id) != Some(*progress))
            .map(|(release_id, progress)| DownloadTransferChange::Updated {
                release_id: release_id.clone(),
                progress: progress.clone(),
            }),
    );
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::release_queue::{ReleaseQueueOp, ReleaseQueueState};

    fn progress(bytes_done: u64) -> DownloadTransferProgress {
        DownloadTransferProgress::new("release", bytes_done, 100).unwrap()
    }

    fn contents(states: Vec<(&str, crate::library::DownloadState)>) -> DownloadQueueContents {
        DownloadQueueContents {
            ops: states
                .into_iter()
                .map(|(release_id, state)| ReleaseQueueOp {
                    release_id: release_id.to_string(),
                    created_at: 0,
                    payload: (),
                    state,
                })
                .collect(),
            paused: false,
        }
    }

    /// The first read is every downloading release; a step of bytes names
    /// only the release it moved; finishing removes it; a queued entry
    /// joining with nothing downloading says nothing.
    #[tokio::test]
    async fn a_reader_hears_each_release_whose_progress_moved() {
        let (tx, rx) = watch::channel(contents(vec![(
            "release-1",
            ReleaseQueueState::Active {
                progress: progress(10),
            },
        )]));
        let mut watch = DownloadTransfersWatch::of(rx);
        assert_eq!(
            watch.next().await,
            Some(vec![DownloadTransferChange::Reset(BTreeMap::from([(
                "release-1".to_string(),
                progress(10)
            )]))])
        );

        tx.send_replace(contents(vec![
            (
                "release-1",
                ReleaseQueueState::Active {
                    progress: progress(40),
                },
            ),
            ("release-2", ReleaseQueueState::Queued),
        ]));
        assert_eq!(
            watch.next().await,
            Some(vec![DownloadTransferChange::Updated {
                release_id: "release-1".to_string(),
                progress: progress(40),
            }])
        );

        tx.send_replace(contents(vec![("release-2", ReleaseQueueState::Queued)]));
        assert_eq!(
            watch.next().await,
            Some(vec![DownloadTransferChange::Removed {
                release_id: "release-1".to_string(),
            }])
        );
    }

    /// The queue's own value without progress stays the same while bytes
    /// arrive, which is what lets it be sent only when it changes.
    #[test]
    fn bytes_arriving_leave_the_phases_as_they_were() {
        let at = |bytes| {
            contents(vec![(
                "release-1",
                ReleaseQueueState::Active {
                    progress: progress(bytes),
                },
            )])
            .phases()
        };
        assert_eq!(at(10).ops, at(40).ops);
    }
}
