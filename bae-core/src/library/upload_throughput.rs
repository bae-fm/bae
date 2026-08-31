//! Rolling-window throughput tracker for the cloud-upload pipeline. The
//! `ReleaseUploadObserver` feeds it as coven first consumes plaintext into its
//! durable spool and then sends the encrypted spool to the provider. Each blob
//! has one current phase measurement, reset at the phase boundary so preparation
//! bytes cannot distort provider-upload speed or ETA. Samples age out of the
//! window so an idle queue drops the displayed rate back to zero.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const DEFAULT_WINDOW: Duration = Duration::from_secs(10);

#[derive(Default)]
struct MeasurementState {
    samples: VecDeque<(Instant, u64)>,
    measurement_started: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransferPhase {
    Preparing,
    Uploading,
}

struct ActiveMeasurement {
    phase: TransferPhase,
    measurement: MeasurementState,
}

#[derive(Default)]
struct ThroughputState {
    uploads: HashMap<crate::library::outbox_snapshot::UploadBlobKey, ActiveMeasurement>,
}

/// Rolling-window byte counter. Each sample is `(when, bytes)`; while the first
/// window fills, the rate uses the elapsed time since the transfer began. Once
/// full, it uses the configured window. A new batch starts a new measurement so
/// idle time between files cannot depress its first reports.
pub struct UploadThroughput {
    state: Mutex<ThroughputState>,
    window: Duration,
}

/// One same-instant reading of the aggregate rate and each active blob's rate.
/// Snapshot construction sums blob rates by release without relabeling the
/// queue-wide measurement as every release's speed.
#[derive(Default)]
pub(crate) struct UploadRates {
    pub(crate) aggregate_bps: u64,
    pub(crate) provider_bps: u64,
    by_upload: HashMap<crate::library::outbox_snapshot::UploadBlobKey, u64>,
}

impl UploadRates {
    pub(crate) fn for_uploads<'a>(
        &self,
        uploads: impl IntoIterator<Item = &'a crate::library::outbox_snapshot::UploadBlobKey>,
    ) -> u64 {
        uploads
            .into_iter()
            .filter_map(|upload| self.by_upload.get(upload))
            .try_fold(0u64, |total, rate| total.checked_add(*rate))
            .expect("per-release throughput cannot overflow")
    }
}

impl UploadThroughput {
    pub fn new() -> Self {
        Self::with_window(DEFAULT_WINDOW)
    }

    pub fn with_window(window: Duration) -> Self {
        Self {
            state: Mutex::new(ThroughputState::default()),
            window,
        }
    }

