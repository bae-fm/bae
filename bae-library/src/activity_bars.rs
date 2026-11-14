//! How far each Activity transfer line's batch has got in bytes, for the bar
//! each line draws. A live read of the library's tables says how many bytes
//! each batch holds and how many of them are done, and which files are being
//! sent and which releases wait to download; coven's upload watch says how
//! far each file being sent has got, and the download queue's transfers say
//! how far the release downloading has got. A bar is the bytes the tables
//! count done plus each file's or release's size times its latest share,
//! worked out again when any of the three speaks.

use bae_store::DbError;
use bae_transfers::{DownloadTransfer, ReleaseTransferChange, ReleaseTransfersWatch};
use bae_views::{ActivityBytes, DownloadBytes, UploadBytes};
use std::collections::BTreeMap;

/// One transfer line's batch in bytes: the bytes of its releases done, of
/// all of theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchBar {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl BatchBar {
    /// The share of the batch's bytes done, from 0 to 1. A batch of no
    /// bytes has done none of them.
    pub fn fraction(self) -> f64 {
        if self.bytes_total == 0 {
            return 0.0;
        }
        self.bytes_done as f64 / self.bytes_total as f64
    }
}

/// Each transfer line's bar, while its batch holds a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityBars {
    pub uploads: Option<BatchBar>,
    pub pins: Option<BatchBar>,
    pub moving_to_this_device: Option<BatchBar>,
}

/// What the watch has heard from its read and its two transfer watches.
#[derive(Default)]
struct BarsCount {
    /// The tables as the read last found them; `None` before the first read.
    read: Option<ActivityBytes>,
    /// How far each file being sent has got, as the upload watch last told
    /// it.
    sent: BTreeMap<coven::FileId, coven::TransferProgress>,
    /// Each file whose upload finished, with how far it had got, until the
    /// read that stores it arrives. Coven ends a file's progress after the
    /// commit that stores it, but the read can deliver that commit later, and
    /// the bar must not drop in between.
    finished: BTreeMap<coven::FileId, coven::TransferProgress>,
    /// How far each release downloading has got, as the download queue
    /// last told it.
    downloading: BTreeMap<String, DownloadTransfer>,
}

impl BarsCount {
    fn read(&mut self, bytes: ActivityBytes) {
        // A file the read no longer lists as sending is stored in it.
        if let Some(uploads) = &bytes.uploads {
            self.finished
                .retain(|file, _| uploads.sending.iter().any(|sending| sending.file == *file));
        } else {
            self.finished.clear();
        }
        self.read = Some(bytes);
    }

    fn uploads_heard(&mut self, changes: Vec<coven::TransferChange>) {
        for change in &changes {
            match change {
                coven::TransferChange::Removed { file, end: coven::TransferEnd::Finished } => {
                    let stored =
                        self.read.as_ref().and_then(|read| read.uploads.as_ref()).is_some_and(
                            |uploads| uploads.sending.iter().all(|sending| sending.file != *file),
                        );
                    if let (false, Some(progress)) = (stored, self.sent.get(file)) {
                        self.finished.insert(*file, *progress);
                    }
                }
                coven::TransferChange::Updated { file, .. } => {
                    self.finished.remove(file);
                }
                coven::TransferChange::Reset(_) => self.finished.clear(),
                coven::TransferChange::Removed { .. } => {}
            }
        }
        crate::file_transfers::apply(&mut self.sent, changes);
    }

    fn downloads_heard(&mut self, changes: Vec<ReleaseTransferChange<DownloadTransfer>>) {
        bae_transfers::apply_release_changes(&mut self.downloading, changes);
    }

    /// Each line's bar as everything heard so far counts it; `None` before
    /// the first read.
    fn bars(&self) -> Option<ActivityBars> {
        let read = self.read.as_ref()?;
        let downloading = &self.downloading;
        Some(ActivityBars {
            uploads: read
                .uploads
                .as_ref()
                .map(|uploads| upload_bar(uploads, &self.sent, &self.finished)),
            pins: read.pins.as_ref().map(|pins| download_bar(pins, downloading, pin_share)),
            moving_to_this_device: read
                .moving_to_this_device
                .as_ref()
                .map(|moves| download_bar(moves, downloading, move_to_this_device_share)),
        })
    }
}

