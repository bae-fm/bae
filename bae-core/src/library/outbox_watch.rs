//! The upload queue and its transfers for a reader that draws them: the queue
//! each time it changes, and each transfer as it moves.

use super::{LibraryManager, OutboxState, OutboxTransfers, UploadTransfer, UploadTransferKey};
use tokio::sync::watch;

type OutboxValues = watch::Receiver<Option<Result<OutboxState, String>>>;

/// The upload queue: as it stands first, then each time its revision moves,
/// or the failure that kept it from being read.
pub struct OutboxQueueWatch {
    manager: LibraryManager,
    values: OutboxValues,
    /// What the reader was last told; `None` before the first read.
    told: Option<Told>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Told {
    Revision(u64),
    Failure,
}

impl OutboxQueueWatch {
    pub(crate) fn of(manager: LibraryManager) -> Self {
        Self {
            values: manager.subscribe_outbox_values(),
            manager,
            told: None,
        }
    }

    /// The queue now on the first call, read from the durable queue when
    /// nothing has been published yet; after that, the next queue whose
    /// revision differs, or a failure. A transfer that moves on the same
    /// queue wakes nothing here. `None` once the outbox is gone.
    pub async fn next(&mut self) -> Option<Result<OutboxState, String>> {
        if self.told.is_none() {
            let current = self.values.borrow_and_update().clone();
            let value = match current {
                Some(value) => value,
                None => self
                    .manager
                    .outbox_state()
                    .await
                    .map_err(|error| error.to_string()),
            };
            self.told = Some(Self::telling(&value));
            return Some(value);
        }
        loop {
            self.values.changed().await.ok()?;
            let Some(value) = self.values.borrow_and_update().clone() else {
                tracing::warn!("outbox value stream published an absent queue");
                continue;
            };
            let told = Self::telling(&value);
            if self.told == Some(told) && told != Told::Failure {
                continue;
            }
            self.told = Some(told);
            return Some(value);
        }
    }

    fn telling(value: &Result<OutboxState, String>) -> Told {
        match value {
            Ok(state) => Told::Revision(state.revision),
            Err(_) => Told::Failure,
        }
    }
}

/// A change to one transfer, or every transfer.
#[derive(Debug, Clone, PartialEq)]
pub enum OutboxTransferChange {
    Updated {
        key: UploadTransferKey,
        transfer: UploadTransfer,
    },
    /// The file or release left the queue.
    Removed { key: UploadTransferKey },
    /// The whole queue's transfer.
    TotalUpdated(UploadTransfer),
    /// Every transfer, as a reader first reads them.
    Reset(OutboxTransfers),
}

/// Every transfer for a reader that draws each one: all of them first, then
/// each one that changed since it last read. A failed read of the queue is
/// [`OutboxQueueWatch`]'s to report; this waits through it for the next
/// queue.
pub struct OutboxTransfersWatch {
    manager: LibraryManager,
    values: OutboxValues,
    /// What the reader was last told; `None` before the first read.
    told: Option<OutboxTransfers>,
}

impl OutboxTransfersWatch {
    pub(crate) fn of(manager: LibraryManager) -> Self {
        Self {
            values: manager.subscribe_outbox_values(),
            manager,
            told: None,
        }
    }

    /// Every transfer as one `Reset` on the first call that has them; after
    /// that, wait for transfers to change and name each one. `None` once the
    /// outbox is gone.
    pub async fn next(&mut self) -> Option<Vec<OutboxTransferChange>> {
        if self.told.is_none() {
            let current = self.values.borrow_and_update().clone();
            let first = match current {
                Some(value) => value,
                None => self
                    .manager
                    .outbox_state()
                    .await
                    .map_err(|error| error.to_string()),
            };
            match first {
                Ok(state) => {
                    self.told = Some(state.transfers.clone());
                    return Some(vec![OutboxTransferChange::Reset(state.transfers)]);
                }
                Err(error) => {
                    tracing::debug!("no upload transfers to show until the queue reads: {error}");
                }
            }
        }
        loop {
            self.values.changed().await.ok()?;
            let next = match self.values.borrow_and_update().clone() {
                Some(Ok(state)) => state.transfers,
                Some(Err(error)) => {
                    tracing::debug!("no upload transfers to show until the queue reads: {error}");
                    continue;
                }
                None => {
                    tracing::warn!("outbox value stream published an absent queue");
                    continue;
                }
            };
            let Some(told) = &self.told else {
                self.told = Some(next.clone());
                return Some(vec![OutboxTransferChange::Reset(next)]);
            };
            let changes = changes_between(told, &next);
            self.told = Some(next);
            if !changes.is_empty() {
                return Some(changes);
            }
        }
    }
}

/// What a reader told `told` must hear to hold `next`.
fn changes_between(told: &OutboxTransfers, next: &OutboxTransfers) -> Vec<OutboxTransferChange> {
    let mut changes: Vec<OutboxTransferChange> = told
        .keyed
        .keys()
        .filter(|key| !next.keyed.contains_key(*key))
        .map(|key| OutboxTransferChange::Removed { key: key.clone() })
        .collect();
    changes.extend(
        next.keyed
            .iter()
            .filter(|(key, transfer)| told.keyed.get(*key) != Some(*transfer))
            .map(|(key, transfer)| OutboxTransferChange::Updated {
                key: key.clone(),
                transfer: *transfer,
            }),
    );
    if told.total != next.total {
        changes.push(OutboxTransferChange::TotalUpdated(next.total));
    }
    changes
}

#[cfg(test)]
#[path = "outbox_watch_tests.rs"]
mod tests;
