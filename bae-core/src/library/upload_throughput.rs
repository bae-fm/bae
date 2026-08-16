//! Rolling-window throughput tracker for the cloud-upload pipeline. The
//! `ReleaseUploadObserver` feeds it from `on_blob_upload_progress`, recording the
//! byte delta since the file's previous report. Coven sends the exact final
//! provider count through that callback before completion, so the same path
//! accounts for the whole upload. The snapshot builder reads the rate at emit
//! time. Samples age out of the window so an idle queue drops the displayed rate
//! back to zero.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_WINDOW: Duration = Duration::from_secs(10);

struct ThroughputState {
    samples: VecDeque<(Instant, u64)>,
    active_uploads: u32,
    measurement_started: Option<Instant>,
}

/// Rolling-window byte counter. Each sample is `(when, bytes)`; while the first
/// window fills, the rate uses the elapsed time since the transfer began. Once
/// full, it uses the configured window. A new batch starts a new measurement so
/// idle time between files cannot depress its first reports.
pub struct UploadThroughput {
    state: Mutex<ThroughputState>,
    window: Duration,
}

impl UploadThroughput {
    pub fn new() -> Self {
        Self::with_window(DEFAULT_WINDOW)
    }

    pub fn with_window(window: Duration) -> Self {
        Self {
            state: Mutex::new(ThroughputState {
                samples: VecDeque::new(),
                active_uploads: 0,
                measurement_started: None,
            }),
            window,
        }
    }

    /// Start one provider transfer. Concurrent transfers share one aggregate
    /// measurement; the first transfer after an idle interval resets it.
    pub fn begin(&self) {
        self.begin_at(Instant::now());
    }

    fn begin_at(&self, now: Instant) {
        let mut state = self.state.lock().unwrap();
        if state.active_uploads == 0 {
            state.samples.clear();
            state.measurement_started = Some(now);
        }
        state.active_uploads = state
            .active_uploads
            .checked_add(1)
            .expect("active upload count overflow");
    }

    /// Finish one provider transfer, whether it completed or failed.
    pub fn end(&self) {
        let mut state = self.state.lock().unwrap();
        state.active_uploads = state
            .active_uploads
            .checked_sub(1)
            .expect("provider transfer ended without a matching start");
    }

    /// Record the provider-byte delta from one coalesced upload-progress report.
    pub fn record(&self, bytes: u64) {
        self.record_at(bytes, Instant::now());
    }

    fn record_at(&self, bytes: u64, now: Instant) {
        let mut state = self.state.lock().unwrap();
        assert!(
            state.active_uploads > 0,
            "provider bytes arrived without an active transfer"
        );
        prune(&mut state.samples, now, self.window);
        state.samples.push_back((now, bytes));
    }

    /// Bytes per second over the rolling window. Zero when no samples have
    /// landed in the window.
    pub fn bytes_per_sec(&self) -> u64 {
        self.bytes_per_sec_at(Instant::now())
    }

    fn bytes_per_sec_at(&self, now: Instant) -> u64 {
        let mut state = self.state.lock().unwrap();
        if state.active_uploads == 0 {
            return 0;
        }
        prune(&mut state.samples, now, self.window);
        let total = state
            .samples
            .iter()
            .try_fold(0u64, |total, (_, bytes)| total.checked_add(*bytes))
            .expect("throughput byte sample overflow");
        let started = state
            .measurement_started
            .expect("an active upload has a measurement start");
        let elapsed = now
            .checked_duration_since(started)
            .expect("throughput clock regressed before measurement start")
            .min(self.window);
        if elapsed.is_zero() {
            return 0;
        }
        let secs = elapsed.as_secs_f64();
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
        let age = now
            .checked_duration_since(t)
            .expect("throughput clock regressed before a byte sample");
        if age > window {
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
        t.begin_at(now - Duration::from_secs(10));
        t.record_at(5_000_000, now);
        t.record_at(5_000_000, now);
        // 10 MB over 10s = 1 MB/s.
        assert_eq!(t.bytes_per_sec_at(now), 1_000_000);
    }

    #[test]
    fn a_new_transfer_uses_its_elapsed_time_until_the_window_fills() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        t.begin_at(started);
        let first_tick = started + Duration::from_millis(500);
        t.record_at(500_000, first_tick);

        assert_eq!(t.bytes_per_sec_at(first_tick), 1_000_000);
    }

    #[test]
    fn samples_older_than_window_are_dropped() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let start = Instant::now();
        t.begin_at(start);
        t.record_at(10_000_000, start);
        let later = start + Duration::from_secs(11);
        // Sample aged out: rate is back to zero.
        assert_eq!(t.bytes_per_sec_at(later), 0);
    }

    #[test]
    fn ending_the_last_transfer_hides_the_rate_and_a_new_batch_starts_fresh() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let first_start = Instant::now();
        t.begin_at(first_start);
        let first_tick = first_start + Duration::from_secs(1);
        t.record_at(1_000_000, first_tick);
        assert_eq!(t.bytes_per_sec_at(first_tick), 1_000_000);

        t.end();
        assert_eq!(t.bytes_per_sec_at(first_tick), 0);

        let second_start = first_start + Duration::from_secs(20);
        t.begin_at(second_start);
        let second_tick = second_start + Duration::from_secs(1);
        t.record_at(2_000_000, second_tick);
        assert_eq!(t.bytes_per_sec_at(second_tick), 2_000_000);
    }

    #[test]
    #[should_panic(expected = "provider bytes arrived without an active transfer")]
    fn provider_bytes_require_an_active_transfer() {
        UploadThroughput::with_window(Duration::from_secs(10)).record(1);
    }

    #[test]
    #[should_panic(expected = "throughput byte sample overflow")]
    fn aggregate_samples_cannot_wrap_their_byte_counter() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let now = Instant::now();
        tracker.begin_at(now - Duration::from_secs(1));
        tracker.record_at(u64::MAX, now);
        tracker.record_at(1, now);

        tracker.bytes_per_sec_at(now);
    }

    #[test]
    #[should_panic(expected = "throughput clock regressed before measurement start")]
    fn measurement_clock_cannot_regress() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        tracker.begin_at(started);

        tracker.bytes_per_sec_at(started - Duration::from_millis(1));
    }

    #[test]
    #[should_panic(expected = "throughput clock regressed before a byte sample")]
    fn sample_clock_cannot_regress() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        tracker.begin_at(started);
        tracker.record_at(1, started + Duration::from_millis(1));

        tracker.bytes_per_sec_at(started);
    }
}