/// An upload batch's bar: the bytes of its stored files and, of each file
/// being sent, the share of its size its object has moved, which for a file
/// that finished is its last share until a read lists it stored. A file the
/// watch hasn't named yet has moved nothing.
fn upload_bar(
    uploads: &UploadBytes,
    sent: &BTreeMap<coven::FileId, coven::TransferProgress>,
    finished: &BTreeMap<coven::FileId, coven::TransferProgress>,
) -> BatchBar {
    let in_flight: u64 = uploads
        .sending
        .iter()
        .map(|file| {
            let share = sent
                .get(&file.file)
                .or_else(|| finished.get(&file.file))
                .map_or(0.0, |moved| crate::file_transfers::fraction(*moved));
            (file.size as f64 * share) as u64
        })
        .sum();
    BatchBar { bytes_done: uploads.bytes_done + in_flight, bytes_total: uploads.bytes_total }
}

/// A download batch's bar: the sizes of its done releases and, of each
/// release not done, its size times `share` of the transfer running for it,
/// if one runs.
fn download_bar(
    batch: &DownloadBytes,
    downloading: &BTreeMap<String, DownloadTransfer>,
    share: fn(&DownloadTransfer) -> Option<f64>,
) -> BatchBar {
    let in_flight: u64 = batch
        .waiting
        .iter()
        .filter_map(|release| {
            let share = downloading.get(&release.release_id).and_then(share)?;
            Some((release.bytes_total as f64 * share) as u64)
        })
        .sum();
    BatchBar { bytes_done: batch.bytes_done + in_flight, bytes_total: batch.bytes_total }
}

/// The share of a pin's bytes downloaded, if the transfer is a pin.
fn pin_share(transfer: &DownloadTransfer) -> Option<f64> {
    match transfer {
        DownloadTransfer::Pin(progress) => Some(progress.fraction),
        DownloadTransfer::MoveToThisDevice(_) => None,
    }
}

/// The share of a move-to-this-device's bytes downloaded, if the transfer is a
/// move-to-this-device; one with no bytes to download has downloaded none.
fn move_to_this_device_share(transfer: &DownloadTransfer) -> Option<f64> {
    match transfer {
        DownloadTransfer::MoveToThisDevice(progress) if progress.bytes_total == 0 => Some(0.0),
        DownloadTransfer::MoveToThisDevice(progress) => {
            Some(progress.bytes_done as f64 / progress.bytes_total as f64)
        }
        DownloadTransfer::Pin(_) => None,
    }
}

/// Each transfer line's bar for a reader that draws them: once the first
/// read has said what each batch holds, then each time the read or a transfer
/// changes.
pub struct ActivityBarsWatch {
    bytes: coven::LiveQuery<ActivityBytes, DbError>,
    uploads: coven::TransferWatch,
    downloads: ReleaseTransfersWatch<DownloadTransfer>,
    count: BarsCount,
}

impl ActivityBarsWatch {
    pub(crate) fn of(
        bytes: coven::LiveQuery<ActivityBytes, DbError>,
        uploads: coven::TransferWatch,
        downloads: ReleaseTransfersWatch<DownloadTransfer>,
    ) -> Self {
        Self { bytes, uploads, downloads, count: BarsCount::default() }
    }

    /// The bars once the first read has said what each batch holds, then
    /// the bars after each change of the read or a transfer. A failed read is
    /// handed on. `None` once the read or a watch has ended with its store.
    pub async fn next(&mut self) -> Option<Result<ActivityBars, DbError>> {
        loop {
            tokio::select! {
                read = self.bytes.next() => match read? {
                    Ok(bytes) => self.count.read(bytes),
                    Err(error) => return Some(Err(error)),
                },
                changes = self.uploads.next() => self.count.uploads_heard(changes?),
                changes = self.downloads.next() => self.count.downloads_heard(changes?),
            }
            if let Some(bars) = self.count.bars() {
                return Some(Ok(bars));
            }
        }
    }
}

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