    /// Start consuming one blob's plaintext into coven's durable spool.
    ///
    /// Every public entry point captures its timestamp AFTER acquiring the
    /// state lock. Captured before, two threads can interleave so that an
    /// older timestamp is applied after a newer one reset the measurement,
    /// and the "clock regressed" invariants below fire on a race rather than
    /// a real clock fault — a live panic that poisoned this mutex and took
    /// the whole sync stack down with it. Under the lock, mutex ordering plus
    /// `Instant`'s monotonicity make out-of-order timestamps unrepresentable;
    /// the `_at` variants exist for tests, which own their ordering.
    pub(crate) fn begin_preparation(&self, upload: crate::library::outbox_snapshot::UploadBlobKey) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        Self::begin_preparation_locked(&mut state, upload, now);
    }

    #[cfg(test)]
    pub(crate) fn begin_preparation_at(
        &self,
        upload: crate::library::outbox_snapshot::UploadBlobKey,
        now: Instant,
    ) {
        let mut state = self.state.lock().unwrap();
        Self::begin_preparation_locked(&mut state, upload, now);
    }

    fn begin_preparation_locked(
        state: &mut ThroughputState,
        upload: crate::library::outbox_snapshot::UploadBlobKey,
        now: Instant,
    ) {
        assert!(
            !state.uploads.contains_key(&upload),
            "one blob cannot start preparation twice"
        );
        state.uploads.insert(
            upload,
            ActiveMeasurement {
                phase: TransferPhase::Preparing,
                measurement: MeasurementState {
                    samples: VecDeque::new(),
                    measurement_started: Some(now),
                },
            },
        );
    }

    /// Start the provider phase. A process may resume directly from coven's
    /// durable prepared journal, so this also accepts a blob with no local
    /// preparation measurement.
    pub(crate) fn begin_upload(&self, upload: crate::library::outbox_snapshot::UploadBlobKey) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        Self::begin_upload_locked(&mut state, upload, now);
    }

    #[cfg(test)]
    pub(crate) fn begin_upload_at(
        &self,
        upload: crate::library::outbox_snapshot::UploadBlobKey,
        now: Instant,
    ) {
        let mut state = self.state.lock().unwrap();
        Self::begin_upload_locked(&mut state, upload, now);
    }

    fn begin_upload_locked(
        state: &mut ThroughputState,
        upload: crate::library::outbox_snapshot::UploadBlobKey,
        now: Instant,
    ) {
        if let Some(active) = state.uploads.get(&upload) {
            assert!(
                active.phase == TransferPhase::Preparing,
                "one blob cannot start two provider transfers"
            );
        }
        state.uploads.insert(
            upload,
            ActiveMeasurement {
                phase: TransferPhase::Uploading,
                measurement: MeasurementState {
                    samples: VecDeque::new(),
                    measurement_started: Some(now),
                },
            },
        );
    }

    /// Finish whichever byte-moving phase is active, whether the attempt
    /// completed or failed.
    pub(crate) fn end(&self, upload: &crate::library::outbox_snapshot::UploadBlobKey) {
        let mut state = self.state.lock().unwrap();
        assert!(
            state.uploads.remove(upload).is_some(),
            "transfer ended without a matching blob start"
        );
    }

    /// Record the plaintext-byte delta from one coalesced preparation report.
    pub(crate) fn record_preparation(
        &self,
        upload: &crate::library::outbox_snapshot::UploadBlobKey,
        bytes: u64,
    ) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        Self::record_phase_locked(
            &mut state,
            upload,
            TransferPhase::Preparing,
            bytes,
            now,
            self.window,
        );
    }

    #[cfg(test)]
    pub(crate) fn record_preparation_at(
        &self,
        upload: &crate::library::outbox_snapshot::UploadBlobKey,
        bytes: u64,
        now: Instant,
    ) {
        let mut state = self.state.lock().unwrap();
        Self::record_phase_locked(
            &mut state,
            upload,
            TransferPhase::Preparing,
            bytes,
            now,
            self.window,
        );
    }

    /// Record the provider-byte delta from one coalesced upload report.
    pub(crate) fn record_upload(
        &self,
        upload: &crate::library::outbox_snapshot::UploadBlobKey,
        bytes: u64,
    ) {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        Self::record_phase_locked(
            &mut state,
            upload,
            TransferPhase::Uploading,
            bytes,
            now,
            self.window,
        );
    }

    #[cfg(test)]
    pub(crate) fn record_upload_at(
        &self,
        upload: &crate::library::outbox_snapshot::UploadBlobKey,
        bytes: u64,
        now: Instant,
    ) {
        let mut state = self.state.lock().unwrap();
        Self::record_phase_locked(
            &mut state,
            upload,
            TransferPhase::Uploading,
            bytes,
            now,
            self.window,
        );
    }

    fn record_phase_locked(
        state: &mut ThroughputState,
        upload: &crate::library::outbox_snapshot::UploadBlobKey,
        expected_phase: TransferPhase,
        bytes: u64,
        now: Instant,
        window: Duration,
    ) {
        let active = state
            .uploads
            .get_mut(upload)
            .expect("bytes require a matching blob measurement");
        assert!(
            active.phase == expected_phase,
            "bytes arrived for the wrong transfer phase"
        );
        prune(&mut active.measurement.samples, now, window);
        active.measurement.samples.push_back((now, bytes));
    }

    /// Bytes per second over the rolling window. Zero when no samples have
    /// landed in the window.
    pub fn bytes_per_sec(&self) -> u64 {
        self.rates().aggregate_bps
    }

    pub(crate) fn rates(&self) -> UploadRates {
        let mut state = self.state.lock().unwrap();
        let now = Instant::now();
        Self::rates_locked(&mut state, now, self.window)
    }

    #[cfg(test)]
    fn bytes_per_sec_at(&self, now: Instant) -> u64 {
        self.rates_at(now).aggregate_bps
    }

    #[cfg(test)]
    pub(crate) fn rates_at(&self, now: Instant) -> UploadRates {
        let mut state = self.state.lock().unwrap();
        Self::rates_locked(&mut state, now, self.window)
    }

    fn rates_locked(state: &mut ThroughputState, now: Instant, window: Duration) -> UploadRates {
        let mut aggregate_bps = 0u64;
        let mut provider_bps = 0u64;
        let mut by_upload = HashMap::with_capacity(state.uploads.len());
        for (upload, active) in &mut state.uploads {
            let rate = Self::bytes_per_sec_locked(&mut active.measurement, now, window);
            aggregate_bps = aggregate_bps
                .checked_add(rate)
                .expect("aggregate throughput cannot overflow");
            if active.phase == TransferPhase::Uploading {
                provider_bps = provider_bps
                    .checked_add(rate)
                    .expect("provider throughput cannot overflow");
            }
            by_upload.insert(upload.clone(), rate);
        }
        UploadRates {
            aggregate_bps,
            provider_bps,
            by_upload,
        }
    }

    fn bytes_per_sec_locked(state: &mut MeasurementState, now: Instant, window: Duration) -> u64 {
        prune(&mut state.samples, now, window);
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
            .min(window);
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

    fn upload(id: impl Into<String>) -> crate::library::outbox_snapshot::UploadBlobKey {
        crate::library::outbox_snapshot::UploadBlobKey::new("files", id)
    }

    #[test]
    fn empty_tracker_reports_zero() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        assert_eq!(t.bytes_per_sec(), 0);
    }

    #[test]
    fn samples_in_window_sum_and_divide_by_window() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let now = Instant::now();
        let upload = upload("window");
        t.begin_upload_at(upload.clone(), now - Duration::from_secs(10));
        t.record_upload_at(&upload, 5_000_000, now);
        t.record_upload_at(&upload, 5_000_000, now);
        // 10 MB over 10s = 1 MB/s.
        assert_eq!(t.bytes_per_sec_at(now), 1_000_000);
    }

    #[test]
    fn a_new_transfer_uses_its_elapsed_time_until_the_window_fills() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        let upload = upload("new-transfer");
        t.begin_upload_at(upload.clone(), started);
        let first_tick = started + Duration::from_millis(500);
        t.record_upload_at(&upload, 500_000, first_tick);

        assert_eq!(t.bytes_per_sec_at(first_tick), 1_000_000);
    }

    #[test]
    fn concurrent_upload_rates_remain_attributed_to_their_blob() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let first = upload("first");
        let second = upload("second");
        let started = Instant::now();
        tracker.begin_upload_at(first.clone(), started);
        tracker.begin_upload_at(second.clone(), started);
        let measured = started + Duration::from_secs(10);
        tracker.record_upload_at(&first, 10_000_000, measured);
        tracker.record_upload_at(&second, 20_000_000, measured);

        let rates = tracker.rates_at(measured);
        assert_eq!(rates.for_uploads([&first]), 1_000_000);
        assert_eq!(rates.for_uploads([&second]), 2_000_000);
        assert_eq!(rates.for_uploads([&first, &second]), 3_000_000);
        assert_eq!(rates.aggregate_bps, 3_000_000);
    }

    #[test]
    fn preparation_rate_is_displayed_but_provider_rate_starts_at_upload() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let upload = upload("phase-change");
        let started = Instant::now();
        tracker.begin_preparation_at(upload.clone(), started);
        let prepared = started + Duration::from_secs(10);
        tracker.record_preparation_at(&upload, 10_000_000, prepared);

        let preparation_rates = tracker.rates_at(prepared);
        assert_eq!(preparation_rates.for_uploads([&upload]), 1_000_000);
        assert_eq!(preparation_rates.aggregate_bps, 1_000_000);
        assert_eq!(preparation_rates.provider_bps, 0);

        tracker.begin_upload_at(upload.clone(), prepared);
        assert_eq!(tracker.rates_at(prepared).aggregate_bps, 0);
        let uploading = prepared + Duration::from_secs(5);
        tracker.record_upload_at(&upload, 10_000_000, uploading);

        let upload_rates = tracker.rates_at(uploading);
        assert_eq!(upload_rates.for_uploads([&upload]), 2_000_000);
        assert_eq!(upload_rates.aggregate_bps, 2_000_000);
        assert_eq!(upload_rates.provider_bps, 2_000_000);
    }

    #[test]
    fn samples_older_than_window_are_dropped() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let start = Instant::now();
        let upload = upload("aging");
        t.begin_upload_at(upload.clone(), start);
        t.record_upload_at(&upload, 10_000_000, start);
        let later = start + Duration::from_secs(11);
        // Sample aged out: rate is back to zero.
        assert_eq!(t.bytes_per_sec_at(later), 0);
    }

    #[test]
    fn ending_the_last_transfer_hides_the_rate_and_a_new_batch_starts_fresh() {
        let t = UploadThroughput::with_window(Duration::from_secs(10));
        let first_start = Instant::now();
        let first = upload("first-batch");
        t.begin_upload_at(first.clone(), first_start);
        let first_tick = first_start + Duration::from_secs(1);
        t.record_upload_at(&first, 1_000_000, first_tick);
        assert_eq!(t.bytes_per_sec_at(first_tick), 1_000_000);

        t.end(&first);
        assert_eq!(t.bytes_per_sec_at(first_tick), 0);

        let second_start = first_start + Duration::from_secs(20);
        let second = upload("second-batch");
        t.begin_upload_at(second.clone(), second_start);
        let second_tick = second_start + Duration::from_secs(1);
        t.record_upload_at(&second, 2_000_000, second_tick);
        assert_eq!(t.bytes_per_sec_at(second_tick), 2_000_000);
    }

    #[test]
    #[should_panic(expected = "bytes require a matching blob measurement")]
    fn provider_bytes_require_an_active_transfer() {
        UploadThroughput::with_window(Duration::from_secs(10)).record_upload(&upload("idle"), 1);
    }

    #[test]
    #[should_panic(expected = "throughput byte sample overflow")]
    fn aggregate_samples_cannot_wrap_their_byte_counter() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let now = Instant::now();
        let upload = upload("overflow");
        tracker.begin_upload_at(upload.clone(), now - Duration::from_secs(1));
        tracker.record_upload_at(&upload, u64::MAX, now);
        tracker.record_upload_at(&upload, 1, now);

        tracker.bytes_per_sec_at(now);
    }

    /// Public entry points capture their timestamps under the state lock, so
    /// concurrent begin/record/read/end interleavings can never construct an
    /// out-of-order timestamp pair — the exact race that panicked live and
    /// poisoned this mutex for the rest of the process.
    #[test]
    fn concurrent_use_never_regresses_the_clock() {
        let tracker = std::sync::Arc::new(UploadThroughput::with_window(Duration::from_millis(50)));
        let mut workers = Vec::new();
        for worker_index in 0..4 {
            let tracker = std::sync::Arc::clone(&tracker);
            workers.push(std::thread::spawn(move || {
                for upload_index in 0..5_000 {
                    let upload = upload(format!("{worker_index}-{upload_index}"));
                    tracker.begin_upload(upload.clone());
                    tracker.record_upload(&upload, 1);
                    tracker.bytes_per_sec();
                    tracker.end(&upload);
                }
            }));
        }
        for worker in workers {
            worker.join().expect("no worker panicked");
        }
    }

    #[test]
    #[should_panic(expected = "throughput clock regressed before measurement start")]
    fn measurement_clock_cannot_regress() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        tracker.begin_upload_at(upload("regressed-measurement"), started);

        tracker.bytes_per_sec_at(started - Duration::from_millis(1));
    }

    #[test]
    #[should_panic(expected = "throughput clock regressed before a byte sample")]
    fn sample_clock_cannot_regress() {
        let tracker = UploadThroughput::with_window(Duration::from_secs(10));
        let started = Instant::now();
        let upload = upload("regressed-sample");
        tracker.begin_upload_at(upload.clone(), started);
        tracker.record_upload_at(&upload, 1, started + Duration::from_millis(1));

        tracker.bytes_per_sec_at(started);
    }
}
