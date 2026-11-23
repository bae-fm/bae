//! The download (pin) queue's types. The queue is the `download_queue`
//! table: the Downloads pane and a release's download status read it, and the
//! bytes the active download moves travel apart, keyed by release.

use crate::release_queue::{
    ReleaseQueueEntry, ReleaseQueueOp, ReleaseQueueProgress, ReleaseQueueRow, ReleaseQueueSnapshot,
    ReleaseQueueState,
};

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
    pub fn new(
        release_id: &str,
        bytes_done: u64,
        bytes_total: u64,
    ) -> Result<Self, DownloadProgressError> {
        let refused =
            |problem| DownloadProgressError { release_id: release_id.to_string(), problem };
        if bytes_total == 0 {
            return Err(refused("byte total is zero"));
        }
        if bytes_done > bytes_total {
            return Err(refused("bytes done exceeds byte total"));
        }
        if bytes_total > i64::MAX as u64 {
            return Err(refused("byte total exceeds display range"));
        }
        Ok(Self {
            bytes_done,
            bytes_total,
            fraction: bae_types::progress::share_done(bytes_done, bytes_total),
            throughput_bps: None,
        })
    }
}

/// A pin's byte counts that make no progress: its total is zero or too
/// large to show, or it has done more than its total.
#[derive(Debug, thiserror::Error)]
#[error("pin release {release_id}: {problem}")]
pub struct DownloadProgressError {
    release_id: String,
    problem: &'static str,
}

/// Where a download began, what its rate is measured from: the time on
/// coven's monotonic clock (`elapsed()`) it began and the bytes it held then, beside the bytes it last held. The bytes
/// a download holds move as each file lands, so its rate is what it moved
/// since it began over the time since.
#[derive(Debug, Clone, Copy)]
pub struct DownloadRate {
    began: std::time::Duration,
    bytes_then: u64,
    bytes_last: u64,
}

impl DownloadRate {
    pub fn begin(now: std::time::Duration, bytes_done: u64) -> Self {
        Self { began: now, bytes_then: bytes_done, bytes_last: bytes_done }
    }

    /// `progress` read at `now`, with the rate it has gone at since the
    /// download began. Bytes the cache gave up meanwhile begin the
    /// measurement again from what it holds now.
    pub fn measure(
        &mut self,
        progress: DownloadTransferProgress,
        now: std::time::Duration,
    ) -> DownloadTransferProgress {
        if progress.bytes_done < self.bytes_last {
            *self = Self::begin(now, progress.bytes_done);
        }
        self.bytes_last = progress.bytes_done;
        let moved = progress.bytes_done - self.bytes_then;
        let elapsed = now.checked_sub(self.began).expect("download clock regressed");
        let throughput_bps = (moved > 0 && !elapsed.is_zero())
            .then(|| (moved as f64 / elapsed.as_secs_f64()) as u64);
        DownloadTransferProgress { throughput_bps, ..progress }
    }
}

/// How far one release being moved to this device has come: the bytes of the file
/// coven is downloading into the person's folder, and of the whole release.
/// Held only in memory while the copies download, before the write that
/// switches the files; no table holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveToThisDeviceTransferProgress {
    /// The current file, once coven starts downloading it.
    pub file: Option<MoveToThisDeviceFileProgress>,
    /// The bytes of every file of the release that are down.
    pub bytes_done: u64,
    /// The bytes of every file the release downloads.
    pub bytes_total: u64,
    /// The share of the release's bytes that are down, from 0 to 1, which
    /// its bar fills to. A release with no bytes to download has none down.
    pub fraction: f64,
}

/// How far coven has downloaded the current file into the chosen folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveToThisDeviceFileProgress {
    pub file: coven::FileId,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

impl MoveToThisDeviceFileProgress {
    pub fn fraction(self) -> f64 {
        bae_types::progress::share_done(self.bytes_done, self.bytes_total)
    }
}

impl MoveToThisDeviceTransferProgress {
    /// Nothing down yet, of a release of `bytes_total`.
    pub fn begin(bytes_total: u64) -> Self {
        Self { file: None, bytes_done: 0, bytes_total, fraction: 0.0 }
    }

    /// The progress coven reports of one step of preparing the copies.
    pub fn of(progress: coven::Progress<coven::DeviceOnlyStep>) -> Self {
        let coven::DeviceOnlyStep::Downloading { file, before, all } = progress.step;
        let bytes_done = before + progress.done;
        Self {
            file: Some(MoveToThisDeviceFileProgress {
                file,
                bytes_done: progress.done,
                bytes_total: progress.total,
            }),
            bytes_done,
            bytes_total: all,
            fraction: bae_types::progress::share_done(bytes_done, all),
        }
    }
}

/// How far the active download has come, by what it does: a pin's bytes and
/// rate, or a move-to-this-device's bytes of the release and of its current file.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadTransfer {
    Pin(DownloadTransferProgress),
    MoveToThisDevice(MoveToThisDeviceTransferProgress),
}

/// What a queued download does, and what it needs to.
#[derive(Debug, Clone, PartialEq)]
pub enum DownloadRequest {
    /// Keep the release's files whole on this device.
    Pin,
    /// Download the release's files into a folder of their own below
    /// `target_dir`, and switch them to this device only.
    #[cfg(desktop)]
    MoveToThisDevice { target_dir: std::path::PathBuf },
}

/// A release to queue for download.
pub type QueuedPin = ReleaseQueueEntry<DownloadRequest>;
pub type DownloadProgress = ReleaseQueueProgress;
/// A download as the Downloads pane lists it: its phase without the active
/// one's bytes, which travel apart, keyed by release
/// (the library's download transfers watch).
pub type QueuedDownload = ReleaseQueueOp<DownloadRequest, ()>;
pub type DownloadPhase = ReleaseQueueState<()>;
pub type DownloadRow = ReleaseQueueRow<DownloadRequest, ()>;
/// The download queue as the Downloads pane shows it: its entries and their
/// phases, each with its release. The snapshot changes as entries come, go,
/// start or fail, not as bytes arrive.
pub type DownloadSnapshot = ReleaseQueueSnapshot<DownloadRequest, ()>;

#[cfg(test)]
#[path = "download_snapshot_tests.rs"]
mod tests;
