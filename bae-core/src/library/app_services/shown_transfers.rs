//! What a read shows of each release's place in a shared map that changes for
//! every release: a release's transfer in flight, which the map holds at
//! progress cadence, or its entry in the download queue, which moves with
//! every byte. A detail or storage row is resolved against it, so a read
//! re-resolves only when a release it shows changes there.

use crate::album_detail::ReleaseStorageAction;
use crate::library::{DownloadPhase, DownloadQueueContents};
use std::collections::HashMap;

/// What each release a read shows has in `Source`, as the read last saw it.
pub(super) struct ShownPerRelease<Source, Value> {
    source: tokio::sync::watch::Receiver<Source>,
    /// What `Source` holds for one release.
    of: fn(&Source, &str) -> Option<Value>,
    shown: Vec<String>,
    values: Vec<Option<Value>>,
}

/// The transfer in flight for each release a read shows.
pub(super) type ShownTransfers =
    ShownPerRelease<HashMap<String, ReleaseStorageAction>, ReleaseStorageAction>;

/// The download queue's phase for each release a read shows; the bytes an
/// active download moves are no change.
pub(super) type ShownDownloads = ShownPerRelease<DownloadQueueContents, DownloadPhase>;

impl ShownTransfers {
    pub(super) fn new(
        transfers: tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>>,
    ) -> Self {
        Self::over(transfers, |transfers, release_id| {
            transfers.get(release_id).copied()
        })
    }
}

impl ShownDownloads {
    pub(super) fn new(downloads: tokio::sync::watch::Receiver<DownloadQueueContents>) -> Self {
        Self::over(downloads, |queue, release_id| {
            queue
                .ops
                .iter()
                .find(|op| op.release_id == release_id)
                .map(|op| match &op.state {
                    crate::library::DownloadState::Queued => DownloadPhase::Queued,
                    crate::library::DownloadState::Active { .. } => {
                        DownloadPhase::Active { progress: () }
                    }
                    crate::library::DownloadState::Failed { error } => DownloadPhase::Failed {
                        error: error.clone(),
                    },
                })
        })
    }
}

impl<Source, Value: PartialEq> ShownPerRelease<Source, Value> {
    fn over(
        source: tokio::sync::watch::Receiver<Source>,
        of: fn(&Source, &str) -> Option<Value>,
    ) -> Self {
        Self {
            source,
            of,
            shown: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Show `release_ids` from now on, noting what each has as it stands.
    /// Call it before resolving the rows that show them, so a change landing
    /// in between is still seen by [`Self::changed`].
    pub(super) fn show(&mut self, release_ids: Vec<String>) {
        self.values = Self::values_of(&mut self.source, self.of, &release_ids);
        self.shown = release_ids;
    }

    /// Wait until what a shown release has changes. Cancel-safe: a change
    /// stays owed until a call returns it. `Err` once the source is gone.
    pub(super) async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
        loop {
            self.source.changed().await?;
            let values = Self::values_of(&mut self.source, self.of, &self.shown);
            if values != self.values {
                self.values = values;
                return Ok(());
            }
        }
    }

    fn values_of(
        source: &mut tokio::sync::watch::Receiver<Source>,
        of: fn(&Source, &str) -> Option<Value>,
        release_ids: &[String],
    ) -> Vec<Option<Value>> {
        let source = source.borrow_and_update();
        release_ids
            .iter()
            .map(|release_id| of(&source, release_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_a_shown_releases_transfer_is_a_change() {
        let (tx, rx) = tokio::sync::watch::channel(HashMap::new());
        let mut shown = ShownTransfers::new(rx);
        shown.show(vec!["release-shown".to_string()]);

        tx.send_modify(|transfers| {
            transfers.insert("release-other".to_string(), ReleaseStorageAction::Upload);
        });
        // Asked once: the map's change is already there to be read, so a wait
        // that would end on it ends on this poll.
        let polled = {
            let mut changed = std::pin::pin!(shown.changed());
            tokio::task::unconstrained(std::future::poll_fn(|cx| {
                std::task::Poll::Ready(std::future::Future::poll(changed.as_mut(), cx))
            }))
            .await
        };
        assert!(
            polled.is_pending(),
            "another release's transfer changes nothing shown"
        );

        tx.send_modify(|transfers| {
            transfers.insert("release-shown".to_string(), ReleaseStorageAction::Upload);
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), shown.changed())
            .await
            .expect("a shown release's transfer is a change")
            .unwrap();
    }

    fn downloading(release_id: &str, bytes_done: u64) -> DownloadQueueContents {
        DownloadQueueContents {
            ops: vec![crate::library::release_queue::ReleaseQueueOp {
                release_id: release_id.to_string(),
                created_at: 0,
                payload: (),
                state: crate::library::DownloadState::Active {
                    progress: crate::library::DownloadTransferProgress::new(
                        release_id, bytes_done, 100,
                    )
                    .unwrap(),
                },
            }],
            paused: false,
        }
    }

    /// Bytes arriving for a shown release's download are no change to what
    /// the read shows; the download ending is.
    #[tokio::test]
    async fn only_a_shown_releases_download_phase_is_a_change() {
        let (tx, rx) = tokio::sync::watch::channel(downloading("release-shown", 10));
        let mut shown = ShownDownloads::new(rx);
        shown.show(vec!["release-shown".to_string()]);

        tx.send_replace(downloading("release-shown", 40));
        let polled = {
            let mut changed = std::pin::pin!(shown.changed());
            tokio::task::unconstrained(std::future::poll_fn(|cx| {
                std::task::Poll::Ready(std::future::Future::poll(changed.as_mut(), cx))
            }))
            .await
        };
        assert!(polled.is_pending(), "bytes arriving change no phase");

        tx.send_replace(DownloadQueueContents::default());
        tokio::time::timeout(std::time::Duration::from_secs(1), shown.changed())
            .await
            .expect("the download leaving the queue is a change")
            .unwrap();
    }
}
