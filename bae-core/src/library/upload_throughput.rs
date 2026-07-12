//! Rolling-window throughput tracker for the cloud-upload pipeline. The
//! `ReleaseUploadObserver` feeds it from two callbacks: `on_blob_upload_progress`
//! records the byte delta since the file's previous report, so the rate moves
//! mid-upload, and `on_blob_uploaded` credits whatever the progress reports had
//! not counted yet (the tail past the last report, or a whole file that finished
//! between coalescing ticks). The snapshot builder reads the rate at emit time.
//! Samples age out of the window so an idle queue drops the displayed rate back
//! to zero.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_WINDOW: Duration = Duration::from_secs(10);

/// Rolling-window byte counter. Each sample is `(when, bytes)`; the read sums
/// every sample whose age is within the window and divides by the window
/// length, so idle stretches naturally drop the rate.
pub struct UploadThroughput {
    samples: Mutex<VecDeque<(Instant, u64)>>,
    window: Duration,
}

impl UploadThroughput {
    pub fn new() -> Self {
        Self::with_window(DEFAULT_WINDOW)
    }

    pub fn with_window(window: Duration) -> Self {
        Self {
            samples: Mutex::new(VecDeque::new()),
            window,
        }
    }

    /// Record `bytes` transferred. Fed from both of the observer's upload
    /// callbacks: `on_blob_upload_progress` passes the delta since the file's
    /// previous report; `on_blob_uploaded` passes the bytes no progress report
    /// counted.
    pub fn record(&self, bytes: u64) {
        self.record_at(bytes, Instant::now());
    }

    fn record_at(&self, bytes: u64, now: Instant) {
        let mut s = self.samples.lock().unwrap();
        prune(&mut s, now, self.window);
        s.push_back((now, bytes));
    }

    /// Bytes per second over the rolling window. Zero when no samples have
    /// landed in the window.
    pub fn bytes_per_sec(&self) -> u64 {
        self.bytes_per_sec_at(Instant::now())
    }

    fn bytes_per_sec_at(&self, now: Instant) -> u64 {
        let mut s = self.samples.lock().unwrap();
        prune(&mut s, now, self.window);
        let total: u64 = s.iter().map(|(_, b)| *b).sum();
        let secs = self.window.as_secs_f64();
        (total as f64 / secs) as u64
    }
}

impl Default for UploadThroughput {
    fn default() -> Self {
        Self::new()
    }
}

fn prune(samples: &mut VecDeque<(Instant, u64)>, now: Instant, window: Duration) {
    while let Some(&(t, _)) = samples.front() {
        if now.saturating_duration_since(t) > window {
            samples.pop_front();
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tracker_reports_zero() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        assert_eq!(t.bytes_per_sec(), 0);
    }

    #[test]
    fn samples_in_window_sum_and_divide_by_window() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let now = Instant::now();
        t.record_at(5_000_000, now);
        t.record_at(5_000_000, now);
        // 10 MB over 10s = 1 MB/s.
        assert_eq!(t.bytes_per_sec_at(now), 1_000_000);
    }

    #[test]
    fn samples_older_than_window_are_dropped() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let start = Instant::now();
        t.record_at(10_000_000, start);
        let later = start + Duration::from_secs(11);
        // Sample aged out: rate is back to zero.
        assert_eq!(t.bytes_per_sec_at(later), 0);
    }
}
