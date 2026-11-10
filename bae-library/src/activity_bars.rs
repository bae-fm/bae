//! How far each Activity transfer line's batch has got in bytes, for the bar
//! each line draws. A live read of the library's tables says which releases
//! each batch holds, their sizes and which are done or stored; coven's upload
//! watch says how far each file being sent has got; the download queue's
//! transfers say how far the release downloading has got. The three wake in
//! any order: a release's transfer can end before the read counts it done.
//! So each release counts the most bytes any wake has shown it while its
//! batch lasts, and the bytes done never go back until the batch ends.

use bae_store::DbError;
use bae_transfers::{DownloadTransfer, ReleaseTransferChange, ReleaseTransfersWatch};
use bae_views::{ActivityBytes, BatchReleases, DownloadRelease, UploadRelease};
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
    pub making_local: Option<BatchBar>,
}

/// One release's bytes as its line counts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counted {
    bytes_done: u64,
    bytes_total: u64,
}

/// One line's batch as counted so far: its number and each release's bytes,
/// each the most any wake has shown it while the batch lasts.
struct LineCount {
    number: u64,
    releases: BTreeMap<String, Counted>,
}

impl LineCount {
    /// Count `now` into `held`, the line's count before this wake: each
    /// release of the batch numbered `number` with its bytes as this wake
    /// shows them, or `None` for one the library no longer holds. A release
    /// keeps the most bytes it has counted while the batch lasts, so neither
    /// its bytes done nor its total go back, and a release that left the
    /// library keeps what it had counted. A batch numbered anew starts from
    /// nothing.
    fn count(
        held: &mut Option<LineCount>,
        number: u64,
        now: impl Iterator<Item = (String, Option<Counted>)>,
    ) -> BatchBar {
        let before = match held.take() {
            Some(line) if line.number == number => line.releases,
            Some(_) | None => BTreeMap::new(),
        };
        let mut releases = BTreeMap::new();
        let mut bar = BatchBar { bytes_done: 0, bytes_total: 0 };
        for (release_id, now) in now {
            let counted = match (before.get(&release_id), now) {
                (Some(was), Some(now)) => Counted {
                    bytes_done: was.bytes_done.max(now.bytes_done),
                    bytes_total: was.bytes_total.max(now.bytes_total),
                },
                (None, Some(now)) => now,
                (Some(was), None) => *was,
                // Gone before any wake counted it: it has no bytes to keep.
                (None, None) => continue,
            };
            bar.bytes_done += counted.bytes_done;
            bar.bytes_total += counted.bytes_total;
            releases.insert(release_id, counted);
        }
        *held = Some(LineCount { number, releases });
        bar
    }
}

/// What the watch has heard from its read and its two transfer watches, and
/// each line's count so far.
#[derive(Default)]
struct BarsCount {
    /// The tables as the read last found them; `None` before the first read.
    read: Option<ActivityBytes>,
    /// How far each file being sent has got, as the upload watch last told
    /// it.
    sent: BTreeMap<coven::FileId, coven::TransferProgress>,
    /// How far each release downloading has got, as the download queue
    /// last told it.
    downloading: BTreeMap<String, DownloadTransfer>,
    uploads: Option<LineCount>,
    pins: Option<LineCount>,
    making_local: Option<LineCount>,
}

impl BarsCount {
    fn read(&mut self, bytes: ActivityBytes) {
        self.read = Some(bytes);
    }

    fn uploads_heard(&mut self, changes: Vec<coven::TransferChange>) {
        crate::file_transfers::apply(&mut self.sent, changes);
    }

    fn downloads_heard(&mut self, changes: Vec<ReleaseTransferChange<DownloadTransfer>>) {
        bae_transfers::apply_release_changes(&mut self.downloading, changes);
    }

    /// Each line's bar as everything heard so far counts it; `None` before
    /// the first read.
    fn bars(&mut self) -> Option<ActivityBars> {
        let read = self.read.as_ref()?;
        let sent = &self.sent;
        let downloading = &self.downloading;
        Some(ActivityBars {
            uploads: count_line(&mut self.uploads, read.uploads.as_ref(), |_, release| {
                upload_now(release, sent)
            }),
            pins: count_line(&mut self.pins, read.pins.as_ref(), |release_id, release| {
                download_now(release, downloading.get(release_id).and_then(pin_share))
            }),
            making_local: count_line(
                &mut self.making_local,
                read.making_local.as_ref(),
                |release_id, release| {
                    download_now(release, downloading.get(release_id).and_then(make_local_share))
                },
            ),
        })
    }
}

