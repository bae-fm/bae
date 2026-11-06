//! One track's streamed PCM, via a lock-free ring buffer.
//!
//! Producer/consumer pair for streaming a single track's samples from the
//! decoder thread to the audio callback:
//! - `TrackSink`: producer side; the decoder writes f32 samples here.
//! - `TrackStream`: consumer side; the audio callback (via `PlaybackSource`)
//!   pulls samples from here.
//!
//! Uses `rtrb` for lock-free SPSC communication, safe for real-time audio.

use crate::stream_wake::StreamWake;
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use tokio::sync::oneshot;

/// How much audio the ring holds, whatever the sample rate.
const DEFAULT_BUFFER_MS: u32 = 100;

/// Shared state between sink and stream.
pub struct StreamingState {
    sample_rate: u32,
    channels: u32,
    /// Playback position in frames (samples per channel).
    position_samples: AtomicU64,
    /// The producer reached EOF.
    finished: AtomicBool,
    cancelled: AtomicBool,
    /// FFmpeg frames that failed to decode.
    decode_error_count: AtomicU32,
    /// Samples pushed into the ring so far — proof the decode produced audio.
    samples_decoded: AtomicU64,
    /// Set by the decoder thread on its first push; the audio callback reads it
    /// to unpark a decoder parked on a full ring.
    decoder_thread: OnceLock<std::thread::Thread>,
    /// The wake of the drain that reads this stream, once the stream is in a
    /// source that drain reads. The producer notifies it after each push and
    /// when it finishes. Set under the lock and read under it, so a push either
    /// sees the wake or happened before the stream was placed, where the
    /// drain's first look finds it.
    consumer_wake: Mutex<Option<StreamWake>>,
    /// What the producer has done so far, for a test to wait on.
    #[cfg(any(test, feature = "test-instrumentation"))]
    activity: std::sync::Mutex<ProducerActivity>,
    #[cfg(any(test, feature = "test-instrumentation"))]
    activity_changed: std::sync::Condvar,
}

/// How often the producer pushed samples and how often it found the ring full
/// and parked.
#[cfg(any(test, feature = "test-instrumentation"))]
#[derive(Default)]
struct ProducerActivity {
    pushes: u64,
    parks: u64,
}

impl StreamingState {
    #[cfg(any(test, feature = "test-instrumentation"))]
    fn note(&self, change: impl FnOnce(&mut ProducerActivity)) {
        change(&mut self.activity.lock().unwrap());
        self.activity_changed.notify_all();
    }

    fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            sample_rate,
            channels,
            position_samples: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            decode_error_count: AtomicU32::new(0),
            samples_decoded: AtomicU64::new(0),
            decoder_thread: OnceLock::new(),
            consumer_wake: Mutex::new(None),
            #[cfg(any(test, feature = "test-instrumentation"))]
            activity: std::sync::Mutex::default(),
            #[cfg(any(test, feature = "test-instrumentation"))]
            activity_changed: std::sync::Condvar::new(),
        }
    }

    /// Tell the drain reading this stream, if it is in one's source yet, that
    /// it may have work.
    fn wake_consumer(&self) {
        if let Some(wake) = &*self.consumer_wake.lock().unwrap_or_else(PoisonError::into_inner) {
            wake.notify();
        }
    }

    fn unpark_decoder(&self) {
        if let Some(thread) = self.decoder_thread.get() {
            thread.unpark();
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    pub fn position_samples(&self) -> u64 {
        self.position_samples.load(Ordering::Relaxed)
    }

    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub fn decode_error_count(&self) -> u32 {
        self.decode_error_count.load(Ordering::Relaxed)
    }

    pub fn set_decode_error_count(&self, count: u32) {
        self.decode_error_count.store(count, Ordering::Relaxed);
    }

    pub fn samples_decoded(&self) -> u64 {
        self.samples_decoded.load(Ordering::Relaxed)
    }

    /// Count `n` more interleaved samples as pushed. Live as the decoder
    /// produces them, not set once at decode-complete, so a `Starved` event
    /// fired mid-decode carries a truthful count.
    fn add_samples_decoded(&self, n: u64) {
        self.samples_decoded.fetch_add(n, Ordering::Relaxed);
    }
}

/// Producer side: the decoder pushes decoded f32 samples into the ring.
pub struct TrackSink {
    producer: Producer<f32>,
    state: Arc<StreamingState>,
    /// Fires once the ring is 50% full (or the producer finishes).
    ready_tx: Option<oneshot::Sender<()>>,
    /// Ring capacity, for the 50% ready threshold.
    capacity: usize,
    samples_pushed: usize,
}

impl TrackSink {
    /// Push what fits and return how many samples landed; a full ring pushes
    /// fewer than `samples.len()`.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        if self.state.is_cancelled() {
            return 0;
        }

        let mut pushed = 0;
        for &sample in samples {
            match self.producer.push(sample) {
                Ok(()) => pushed += 1,
                Err(_) => break, // Buffer full
            }
        }
        self.state.add_samples_decoded(pushed as u64);
        if pushed > 0 {
            self.state.wake_consumer();
        }
        pushed
    }

    /// Push every sample, blocking until they all land or the stream is
    /// cancelled. Writes in bulk (memcpy) rather than per-sample. On a full ring
    /// it parks until the consumer drains, cancels or drops the stream, so a
    /// paused player wakes it zero times. Signals ready at 50% capacity.
    pub fn push_samples_blocking(&mut self, samples: &[f32]) -> usize {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the first push records the decoder thread; later pushes come from it"
        )]
        let _ = self.state.decoder_thread.set(std::thread::current());
        // Pairs with the fence in `TrackStream::cancel`: either this thread's
        // cancelled check below sees the cancel, or the canceller sees this
        // thread recorded and unparks it.
        std::sync::atomic::fence(Ordering::SeqCst);

        let mut offset = 0;
        while offset < samples.len() {
            if self.state.is_cancelled() {
                return offset;
            }

            let remaining = &samples[offset..];
            let (pushed, _) = self.producer.push_partial_slice(remaining);
            let n = pushed.len();
            if n == 0 {
                // Full ring: park until the consumer frees space, cancels, or
                // drops. Each of those unparks this thread, and an unpark that
                // lands before the park makes it return at once.
                #[cfg(any(test, feature = "test-instrumentation"))]
                self.state.note(|activity| activity.parks += 1);
                std::thread::park();
                continue;
            }

            #[cfg(any(test, feature = "test-instrumentation"))]
            self.state.note(|activity| activity.pushes += 1);
            offset += n;
            self.samples_pushed += n;
            self.state.add_samples_decoded(n as u64);
            self.state.wake_consumer();

            if self.ready_tx.is_some() && self.samples_pushed >= self.capacity / 2 {
                if let Some(tx) = self.ready_tx.take() {
                    #[expect(
                        clippy::let_underscore_must_use,
                        reason = "a reader that stopped waiting for readiness needs no signal"
                    )]
                    let _ = tx.send(());
                }
            }
        }
        offset
    }

    /// Push generated silent frames before decoded source audio.
    pub fn push_silence_frames_blocking(&mut self, frames: u64) -> u64 {
        let channels = self.state.channels() as usize;
        if channels == 0 {
            return 0;
        }

        let chunk_frames = 4096usize;
        let chunk = vec![0.0; chunk_frames * channels];
        let mut frames_pushed = 0;
        while frames_pushed < frames {
            if self.state.is_cancelled() {
                return frames_pushed;
            }

            let remaining_frames = (frames - frames_pushed) as usize;
            let this_chunk_frames = remaining_frames.min(chunk_frames);
            let samples = this_chunk_frames * channels;
            let pushed = self.push_samples_blocking(&chunk[..samples]);
            frames_pushed += (pushed / channels) as u64;
            if pushed == 0 {
                return frames_pushed;
            }
        }
        frames_pushed
    }

    /// EOF: every sample has been pushed. Also signals ready if it hasn't fired —
    /// a track shorter than half the ring never reaches the 50% threshold.
    pub fn mark_finished(&mut self) {
        if let Some(tx) = self.ready_tx.take() {
            #[expect(
                clippy::let_underscore_must_use,
                reason = "a reader that stopped waiting for readiness needs no signal"
            )]
            let _ = tx.send(());
        }
        self.state.finished.store(true, Ordering::Release);
        self.state.wake_consumer();
    }

    /// Record the decode's FFmpeg error count, once the decode ends.
    pub fn set_decode_error_count(&self, count: u32) {
        self.state.set_decode_error_count(count);
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }
}

/// Consumer side: the audio callback pulls f32 samples out of the ring.
pub struct TrackStream {
    consumer: Consumer<f32>,
    state: Arc<StreamingState>,
}

