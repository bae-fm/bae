//! The transfers a read shows: a release's transfer in flight is part of how
//! a detail or storage row is resolved, and the transfer map changes for every
//! release at progress cadence, so a read re-resolves only when a release it
//! shows changes.

use crate::album_detail::ReleaseStorageAction;
use std::collections::HashMap;

/// The transfer in flight for each release a read shows, as the read last
/// saw them.
pub(super) struct ShownTransfers {
    transfers: tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>>,
    shown: Vec<String>,
    actions: Vec<Option<ReleaseStorageAction>>,
}

impl ShownTransfers {
    pub(super) fn new(
        transfers: tokio::sync::watch::Receiver<HashMap<String, ReleaseStorageAction>>,
    ) -> Self {
        Self {
            transfers,
            shown: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Show `release_ids` from now on, noting their transfers as they stand.
    /// Call it before resolving the rows that show them, so a change landing
    /// in between is still seen by [`Self::changed`].
    pub(super) fn show(&mut self, release_ids: Vec<String>) {
        self.actions = Self::actions_of(&self.transfers.borrow_and_update(), &release_ids);
        self.shown = release_ids;
    }

    /// Wait until the transfer of a shown release changes. Cancel-safe: a
    /// change stays owed until a call returns it. `Err` once the transfer
    /// map is gone.
    pub(super) async fn changed(&mut self) -> Result<(), tokio::sync::watch::error::RecvError> {
        loop {
            self.transfers.changed().await?;
            let actions = Self::actions_of(&self.transfers.borrow_and_update(), &self.shown);
            if actions != self.actions {
                self.actions = actions;
                return Ok(());
            }
        }
    }

    fn actions_of(
        transfers: &HashMap<String, ReleaseStorageAction>,
        release_ids: &[String],
    ) -> Vec<Option<ReleaseStorageAction>> {
        release_ids
            .iter()
            .map(|release_id| transfers.get(release_id).copied())
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
            transfers.insert(
                "release-other".to_string(),
                ReleaseStorageAction::MakeRemote,
            );
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), shown.changed())
                .await
                .is_err(),
            "another release's transfer changes nothing shown"
        );

        tx.send_modify(|transfers| {
            transfers.insert(
                "release-shown".to_string(),
                ReleaseStorageAction::MakeRemote,
            );
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), shown.changed())
            .await
            .expect("a shown release's transfer is a change")
            .unwrap();
    }
}
