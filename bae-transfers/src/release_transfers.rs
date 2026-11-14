//! The progress of each release transfer under way — the active download's
//! bytes, the active export's percent, a move-to-this-device's bytes — for a reader
//! that draws each one: all of it first, then each release whose progress
//! moved. No table holds the progress, so the reads of the transfers' tables
//! change only as transfers come, go, start or fail.

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
    Removed {
        release_id: String,
    },
    /// Every active release's progress, as a reader first reads it.
    Reset(BTreeMap<String, Progress>),
}

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

    /// Hear every release's progress, then each change to it.
    pub fn watch(&self) -> ReleaseTransfersWatch<Progress> {
        ReleaseTransfersWatch { transfers: self.transfers.subscribe(), told: None }
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

    /// `release_id`'s transfer is no longer under way.
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
    /// What the reader was last told; `None` before the first read.
    told: Option<BTreeMap<String, Progress>>,
}

impl<Progress: Clone + PartialEq> ReleaseTransfersWatch<Progress> {
    /// Every active release's progress as one `Reset` on the first call;
    /// after that, wait for progress to change and name each release it
    /// changed for. `None` once the transfers are gone.
    pub async fn next(&mut self) -> Option<Vec<ReleaseTransferChange<Progress>>> {
        let Some(told) = &mut self.told else {
            let current = self.transfers.borrow_and_update().clone();
            self.told = Some(current.clone());
            return Some(vec![ReleaseTransferChange::Reset(current)]);
        };
        loop {
            self.transfers.changed().await.ok()?;
            let next = self.transfers.borrow_and_update().clone();
            let changes = release_changes(told, &next);
            *told = next;
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }

    /// Read changes until every release's progress, as told, satisfies
    /// `done`, and return it; `None` if the transfers end first.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn until(
        &mut self,
        done: impl Fn(&BTreeMap<String, Progress>) -> bool,
    ) -> Option<BTreeMap<String, Progress>> {
        loop {
            self.next().await?;
            let told = self.told.as_ref().expect("a read tells the reader");
            if done(told) {
                return Some(told.clone());
            }
        }
    }
}

/// What a reader told each release's progress in `told` must hear to hold
/// `next`: each release that left, then each that came or moved.
pub fn release_changes<Progress: Clone + PartialEq>(
    told: &BTreeMap<String, Progress>,
    next: &BTreeMap<String, Progress>,
) -> Vec<ReleaseTransferChange<Progress>> {
    let mut changes: Vec<ReleaseTransferChange<Progress>> = told
        .keys()
        .filter(|release_id| !next.contains_key(*release_id))
        .map(|release_id| ReleaseTransferChange::Removed { release_id: release_id.clone() })
        .collect();
    changes.extend(
        next.iter().filter(|(release_id, progress)| told.get(*release_id) != Some(*progress)).map(
            |(release_id, progress)| ReleaseTransferChange::Updated {
                release_id: release_id.clone(),
                progress: progress.clone(),
            },
        ),
    );
    changes
}

/// Apply what a reader heard to `held`, each release's progress as it last
/// heard it: what [`release_changes`] said, back into the progress it
/// describes.
pub fn apply_release_changes<Progress>(
    held: &mut BTreeMap<String, Progress>,
    changes: Vec<ReleaseTransferChange<Progress>>,
) {
    for change in changes {
        match change {
            ReleaseTransferChange::Reset(all) => *held = all,
            ReleaseTransferChange::Updated { release_id, progress } => {
                held.insert(release_id, progress);
            }
            ReleaseTransferChange::Removed { release_id } => {
                held.remove(&release_id);
            }
        }
    }
}

#[cfg(test)]
#[path = "release_transfers_tests.rs"]
mod tests;
