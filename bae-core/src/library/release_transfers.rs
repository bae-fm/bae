//! The progress of the active entry of a release queue — a download's bytes,
//! an export's percent — for a reader that draws each one: all of it first,
//! then each release whose progress moved. The queue's own value leaves the
//! progress out, so it changes only as entries come, go, start or fail.

use super::output_snapshot::OutputRequest;
use super::release_queue::{ReleaseQueueContents, ReleaseQueueState};
use super::DownloadTransferProgress;
use std::collections::BTreeMap;
use tokio::sync::watch;

/// A change to one release's progress, or every release's.
#[derive(Debug, Clone, PartialEq)]
pub enum ReleaseTransferChange<Progress> {
    Updated {
        release_id: String,
        progress: Progress,
    },
    /// The release's entry is no longer active.
    Removed { release_id: String },
    /// Every active release's progress, as a reader first reads it.
    Reset(BTreeMap<String, Progress>),
}

pub type DownloadTransferChange = ReleaseTransferChange<DownloadTransferProgress>;
/// An export's progress is its percent.
pub type OutputTransferChange = ReleaseTransferChange<u8>;

/// The progress of every active release in one queue, keyed by release.
pub struct ReleaseTransfersWatch<Extra, Progress> {
    contents: watch::Receiver<ReleaseQueueContents<Extra, Progress>>,
    /// What the reader was last told; `None` before the first read.
    told: Option<BTreeMap<String, Progress>>,
}

pub type DownloadTransfersWatch = ReleaseTransfersWatch<(), DownloadTransferProgress>;
pub type OutputTransfersWatch = ReleaseTransfersWatch<OutputRequest, u8>;

impl<Extra, Progress: Clone + PartialEq> ReleaseTransfersWatch<Extra, Progress> {
    pub(crate) fn of(contents: watch::Receiver<ReleaseQueueContents<Extra, Progress>>) -> Self {
        Self {
            contents,
            told: None,
        }
    }

    /// Every active release's progress as one `Reset` on the first call;
    /// after that, wait for progress to change and name each release it
    /// changed for. `None` once the queue is gone.
    pub async fn next(&mut self) -> Option<Vec<ReleaseTransferChange<Progress>>> {
        let Some(told) = &mut self.told else {
            let current = transfers(&self.contents.borrow_and_update());
            self.told = Some(current.clone());
            return Some(vec![ReleaseTransferChange::Reset(current)]);
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
fn transfers<Extra, Progress: Clone>(
    contents: &ReleaseQueueContents<Extra, Progress>,
) -> BTreeMap<String, Progress> {
    contents
        .ops
        .iter()
        .filter_map(|op| match &op.state {
            ReleaseQueueState::Active { progress } => {
                Some((op.release_id.clone(), progress.clone()))
            }
            ReleaseQueueState::Queued | ReleaseQueueState::Failed { .. } => None,
        })
        .collect()
}

/// What a reader told `told` must hear to hold `next`.
fn changes_between<Progress: Clone + PartialEq>(
    told: &BTreeMap<String, Progress>,
    next: &BTreeMap<String, Progress>,
) -> Vec<ReleaseTransferChange<Progress>> {
    let mut changes: Vec<ReleaseTransferChange<Progress>> = told
        .keys()
        .filter(|release_id| !next.contains_key(*release_id))
        .map(|release_id| ReleaseTransferChange::Removed {
            release_id: release_id.clone(),
        })
        .collect();
    changes.extend(
        next.iter()
            .filter(|(release_id, progress)| told.get(*release_id) != Some(*progress))
            .map(|(release_id, progress)| ReleaseTransferChange::Updated {
                release_id: release_id.clone(),
                progress: progress.clone(),
            }),
    );
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::release_queue::ReleaseQueueOp;
    use crate::library::DownloadQueueContents;

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
            Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
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
            Some(vec![ReleaseTransferChange::Updated {
                release_id: "release-1".to_string(),
                progress: progress(40),
            }])
        );

        tx.send_replace(contents(vec![("release-2", ReleaseQueueState::Queued)]));
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Removed {
                release_id: "release-1".to_string(),
            }])
        );
    }

    /// An export's percent is its progress: a step names the release it moved,
    /// a queued entry joining says nothing of its own, and the next step is
    /// the next change heard.
    #[tokio::test]
    async fn a_reader_hears_each_export_whose_percent_moved() {
        let request = || crate::library::output_snapshot::OutputRequest {
            target_dir: "/exports".into(),
            kind: crate::library::OutputKind::Export,
        };
        let exports =
            |states: Vec<(&str, ReleaseQueueState<u8>)>| crate::library::OutputQueueContents {
                ops: states
                    .into_iter()
                    .map(|(release_id, state)| ReleaseQueueOp {
                        release_id: release_id.to_string(),
                        created_at: 0,
                        payload: request(),
                        state,
                    })
                    .collect(),
                paused: false,
            };
        let (tx, rx) = watch::channel(exports(vec![(
            "release-1",
            ReleaseQueueState::Active { progress: 10 },
        )]));
        let mut watch = OutputTransfersWatch::of(rx);
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
                "release-1".to_string(),
                10
            )]))])
        );

        tx.send_replace(exports(vec![
            ("release-1", ReleaseQueueState::Active { progress: 10 }),
            ("release-2", ReleaseQueueState::Queued),
        ]));
        let mut next = std::pin::pin!(watch.next());
        // Asked once with the join already there to read: it reads it, finds
        // no percent moved, and waits on.
        let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
        }))
        .await;
        assert!(
            polled.is_pending(),
            "a queued entry joining moves no percent"
        );
        tx.send_replace(exports(vec![
            ("release-1", ReleaseQueueState::Active { progress: 62 }),
            ("release-2", ReleaseQueueState::Queued),
        ]));
        assert_eq!(
            next.await,
            Some(vec![ReleaseTransferChange::Updated {
                release_id: "release-1".to_string(),
                progress: 62,
            }])
        );
        let at = |percent| {
            exports(vec![(
                "release-1",
                ReleaseQueueState::Active { progress: percent },
            )])
            .phases()
        };
        assert_eq!(
            at(10).ops,
            at(62).ops,
            "the queue's value leaves the percent out"
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
