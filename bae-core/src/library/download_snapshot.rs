//! The download (pin) queue's types. The queue is the `download_queue`
//! table: the Downloads pane and a release's download status read it, and the
//! bytes the active download moves travel apart, keyed by release.

use super::release_queue::{
    ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot,
    ReleaseQueueState,
};

use super::LibraryError;

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
    /// How fast the download has gone since it began, in bytes per second;
    /// `None` until it has moved bytes.
    pub throughput_bps: Option<u64>,
}

impl DownloadTransferProgress {
    pub fn new(release_id: &str, bytes_done: u64, bytes_total: u64) -> Result<Self, LibraryError> {
        if bytes_total == 0 {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: byte total is zero"
            )));
        }
        if bytes_done > bytes_total {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: bytes done exceeds byte total"
            )));
        }
        if bytes_total > i64::MAX as u64 {
            return Err(LibraryError::Storage(format!(
                "pin release {release_id}: byte total exceeds display range"
            )));
        }
        Ok(Self {
            bytes_done,
            bytes_total,
            fraction: bytes_done as f64 / bytes_total as f64,
            throughput_bps: None,
        })
    }
}

/// Where a download began, what its rate is measured from: the time it
/// began and the bytes it held then, beside the bytes it last held. The bytes
/// a download holds move as each file lands, so its rate is what it moved
/// since it began over the time since.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DownloadRate {
    began: std::time::SystemTime,
    bytes_then: u64,
    bytes_last: u64,
}

impl DownloadRate {
    pub(crate) fn begin(now: std::time::SystemTime, bytes_done: u64) -> Self {
        Self {
            began: now,
            bytes_then: bytes_done,
            bytes_last: bytes_done,
        }
    }

    /// `progress` read at `now`, with the rate it has gone at since the
    /// download began. Bytes the cache gave up meanwhile begin the
    /// measurement again from what it holds now.
    pub(crate) fn measure(
        &mut self,
        progress: DownloadTransferProgress,
        now: std::time::SystemTime,
    ) -> DownloadTransferProgress {
        if progress.bytes_done < self.bytes_last {
            *self = Self::begin(now, progress.bytes_done);
        }
        self.bytes_last = progress.bytes_done;
        let moved = progress.bytes_done - self.bytes_then;
        let elapsed = now
            .duration_since(self.began)
            .expect("download clock regressed");
        let throughput_bps = (moved > 0 && !elapsed.is_zero())
            .then(|| (moved as f64 / elapsed.as_secs_f64()) as u64);
        DownloadTransferProgress {
            throughput_bps,
            ..progress
        }
    }
}

/// A release to queue for download.
pub type DownloadEntry = ReleaseQueueEntry<()>;
pub type DownloadProgress = ReleaseQueueProgress;
/// A download as the Downloads pane lists it: its phase without the active
/// one's bytes, which travel apart, keyed by release
/// ([`DownloadTransfersWatch`](super::DownloadTransfersWatch)).
pub type QueuedDownload = ReleaseQueueOp<(), ()>;
pub type DownloadPhase = ReleaseQueueState<()>;
pub type DownloadRow = ReleaseQueueRow<(), ()>;
/// The download queue as the Downloads pane shows it: its entries and their
/// phases, each beside its release. It changes as entries come, go, start or
/// fail, not as bytes arrive.
pub type DownloadSnapshot = ReleaseQueueSnapshot<(), ()>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, SystemTime};

    fn progress(bytes_done: u64) -> DownloadTransferProgress {
        DownloadTransferProgress::new("release", bytes_done, 1_000).unwrap()
    }

    /// A download's rate is what it moved since it began over the time since,
    /// none before it moves, and a drop in what the cache holds begins it
    /// again.
    #[test]
    fn a_download_rate_counts_from_where_it_began() {
        let began = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut rate = DownloadRate::begin(began, 100);
        assert_eq!(rate.measure(progress(100), began).throughput_bps, None);
        let later = began + Duration::from_secs(2);
        assert_eq!(rate.measure(progress(500), later).throughput_bps, Some(200));
        let dropped = later + Duration::from_secs(1);
        assert_eq!(rate.measure(progress(300), dropped).throughput_bps, None);
        let after = dropped + Duration::from_secs(1);
        assert_eq!(rate.measure(progress(400), after).throughput_bps, Some(100));
    }
}
