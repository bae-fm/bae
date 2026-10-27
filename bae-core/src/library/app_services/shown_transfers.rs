//! What a read shows of each release's transfer in flight, from the map that
//! holds every release's at progress cadence. A detail or storage row is
//! resolved against it, so a read re-resolves only when a release it shows
//! changes there.

use crate::album_detail::ReleaseStorageAction;
use std::collections::HashMap;

type Transfers = HashMap<String, ReleaseStorageAction>;

/// The transfer in flight for each release a read shows, as the read last
/// saw it.
pub(super) struct ShownTransfers {
    source: tokio::sync::watch::Receiver<Transfers>,
    shown: Vec<String>,
    values: Vec<Option<ReleaseStorageAction>>,
}

impl ShownTransfers {
    pub(super) fn new(source: tokio::sync::watch::Receiver<Transfers>) -> Self {
        Self {
            source,
            shown: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Show `release_ids` from now on, noting what each has as it stands.
    /// Call it before resolving the rows that show them, so a change landing
    /// in between is still seen by [`Self::changed`].
    pub(super) fn show(&mut self, release_ids: Vec<String>) {
        self.values = Self::values_of(&mut self.source, &release_ids);
        self.shown = release_ids;
    }

    /// Wait until what a shown release has changes. Cancel-safe: a change
    /// stays owed until a call returns it. `Err` once the source is gone.
    pub(super) async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
        loop {
            self.source.changed().await?;
            let values = Self::values_of(&mut self.source, &self.shown);
            if values != self.values {
                self.values = values;
                return Ok(());
            }
        }
    }

    fn values_of(
        source: &mut tokio::sync::watch::Receiver<Transfers>,
        release_ids: &[String],
    ) -> Vec<Option<ReleaseStorageAction>> {
        let source = source.borrow_and_update();
        release_ids
            .iter()
            .map(|release_id| source.get(release_id).copied())
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
}
