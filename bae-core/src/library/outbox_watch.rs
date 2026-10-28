//! How far and how fast the uploads go, for a reader that draws it: each
//! release with files being sent, and the files being sent together. A live
//! query of the files being sent says which files and releases move; how far
//! each file has got is coven's upload watch, and how fast is measured here
//! from it. Neither is held by any table.

use super::upload_throughput::UploadThroughput;
use super::UploadBar;
use crate::db::DbSendingUploads;
use std::collections::{BTreeMap, HashMap};

/// How far the files being sent have gone together and how fast they go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadTransfer {
    /// The bytes of the files being sent moved so far, out of their objects'
    /// sizes, while any is.
    pub bar: Option<UploadBar>,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// How far a release's upload has got and how fast its files being sent go
/// together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseUploadTransfer {
    /// Of the release's whole size, the bytes of its files stored, and of
    /// each file being sent, the share of its size its object has moved.
    pub bar: UploadBar,
    /// Bytes per second.
    pub throughput_bps: u64,
}

/// How far and how fast each release with files being sent goes, and every
/// file being sent together. They change as each file's bytes go.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxTransfers {
    /// By release id.
    pub releases: HashMap<String, ReleaseUploadTransfer>,
    pub total: UploadTransfer,
}

impl OutboxTransfers {
    /// The transfers of the files `sending` lists: each file as far as
    /// `progress` has it and at its rate in `rates`, a release with its
    /// stored files and its files being sent together. A file the watch or
    /// the rates don't name yet has moved nothing.
    pub(crate) fn of(
        sending: &DbSendingUploads,
        rates: &HashMap<coven::FileId, u64>,
        progress: &BTreeMap<coven::FileId, coven::TransferProgress>,
    ) -> Self {
        let add = |sum: &mut u64, value: u64| {
            *sum = sum.checked_add(value).expect("upload bytes overflow");
        };
        let bytes = |value: i64| u64::try_from(value).expect("a release's bytes");
        let mut releases: HashMap<String, ReleaseUploadTransfer> = sending
            .releases
            .iter()
            .map(|release| {
                (
                    release.release_id.clone(),
                    ReleaseUploadTransfer {
                        bar: UploadBar {
                            bytes_done: bytes(release.bytes_stored),
                            bytes_total: bytes(release.bytes_total),
                        },
                        throughput_bps: 0,
                    },
                )
            })
            .collect();
        let mut total = UploadTransfer {
            bar: (!sending.files.is_empty()).then_some(UploadBar {
                bytes_done: 0,
                bytes_total: 0,
            }),
            throughput_bps: 0,
        };
        for file in &sending.files {
            let rate = rates.get(&file.file).copied().unwrap_or(0);
            let moved = progress.get(&file.file).copied();
            if let Some(release_id) = &file.release_id {
                let release = releases
                    .get_mut(release_id)
                    .expect("a file being sent has its release read with it");
                add(&mut release.throughput_bps, rate);
                let size = file
                    .file_size
                    .map(bytes)
                    .expect("a release file being sent has its size read with it");
                let sent = moved.map_or(0.0, super::file_transfers::fraction);
                add(&mut release.bar.bytes_done, (size as f64 * sent) as u64);
            }
            add(&mut total.throughput_bps, rate);
            if let (Some(bar), Some(moved)) = (&mut total.bar, moved) {
                add(&mut bar.bytes_done, moved.bytes_transferred);
                add(&mut bar.bytes_total, moved.bytes_total);
            }
        }
        Self { releases, total }
    }
}

/// A change to one release's transfer, or every transfer.
#[derive(Debug, Clone, PartialEq)]
pub enum OutboxTransferChange {
    Updated {
        release_id: String,
        transfer: ReleaseUploadTransfer,
    },
    /// The release has nothing being sent any more.
    Removed { release_id: String },
    /// The files being sent together.
    TotalUpdated(UploadTransfer),
    /// Every transfer, as a reader first reads them.
    Reset(OutboxTransfers),
}

/// The transfers each delivery of the files being sent, or each move of
/// their bytes, measures.
struct SendingTransfers {
    sending: coven::LiveQuery<DbSendingUploads>,
    /// The files being sent as the query last read them; `None` until it
    /// first does.
    read: Option<DbSendingUploads>,
    watch: coven::TransferWatch,
    /// How far each file being sent has got, as the watch last told it.
    progress: BTreeMap<coven::FileId, coven::TransferProgress>,
    throughput: UploadThroughput,
    clock: coven::ClockRef,
}

impl SendingTransfers {
    fn of(
        sending: coven::LiveQuery<DbSendingUploads>,
        watch: coven::TransferWatch,
        clock: coven::ClockRef,
    ) -> Self {
        Self {
            sending,
            read: None,
            watch,
            progress: BTreeMap::new(),
            throughput: UploadThroughput::default(),
            clock,
        }
    }

    /// The transfers once the query next reads or the watch next moves, or
    /// the failure that kept the query from reading; `None` once either has
    /// ended with its store.
    async fn next(&mut self) -> Option<coven::CovenResult<OutboxTransfers>> {
        loop {
            tokio::select! {
                read = self.sending.next() => match read? {
                    Ok(sending) => self.read = Some(sending),
                    Err(error) => return Some(Err(error)),
                },
                changes = self.watch.next() => {
                    super::file_transfers::apply(&mut self.progress, changes?);
                }
            }
            if let Some(sending) = &self.read {
                let now = self.clock.elapsed();
                self.throughput.observe(sending, &self.progress, now);
                return Some(Ok(OutboxTransfers::of(
                    sending,
                    &self.throughput.rates(now),
                    &self.progress,
                )));
            }
        }
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
    pub(crate) fn of(
        sending: coven::LiveQuery<DbSendingUploads>,
        watch: coven::TransferWatch,
        clock: coven::ClockRef,
    ) -> Self {
        Self {
            transfers: SendingTransfers::of(sending, watch, clock),
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
/// stands first, then each time it moves. A release's transfer moving while
/// the total holds wakes nothing here.
pub struct OutboxTotalTransferWatch {
    transfers: SendingTransfers,
    /// What the reader was last told; `None` before the first read.
    told: Option<UploadTransfer>,
}

impl OutboxTotalTransferWatch {
    pub(crate) fn of(
        sending: coven::LiveQuery<DbSendingUploads>,
        watch: coven::TransferWatch,
        clock: coven::ClockRef,
    ) -> Self {
        Self {
            transfers: SendingTransfers::of(sending, watch, clock),
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
        .releases
        .keys()
        .filter(|release_id| !next.releases.contains_key(*release_id))
        .map(|release_id| OutboxTransferChange::Removed {
            release_id: release_id.clone(),
        })
        .collect();
    changes.extend(
        next.releases
            .iter()
            .filter(|(release_id, transfer)| told.releases.get(*release_id) != Some(*transfer))
            .map(|(release_id, transfer)| OutboxTransferChange::Updated {
                release_id: release_id.clone(),
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
