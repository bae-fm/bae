//! The progress of each release transfer under way — the active download's
//! bytes, the active export's percent, a move-to-this-device's bytes — for a
//! reader that draws each one: every release's progress at once, then again
//! each time any of it moves. No table holds the progress, so the reads of the
//! transfers' tables change only as transfers come, go, start or fail.

use std::collections::BTreeMap;
use tokio::sync::watch;

/// The progress of each release whose transfer of one kind is under way,
/// keyed by release.
pub struct ReleaseTransfers<Progress> {
    transfers: watch::Sender<BTreeMap<String, Progress>>,
}

impl<Progress> Clone for ReleaseTransfers<Progress> {
    fn clone(&self) -> Self {
        Self { transfers: self.transfers.clone() }
    }
}

impl<Progress: Clone + PartialEq> ReleaseTransfers<Progress> {
    /// No transfer under way.
    pub fn new() -> Self {
        Self { transfers: watch::Sender::new(BTreeMap::new()) }
    }

    /// Hear every release's progress at once, then each time it changes.
    pub fn watch(&self) -> ReleaseTransfersWatch<Progress> {
        let mut transfers = self.transfers.subscribe();
        transfers.mark_changed();
        ReleaseTransfersWatch { transfers }
    }

    /// `release_id`'s transfer is under way and has reached `progress`.
    pub fn set(&self, release_id: &str, progress: Progress) {
        self.transfers.send_if_modified(|transfers| {
            if transfers.get(release_id) == Some(&progress) {
                return false;
            }
            transfers.insert(release_id.to_string(), progress);
            true
        });
    }

    /// `release_id`'s transfer has reached `progress`, if it is listed as
    /// under way; a release not listed stays unlisted.
    pub fn advance(&self, release_id: &str, progress: Progress) {
        self.transfers.send_if_modified(|transfers| match transfers.get_mut(release_id) {
            Some(listed) if *listed != progress => {
                *listed = progress;
                true
            }
            Some(_) | None => false,
        });
    }

    /// `release_id`'s transfer is no longer under way, however it ended.
    pub fn remove(&self, release_id: &str) {
        self.transfers.send_if_modified(|transfers| transfers.remove(release_id).is_some());
    }

    /// List `release_id`'s progress through the returned report until it
    /// drops, however the transfer ends.
    pub fn report(&self, release_id: &str) -> ReleaseTransferReport<Progress> {
        ReleaseTransferReport { transfers: self.clone(), release_id: release_id.to_string() }
    }
}

/// One release's progress while its transfer runs. Dropping it removes the
/// release.
pub struct ReleaseTransferReport<Progress: Clone + PartialEq> {
    transfers: ReleaseTransfers<Progress>,
    release_id: String,
}

impl<Progress: Clone + PartialEq> ReleaseTransferReport<Progress> {
    /// The release's transfer has reached `progress`.
    pub fn progress(&self, progress: Progress) {
        self.transfers.set(&self.release_id, progress);
    }
}

impl<Progress: Clone + PartialEq> Drop for ReleaseTransferReport<Progress> {
    fn drop(&mut self) {
        self.transfers.remove(&self.release_id);
    }
}

/// Hears the progress of every release in one [`ReleaseTransfers`], keyed by
/// release.
pub struct ReleaseTransfersWatch<Progress> {
    transfers: watch::Receiver<BTreeMap<String, Progress>>,
}

impl<Progress: Clone> ReleaseTransfersWatch<Progress> {
    /// Every active release's progress: at once on the first call, then each
    /// time any release's progress changes. `None` once the transfers are
    /// gone.
    pub async fn next(&mut self) -> Option<BTreeMap<String, Progress>> {
        self.transfers.changed().await.ok()?;
        Some(self.transfers.borrow_and_update().clone())
    }

    /// Read until every release's progress satisfies `done`, and return it;
    /// `None` if the transfers end first.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn until(
        &mut self,
        done: impl Fn(&BTreeMap<String, Progress>) -> bool,
    ) -> Option<BTreeMap<String, Progress>> {
        loop {
            let transfers = self.next().await?;
            if done(&transfers) {
                return Some(transfers);
            }
        }
    }
}

#[cfg(test)]
#[path = "release_transfers_tests.rs"]
mod tests;
