//! The progress of the active entry of a release queue — a download's bytes,
//! an export's percent — for a reader that draws each one: all of it first,
//! then each release whose progress moved. The queue's table leaves the
//! progress out, so its reads change only as entries come, go, start or fail.

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
pub struct ReleaseTransfersWatch<Progress> {
    transfers: watch::Receiver<BTreeMap<String, Progress>>,
    /// What the reader was last told; `None` before the first read.
    told: Option<BTreeMap<String, Progress>>,
}

pub type DownloadTransfersWatch = ReleaseTransfersWatch<DownloadTransferProgress>;
pub type OutputTransfersWatch = ReleaseTransfersWatch<u8>;

impl<Progress: Clone + PartialEq> ReleaseTransfersWatch<Progress> {
    pub(crate) fn of(transfers: watch::Receiver<BTreeMap<String, Progress>>) -> Self {
        Self {
            transfers,
            told: None,
        }
    }

    /// Every active release's progress as one `Reset` on the first call;
    /// after that, wait for progress to change and name each release it
    /// changed for. `None` once the queue is gone.
    pub async fn next(&mut self) -> Option<Vec<ReleaseTransferChange<Progress>>> {
        let Some(told) = &mut self.told else {
            let current = self.transfers.borrow_and_update().clone();
            self.told = Some(current.clone());
            return Some(vec![ReleaseTransferChange::Reset(current)]);
        };
        loop {
            self.transfers.changed().await.ok()?;
            let next = self.transfers.borrow_and_update().clone();
            let changes = changes_between(told, &next);
            *told = next;
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }
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

    fn progress(bytes_done: u64) -> DownloadTransferProgress {
        DownloadTransferProgress::new("release", bytes_done, 100).unwrap()
    }

    /// The first read is every downloading release; a step of bytes names
    /// only the release it moved; finishing removes it.
    #[tokio::test]
    async fn a_reader_hears_each_release_whose_progress_moved() {
        let (tx, rx) = watch::channel(BTreeMap::from([("release-1".to_string(), progress(10))]));
        let mut watch = DownloadTransfersWatch::of(rx);
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
                "release-1".to_string(),
                progress(10)
            )]))])
        );

        tx.send_replace(BTreeMap::from([("release-1".to_string(), progress(40))]));
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Updated {
                release_id: "release-1".to_string(),
                progress: progress(40),
            }])
        );

        tx.send_replace(BTreeMap::new());
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Removed {
                release_id: "release-1".to_string(),
            }])
        );
    }

    /// An export's percent is its progress: a send that moves no percent says
    /// nothing, and the next step is the next change heard.
    #[tokio::test]
    async fn a_reader_hears_each_export_whose_percent_moved() {
        let (tx, rx) = watch::channel(BTreeMap::from([("release-1".to_string(), 10u8)]));
        let mut watch = OutputTransfersWatch::of(rx);
        assert_eq!(
            watch.next().await,
            Some(vec![ReleaseTransferChange::Reset(BTreeMap::from([(
                "release-1".to_string(),
                10
            )]))])
        );

        tx.send_replace(BTreeMap::from([("release-1".to_string(), 10u8)]));
        let mut next = std::pin::pin!(watch.next());
        // Asked once with the send already there to read: it reads it, finds
        // no percent moved, and waits on.
        let polled = tokio::task::unconstrained(std::future::poll_fn(|cx| {
            std::task::Poll::Ready(std::future::Future::poll(next.as_mut(), cx))
        }))
        .await;
        assert!(polled.is_pending(), "an unmoved percent says nothing");
        tx.send_replace(BTreeMap::from([("release-1".to_string(), 62u8)]));
        assert_eq!(
            next.await,
            Some(vec![ReleaseTransferChange::Updated {
                release_id: "release-1".to_string(),
                progress: 62,
            }])
        );
    }
}
