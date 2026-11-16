//! How far and how fast the uploads go, for a reader that draws it: each
//! release with files being sent, and the files being sent together. A live
//! query of the files being sent says which files and releases move; how far
//! each file has got is coven's upload watch, and how fast is measured here
//! from it. Neither is held by any table.

use super::upload_throughput::UploadThroughput;
use super::UploadBar;
use bae_records::DbSendingUploads;
use bae_store::live_results::LiveResults;
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
    pub releases: BTreeMap<String, ReleaseUploadTransfer>,
    pub total: UploadTransfer,
}

impl OutboxTransfers {
    /// The transfers of the files `sending` lists: each file as far as
    /// `progress` has it and at its rate in `rates`, a release with its
    /// stored files and its files being sent together. A file the watch or
    /// the rates don't name yet has moved nothing. A release whose bytes
    /// done pass its total is a bug, which goes to `diagnostics`; its bar
    /// shows full.
    pub(crate) fn of(
        sending: &DbSendingUploads,
        rates: &HashMap<coven::FileId, u64>,
        progress: &BTreeMap<coven::FileId, coven::TransferProgress>,
        diagnostics: &bae_diagnostics::Diagnostics,
    ) -> Self {
        let add = |sum: &mut u64, value: u64| {
            *sum = sum.checked_add(value).expect("upload bytes overflow");
        };
        let mut releases: BTreeMap<String, ReleaseUploadTransfer> = sending
            .releases
            .iter()
            .map(|release| {
                (
                    release.release_id.clone(),
                    ReleaseUploadTransfer {
                        bar: UploadBar {
                            bytes_done: release.bytes_stored,
                            bytes_total: release.bytes_total,
                        },
                        throughput_bps: 0,
                    },
                )
            })
            .collect();
        let mut total = UploadTransfer {
            bar: (!sending.files.is_empty()).then_some(UploadBar { bytes_done: 0, bytes_total: 0 }),
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
                let size =
                    file.file_size.expect("a release file being sent has its size read with it");
                let sent = moved.map_or(0.0, super::file_transfers::fraction);
                add(&mut release.bar.bytes_done, (size as f64 * sent) as u64);
            }
            add(&mut total.throughput_bps, rate);
            if let (Some(bar), Some(moved)) = (&mut total.bar, moved) {
                add(&mut bar.bytes_done, moved.bytes_transferred);
                add(&mut bar.bytes_total, moved.bytes_total);
            }
        }
        for (release_id, release) in &mut releases {
            let UploadBar { bytes_done, bytes_total } = release.bar;
            if bytes_done > bytes_total {
                diagnostics.report_bug(
                    "draw a release's upload bar",
                    &format!(
                        "release {release_id}: {bytes_done} bytes done pass its total of \
                         {bytes_total}"
                    ),
                );
                release.bar.bytes_done = bytes_total;
            }
        }
        Self { releases, total }
    }
}

/// Every transfer, measured each time the query of the files being sent
/// reads or their bytes move, for a reader that draws each one.
pub struct OutboxTransfersWatch {
    sending: LiveResults<coven::LiveQuery<DbSendingUploads, bae_store::DbError>>,
    /// The files being sent as the query last read them; `None` until it
    /// first does.
    read: Option<DbSendingUploads>,
    watch: coven::TransferWatch,
    /// How far each file being sent has got, as the watch last told it.
    progress: BTreeMap<coven::FileId, coven::TransferProgress>,
    throughput: UploadThroughput,
    clock: coven::ClockRef,
    diagnostics: bae_diagnostics::Diagnostics,
}

impl OutboxTransfersWatch {
    pub(crate) fn of(
        sending: LiveResults<coven::LiveQuery<DbSendingUploads, bae_store::DbError>>,
        watch: coven::TransferWatch,
        clock: coven::ClockRef,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self {
            sending,
            read: None,
            watch,
            progress: BTreeMap::new(),
            throughput: UploadThroughput::default(),
            clock,
            diagnostics,
        }
    }

    /// Every release's transfer and the total, once the query next reads or
    /// the watch next moves, or the failure that kept the query from
    /// reading; `None` once either has ended with its store.
    pub async fn next(&mut self) -> Option<Result<OutboxTransfers, bae_store::DbError>> {
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
                    &self.diagnostics,
                )));
            }
        }
    }
}

#[cfg(test)]
#[path = "outbox_watch_tests.rs"]
mod tests;