/// Count the batch `read` holds into `held` with each release's bytes as
/// `now` says them, keyed by release; with no batch, the line has no bar and
/// its count is dropped.
fn count_line<Release>(
    held: &mut Option<LineCount>,
    read: Option<&BatchReleases<Release>>,
    now: impl Fn(&String, &Release) -> Counted,
) -> Option<BatchBar> {
    let Some(batch) = read else {
        *held = None;
        return None;
    };
    let releases = batch.releases.iter().map(|(release_id, release)| {
        (release_id.clone(), release.as_ref().map(|release| now(release_id, release)))
    });
    Some(LineCount::count(held, batch.number, releases))
}

/// An upload's bytes now: its stored files' and, of each file being sent,
/// the share of its size its object has moved. A file the watch hasn't
/// named yet has moved nothing.
fn upload_now(
    release: &UploadRelease,
    sent: &BTreeMap<coven::FileId, coven::TransferProgress>,
) -> Counted {
    let in_flight: u64 = release
        .sending
        .iter()
        .map(|(file, size)| {
            let share = sent.get(file).map_or(0.0, |moved| crate::file_transfers::fraction(*moved));
            (*size as f64 * share) as u64
        })
        .sum();
    Counted { bytes_done: release.bytes_stored + in_flight, bytes_total: release.bytes_total }
}

/// A download's bytes now: all of its size once done, else `share` of its
/// size while a transfer of its line's kind runs, else none.
fn download_now(release: &DownloadRelease, share: Option<f64>) -> Counted {
    let bytes_done = if release.done {
        release.bytes_total
    } else {
        share.map_or(0, |share| (release.bytes_total as f64 * share) as u64)
    };
    Counted { bytes_done, bytes_total: release.bytes_total }
}

/// The share of a pin's bytes downloaded, if the transfer is a pin.
fn pin_share(transfer: &DownloadTransfer) -> Option<f64> {
    match transfer {
        DownloadTransfer::Pin(progress) => Some(progress.fraction),
        DownloadTransfer::MakeLocal(_) => None,
    }
}

/// The share of a make-local's bytes downloaded, if the transfer is a
/// make-local; one with no bytes to download has downloaded none.
fn make_local_share(transfer: &DownloadTransfer) -> Option<f64> {
    match transfer {
        DownloadTransfer::MakeLocal(progress) if progress.bytes_total == 0 => Some(0.0),
        DownloadTransfer::MakeLocal(progress) => {
            Some(progress.bytes_done as f64 / progress.bytes_total as f64)
        }
        DownloadTransfer::Pin(_) => None,
    }
}

/// Each transfer line's bar for a reader that draws them: once the first
/// read has said what each batch holds, then each time a bar changes.
pub struct ActivityBarsWatch {
    bytes: coven::LiveQuery<ActivityBytes, DbError>,
    uploads: coven::TransferWatch,
    downloads: ReleaseTransfersWatch<DownloadTransfer>,
    count: BarsCount,
    /// What the reader was last told; `None` before the first read, and
    /// after a failed one.
    told: Option<ActivityBars>,
}

impl ActivityBarsWatch {
    pub(crate) fn of(
        bytes: coven::LiveQuery<ActivityBytes, DbError>,
        uploads: coven::TransferWatch,
        downloads: ReleaseTransfersWatch<DownloadTransfer>,
    ) -> Self {
        Self { bytes, uploads, downloads, count: BarsCount::default(), told: None }
    }

    /// The bars once the first read has said what each batch holds, then
    /// the next bars that differ. A failed read is handed on, and the bars
    /// after it are told whether or not they changed; what each release had
    /// counted stays, since the batch numbers say whether its batch lasts.
    /// `None` once the read or a watch has ended with its store.
    pub async fn next(&mut self) -> Option<Result<ActivityBars, DbError>> {
        loop {
            tokio::select! {
                read = self.bytes.next() => match read? {
                    Ok(bytes) => self.count.read(bytes),
                    Err(error) => {
                        self.told = None;
                        return Some(Err(error));
                    }
                },
                changes = self.uploads.next() => self.count.uploads_heard(changes?),
                changes = self.downloads.next() => self.count.downloads_heard(changes?),
            }
            let Some(bars) = self.count.bars() else {
                continue;
            };
            if self.told == Some(bars) {
                continue;
            }
            self.told = Some(bars);
            return Some(Ok(bars));
        }
    }
}

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
