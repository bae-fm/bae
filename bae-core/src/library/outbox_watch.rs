//! How fast the uploads go, for a reader that draws it: each file being sent,
//! each release with files being sent, and the files being sent together,
//! with the bytes the provider confirmed of those. A live query of the files
//! being sent reads their bytes; the rates measured from them are the only
//! upload values no table holds.

use super::upload_throughput::UploadThroughput;
use super::UploadBar;
use crate::db::DbSendingUploads;
use std::collections::HashMap;

/// What a rate is of: one file being sent, or a release's files being sent
/// together.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UploadTransferKey {
    File(coven::FileId),
    Release(String),
}

/// How far the files being sent have gone together and how fast they go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadTransfer {
    /// The bytes the provider confirmed of the files being sent, while any
    /// is; a file sent in one request has none to count.
    pub bar: Option<UploadBar>,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// How fast each file being sent and each release with files being sent
/// goes, and the files being sent together. They change with each part a
/// provider confirms.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxTransfers {
    /// Bytes per second, by what they are of.
    pub rates: HashMap<UploadTransferKey, u64>,
    pub total: UploadTransfer,
}

impl OutboxTransfers {
    /// The transfers of the files `sending` lists, each file at its rate in
    /// `rates`. A file no rate was measured for moves nothing yet.
    pub(crate) fn of(sending: &DbSendingUploads, rates: &HashMap<coven::FileId, u64>) -> Self {
        let add = |sum: &mut u64, rate: u64| {
            *sum = sum.checked_add(rate).expect("upload rate overflow");
        };
        let mut keyed = HashMap::new();
        let mut total_bps = 0;
        for file in &sending.files {
            let rate = rates.get(&file.file).copied().unwrap_or(0);
            keyed.insert(UploadTransferKey::File(file.file), rate);
            if let Some(release_id) = &file.release_id {
                add(
                    keyed
                        .entry(UploadTransferKey::Release(release_id.clone()))
                        .or_default(),
                    rate,
                );
            }
            add(&mut total_bps, rate);
        }
        let bytes = |value: i64| u64::try_from(value).expect("an upload session's bytes");
        Self {
            rates: keyed,
            total: UploadTransfer {
                bar: (!sending.files.is_empty()).then(|| UploadBar {
                    bytes_done: bytes(sending.bytes_confirmed),
                    bytes_total: bytes(sending.bytes_total),
                }),
                throughput_bps: total_bps,
            },
        }
    }
}

/// A change to one rate, or every transfer.
#[derive(Debug, Clone, PartialEq)]
pub enum OutboxTransferChange {
    Updated {
        key: UploadTransferKey,
        throughput_bps: u64,
    },
    /// The file or release has nothing being sent any more.
    Removed { key: UploadTransferKey },
    /// The files being sent together.
    TotalUpdated(UploadTransfer),
    /// Every transfer, as a reader first reads them.
    Reset(OutboxTransfers),
}

/// The transfers each delivery of the files being sent measures.
struct SendingTransfers {
    sending: coven::LiveQuery<DbSendingUploads>,
    throughput: UploadThroughput,
    clock: coven::ClockRef,
}

impl SendingTransfers {
    fn of(sending: coven::LiveQuery<DbSendingUploads>, clock: coven::ClockRef) -> Self {
        Self {
            sending,
            throughput: UploadThroughput::default(),
            clock,
        }
    }

    /// The transfers the next delivery measures, or the failure that kept it
    /// from reading; `None` once the query has ended with its store.
    async fn next(&mut self) -> Option<coven::CovenResult<OutboxTransfers>> {
        let sending = match self.sending.next().await? {
            Ok(sending) => sending,
            Err(error) => return Some(Err(error)),
        };
        let now = self.clock.elapsed();
        self.throughput.observe(&sending, now);
        Some(Ok(OutboxTransfers::of(
            &sending,
            &self.throughput.rates(now),
        )))
    }
}

/// Every transfer for a reader that draws each one: all of them first, then
/// each one that changed since it last read.
pub struct OutboxTransfersWatch {
    transfers: SendingTransfers,
    /// What the reader was last told; `None` before the first read.
    told: Option<OutboxTransfers>,
}

impl OutboxTransfersWatch {
    pub(crate) fn of(sending: coven::LiveQuery<DbSendingUploads>, clock: coven::ClockRef) -> Self {
        Self {
            transfers: SendingTransfers::of(sending, clock),
            told: None,
        }
    }

    /// Every transfer as one `Reset` on the first read; after that, wait for
    /// transfers to change and name each one. A failed read is handed on,
    /// and the next read is told against what the reader last held. `None`
    /// once the query has ended with its store.
    pub async fn next(&mut self) -> Option<coven::CovenResult<Vec<OutboxTransferChange>>> {
        loop {
            let next = match self.transfers.next().await? {
                Ok(next) => next,
                Err(error) => return Some(Err(error)),
            };
            let Some(told) = &self.told else {
                self.told = Some(next.clone());
                return Some(Ok(vec![OutboxTransferChange::Reset(next)]));
            };
            let changes = changes_between(told, &next);
            self.told = Some(next);
            if !changes.is_empty() {
                return Some(Ok(changes));
            }
        }
    }
}

/// The files being sent together, for a reader that draws only that: as it
/// stands first, then each time it moves. A file's or a release's rate
/// moving while the total holds wakes nothing here.
pub struct OutboxTotalTransferWatch {
    transfers: SendingTransfers,
    /// What the reader was last told; `None` before the first read.
    told: Option<UploadTransfer>,
}

impl OutboxTotalTransferWatch {
    pub(crate) fn of(sending: coven::LiveQuery<DbSendingUploads>, clock: coven::ClockRef) -> Self {
        Self {
            transfers: SendingTransfers::of(sending, clock),
            told: None,
        }
    }

    /// The total on the first read, then the next total that differs. A
    /// failed read is handed on. `None` once the query has ended with its
    /// store.
    pub async fn next(&mut self) -> Option<coven::CovenResult<UploadTransfer>> {
        loop {
            let total = match self.transfers.next().await? {
                Ok(next) => next.total,
                Err(error) => return Some(Err(error)),
            };
            if self.told == Some(total) {
                continue;
            }
            self.told = Some(total);
            return Some(Ok(total));
        }
    }
}

/// What a reader told `told` must hear to hold `next`.
fn changes_between(told: &OutboxTransfers, next: &OutboxTransfers) -> Vec<OutboxTransferChange> {
    let mut changes: Vec<OutboxTransferChange> = told
        .rates
        .keys()
        .filter(|key| !next.rates.contains_key(*key))
        .map(|key| OutboxTransferChange::Removed { key: key.clone() })
        .collect();
    changes.extend(
        next.rates
            .iter()
            .filter(|(key, rate)| told.rates.get(*key) != Some(*rate))
            .map(|(key, rate)| OutboxTransferChange::Updated {
                key: key.clone(),
                throughput_bps: *rate,
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