impl TrackStream {
    /// Pull into `output` and return how many samples landed. Reads in bulk
    /// (memcpy) rather than per-sample, and returns 0 immediately on an empty
    /// ring — never blocks, because this runs on the audio callback.
    pub fn pull_samples(&mut self, output: &mut [f32]) -> usize {
        let (popped, _) = self.consumer.pop_partial_slice(output);
        let pulled = popped.len();
        if pulled == 0 {
            return 0;
        }

        let channels = self.state.channels() as u64;
        if let Some(frames) = (pulled as u64).checked_div(channels) {
            self.state.position_samples.fetch_add(frames, Ordering::Relaxed);
        }

        // Space freed: wake the decoder so it refills.
        self.state.unpark_decoder();

        pulled
    }

    /// The producer finished AND the ring is drained — nothing left to play.
    pub fn is_finished(&self) -> bool {
        self.state.is_finished() && self.consumer.is_empty()
    }

    /// The producer finished, though the ring may still hold samples.
    pub fn producer_finished(&self) -> bool {
        self.state.is_finished()
    }

    /// Have the producer notify `wake` after each push and when it finishes,
    /// for the drain that reads this stream. Called when the stream is placed
    /// in that drain's source, never from the real-time callback.
    pub fn wake_on_push(&self, wake: &StreamWake) {
        *self.state.consumer_wake.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(wake.clone());
    }

    /// Stop the decoder: set the flag and unpark it if it's parked on a full ring.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        // Pairs with the fence after `push_samples_blocking` records its thread.
        std::sync::atomic::fence(Ordering::SeqCst);
        self.state.unpark_decoder();
    }

    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Test-only: how many pushes the producer has made so far. Read it before
    /// pulling, and hand it to [`Self::wait_for_push_after`] when the pull
    /// found nothing.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn pushes(&self) -> u64 {
        self.state.activity.lock().unwrap().pushes
    }

    /// Test-only: wait until the producer has pushed since the count `seen`.
    /// Panics if the hang guard passes first.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn wait_for_push_after(&self, seen: u64) {
        let (_activity, result) = self
            .state
            .activity_changed
            .wait_timeout_while(
                self.state.activity.lock().unwrap(),
                bae_util::test_channels::HANG_GUARD,
                |activity| activity.pushes <= seen,
            )
            .unwrap();
        assert!(!result.timed_out(), "the producer never pushed again");
    }

    /// Test-only: wait until the producer has found the ring full and parked.
    /// Panics if the hang guard passes first.
    #[cfg(any(test, feature = "test-instrumentation"))]
    pub fn wait_until_producer_parked(&self) {
        let (_activity, result) = self
            .state
            .activity_changed
            .wait_timeout_while(
                self.state.activity.lock().unwrap(),
                bae_util::test_channels::HANG_GUARD,
                |activity| activity.parks == 0,
            )
            .unwrap();
        assert!(!result.timed_out(), "the producer never parked on a full ring");
    }

    pub fn decode_error_count(&self) -> u32 {
        self.state.decode_error_count()
    }

    pub fn samples_decoded(&self) -> u64 {
        self.state.samples_decoded()
    }

    pub fn position(&self) -> std::time::Duration {
        bae_util::sample_time::frame_start(self.state.position_samples(), self.state.sample_rate())
    }

    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate()
    }

    pub fn channels(&self) -> u32 {
        self.state.channels()
    }
}

/// Nothing drains the ring once the consumer is gone, so the producer stops.
impl Drop for TrackStream {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Resolves when the ring is 50% full or the producer finishes, whichever comes
/// first.
pub type ReadyReceiver = oneshot::Receiver<()>;

/// A sink/stream pair whose ring holds `DEFAULT_BUFFER_MS` of audio at this
/// sample rate.
pub fn create_track_stream_pair(
    sample_rate: u32,
    channels: u32,
) -> (TrackSink, TrackStream, ReadyReceiver) {
    let capacity_samples =
        (sample_rate as usize * channels as usize * DEFAULT_BUFFER_MS as usize) / 1000;
    create_track_stream_pair_with_capacity(sample_rate, channels, capacity_samples)
}

pub fn create_track_stream_pair_with_capacity(
    sample_rate: u32,
    channels: u32,
    capacity_samples: usize,
) -> (TrackSink, TrackStream, ReadyReceiver) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    let state = Arc::new(StreamingState::new(sample_rate, channels));
    let (ready_tx, ready_rx) = oneshot::channel();

    let sink = TrackSink {
        producer,
        state: state.clone(),
        ready_tx: Some(ready_tx),
        capacity: capacity_samples,
        samples_pushed: 0,
    };

    let source = TrackStream { consumer, state };

    (sink, source, ready_rx)
}

#[cfg(test)]
#[path = "track_stream_tests.rs"]
mod tests;
