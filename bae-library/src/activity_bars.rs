//! Each Activity transfer line's bar, as the share of its batch's bytes done.
//! A live query of the library's tables gives each line's share done and each
//! file or release that bae is transferring with its share of the batch.
//! coven's upload watch says how far each file being sent has got, and the
//! download queue's transfers say how far the release downloading has got.
//! The watch computes each bar again from the query's latest rows and the
//! latest progress whenever either changes.

use bae_store::live_results::LiveResults;
use bae_store::DbError;
use bae_transfers::{DownloadTransfer, ReleaseTransfersWatch};
use bae_views::{ActivityBarRows, BatchBar};
use std::collections::BTreeMap;

/// Each transfer line's bar: the share of its batch's bytes done, from 0 to
/// 1. A line whose batch holds no release has a bar of 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivityBars {
    pub uploads: f64,
    pub pins: f64,
    pub moving_to_this_device: f64,
}

impl ActivityBars {
    /// The bars of `rows`. Each bar is its batch's share done plus, for each
    /// file or release that bae is transferring, its share of the batch times
    /// the fraction of it that bae has transferred. A file or release whose
    /// progress `sent` or `downloading` does not hold adds nothing.
    pub(crate) fn of(
        rows: &ActivityBarRows,
        sent: &BTreeMap<coven::FileId, coven::TransferProgress>,
        downloading: &BTreeMap<String, DownloadTransfer>,
    ) -> Self {
        Self {
            uploads: bar(&rows.uploads, |file| {
                sent.get(file).map(|progress| crate::file_transfers::fraction(*progress))
            }),
            pins: bar(&rows.pins, |release_id| match downloading.get(release_id)? {
                DownloadTransfer::Pin(progress) => Some(progress.fraction),
                DownloadTransfer::MoveToThisDevice(_) => None,
            }),
            moving_to_this_device: bar(&rows.moving_to_this_device, |release_id| match downloading
                .get(
                release_id,
            )? {
                DownloadTransfer::MoveToThisDevice(progress) => Some(progress.fraction),
                DownloadTransfer::Pin(_) => None,
            }),
        }
    }
}

/// `rows`' share done plus each moving key's share times the fraction
/// `fraction` gives it, or nothing when it gives none.
fn bar<Key>(rows: &BatchBar<Key>, fraction: impl Fn(&Key) -> Option<f64>) -> f64 {
    rows.done
        + rows
            .moving
            .iter()
            .map(|moving| moving.share * fraction(&moving.key).unwrap_or(0.0))
            .sum::<f64>()
}

/// Each transfer line's bar for a reader that draws them: once the query
/// first reads, then each time the query or a transfer changes.
pub struct ActivityBarsWatch {
    rows: LiveResults<coven::LiveQuery<ActivityBarRows, DbError>>,
    uploads: coven::TransferWatch,
    downloads: ReleaseTransfersWatch<DownloadTransfer>,
    /// The rows as the query last read them; `None` until it first does.
    read: Option<ActivityBarRows>,
    /// How far each file being sent has got, as the upload watch last told
    /// it.
    sent: BTreeMap<coven::FileId, coven::TransferProgress>,
    /// How far each release downloading has got, as the download queue last
    /// told it.
    downloading: BTreeMap<String, DownloadTransfer>,
}

impl ActivityBarsWatch {
    pub(crate) fn of(
        rows: LiveResults<coven::LiveQuery<ActivityBarRows, DbError>>,
        uploads: coven::TransferWatch,
        downloads: ReleaseTransfersWatch<DownloadTransfer>,
    ) -> Self {
        Self {
            rows,
            uploads,
            downloads,
            read: None,
            sent: BTreeMap::new(),
            downloading: BTreeMap::new(),
        }
    }

    /// Waits until the query reads or a transfer moves, once the query has
    /// read for the first time, and returns the bars, or the failure that
    /// kept the query from reading. Returns `None` once the query or a
    /// transfer watch stops because its store closed.
    pub async fn next(&mut self) -> Option<Result<ActivityBars, DbError>> {
        loop {
            tokio::select! {
                read = self.rows.next() => match read? {
                    Ok(rows) => self.read = Some(rows),
                    Err(error) => return Some(Err(error)),
                },
                changes = self.uploads.next() => {
                    crate::file_transfers::apply(&mut self.sent, changes?);
                }
                downloading = self.downloads.next() => self.downloading = downloading?,
            }
            if let Some(rows) = &self.read {
                return Some(Ok(ActivityBars::of(rows, &self.sent, &self.downloading)));
            }
        }
    }
}

#[cfg(test)]
#[path = "activity_bars_tests.rs"]
mod tests;
