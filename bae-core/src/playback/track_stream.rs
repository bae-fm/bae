//! One track's streamed PCM, via a lock-free ring buffer.
//!
//! Producer/consumer pair for streaming a single track's samples from the
//! decoder thread to the audio callback:
//! - `TrackSink`: producer side; the decoder writes f32 samples here.
//! - `TrackStream`: consumer side; the audio callback (via `PlaybackSource`)
//!   pulls samples from here.
//!
//! Uses `rtrb` for lock-free SPSC communication, safe for real-time audio.

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
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
}

impl StreamingState {
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
    #[cfg(test)]
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
        pushed
    }

    /// Push every sample, blocking until they all land or the stream is
    /// cancelled. Writes in bulk (memcpy) rather than per-sample. On a full ring
    /// it parks until the audio callback drains and unparks it, so a paused
    /// player wakes it zero times. Signals ready at 50% capacity.
    pub fn push_samples_blocking(&mut self, samples: &[f32]) -> usize {
        let _ = self.state.decoder_thread.set(std::thread::current());

        let mut offset = 0;
        while offset < samples.len() {
            if self.state.is_cancelled() {
                return offset;
            }

            let remaining = &samples[offset..];
            let (pushed, _) = self.producer.push_partial_slice(remaining);
            let n = pushed.len();
            if n == 0 {
                // Full ring: park until the audio callback drains. The timeout is
                // the safety net for null audio devices (CI), whose callback may
                // never fire to unpark us.
                std::thread::park_timeout(std::time::Duration::from_millis(100));
                continue;
            }

            offset += n;
            self.samples_pushed += n;
            self.state.add_samples_decoded(n as u64);

            if self.ready_tx.is_some() && self.samples_pushed >= self.capacity / 2 {
                if let Some(tx) = self.ready_tx.take() {
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
            let _ = tx.send(());
        }
        self.state.finished.store(true, Ordering::Release);
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
            self.state
                .position_samples
                .fetch_add(frames, Ordering::Relaxed);
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

    /// Stop the decoder: set the flag and unpark it if it's parked on a full ring.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        self.state.unpark_decoder();
    }

    #[cfg(test)]
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    pub fn decode_error_count(&self) -> u32 {
        self.state.decode_error_count()
    }

    pub fn samples_decoded(&self) -> u64 {
        self.state.samples_decoded()
    }

    pub fn position(&self) -> std::time::Duration {
        let samples = self.state.position_samples();
        let sample_rate = self.state.sample_rate() as u64;
        if sample_rate == 0 {
            return std::time::Duration::ZERO;
        }
        std::time::Duration::from_secs_f64(samples as f64 / sample_rate as f64)
    }

    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate()
    }

    pub fn channels(&self) -> u32 {
        self.state.channels()
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
mod tests {
    use super::*;

    #[test]
    fn test_push_pull_samples() {
        let (mut sink, mut source, _ready) = create_track_stream_pair(44100, 2);

        let samples = vec![0.1, 0.2, 0.3, 0.4];
        let pushed = sink.push_samples(&samples);
        assert_eq!(pushed, 4);

        let mut output = vec![0.0; 4];
        let pulled = source.pull_samples(&mut output);
        assert_eq!(pulled, 4);
        assert_eq!(output, samples);
    }

    #[test]
    fn test_finished_flag() {
        let (mut sink, source, _ready) = create_track_stream_pair(44100, 2);

        assert!(!source.is_finished());
        assert!(!source.producer_finished());

        sink.mark_finished();

        assert!(source.producer_finished());
        assert!(source.is_finished()); // Empty buffer + finished = done
    }

    #[test]
    fn test_position_tracking() {
        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(44100, 2, 10000);

        // 1000 stereo samples = 500 frames.
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 * 0.001).collect();
        sink.push_samples(&samples);

        let mut output = vec![0.0; 1000];
        source.pull_samples(&mut output);

        // Position counts frames, not samples.
        assert_eq!(source.state.position_samples(), 500);

        // 500 / 44100 ≈ 11.3ms
        let pos = source.position();
        assert!(pos.as_millis() >= 11 && pos.as_millis() <= 12);
    }

    #[test]
    fn test_cancel() {
        let (sink, source, _ready) = create_track_stream_pair(44100, 2);

        assert!(!sink.is_cancelled());
        assert!(!source.is_cancelled());

        source.cancel();

        assert!(sink.is_cancelled());
        assert!(source.is_cancelled());
    }

    #[test]
    fn test_buffer_full() {
        let (mut sink, _source, _ready) = create_track_stream_pair_with_capacity(44100, 2, 10);

        // Pushing past capacity pushes only what fits.
        let samples = vec![0.5; 20];
        let pushed = sink.push_samples(&samples);

        assert!(pushed <= 10);
    }

    #[test]
    fn test_buffer_empty() {
        let (_sink, mut source, _ready) = create_track_stream_pair(44100, 2);

        let mut output = vec![0.0; 10];
        let pulled = source.pull_samples(&mut output);

        assert_eq!(pulled, 0);
    }

    #[tokio::test]
    async fn test_ready_signal_fires_at_threshold() {
        use std::thread;
        use std::time::Duration;

        let (mut sink, _source, ready_rx) = create_track_stream_pair_with_capacity(44100, 2, 100);

        thread::spawn(move || {
            // Exactly the 50% threshold.
            let samples: Vec<f32> = (0..50).map(|i| i as f32 * 0.01).collect();
            sink.push_samples_blocking(&samples);
        });

        let result = tokio::time::timeout(Duration::from_millis(100), ready_rx).await;
        assert!(result.is_ok(), "Ready signal should fire at 50%");
        assert!(result.unwrap().is_ok(), "Oneshot should succeed");
    }

    #[tokio::test]
    async fn test_ready_signal_on_finish() {
        use std::thread;
        use std::time::Duration;

        // Capacity 1000, so the 50% threshold is 500.
        let (mut sink, _source, ready_rx) = create_track_stream_pair_with_capacity(44100, 2, 1000);

        thread::spawn(move || {
            // 100 samples never reaches the threshold; mark_finished fires ready.
            let samples: Vec<f32> = (0..100).map(|i| i as f32 * 0.001).collect();
            sink.push_samples_blocking(&samples);
            sink.mark_finished();
        });

        let result = tokio::time::timeout(Duration::from_millis(100), ready_rx).await;
        assert!(result.is_ok(), "Ready signal should fire on finish");
        assert!(result.unwrap().is_ok(), "Oneshot should succeed");
    }

    /// The production producer pushes more samples than the ring holds, so it
    /// parks when the ring fills and resumes as a draining consumer frees space.
    /// Every sample is delivered in order across the park boundary.
    #[test]
    fn push_samples_blocking_parks_until_consumer_drains() {
        use std::thread;
        use std::time::Duration;

        // Ring capacity (8) is far below the total pushed (40), so the producer
        // must park on full at least once.
        let (mut sink, mut source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 8);
        let total = 40usize;
        let samples: Vec<f32> = (0..total).map(|i| i as f32).collect();
        let expected = samples.clone();

        let consumer = thread::spawn(move || {
            let mut got: Vec<f32> = Vec::new();
            while got.len() < total {
                let mut buf = [0.0f32; 4];
                let n = source.pull_samples(&mut buf);
                if n == 0 {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                got.extend_from_slice(&buf[..n]);
            }
            got
        });

        let pushed = sink.push_samples_blocking(&samples);
        assert_eq!(pushed, total, "every sample is eventually pushed");
        assert_eq!(
            consumer.join().unwrap(),
            expected,
            "samples arrive in order across the park boundary"
        );
    }

    /// A blocked producer (ring full, no consumer draining) returns from
    /// `push_samples_blocking` when the stream is cancelled mid-push, having
    /// pushed fewer than all its samples.
    #[test]
    fn push_samples_blocking_returns_on_cancel_mid_push() {
        use std::thread;
        use std::time::Duration;

        let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 8);
        let samples = vec![1.0f32; 100];

        // Nobody drains, so the producer fills the ring and parks; cancel wakes it.
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            source.cancel();
        });

        let pushed = sink.push_samples_blocking(&samples);
        canceller.join().unwrap();

        assert!(sink.is_cancelled(), "the stream is cancelled");
        assert!(
            pushed < samples.len(),
            "cancel interrupts the blocked push before all samples land"
        );
    }

    /// Silence frames are pushed as zeroed samples, `channels` per frame.
    #[test]
    fn push_silence_frames_blocking_pushes_zeroed_frames() {
        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(44100, 2, 10000);

        let pushed = sink.push_silence_frames_blocking(100);
        assert_eq!(pushed, 100, "100 frames pushed");

        let mut out = vec![0.5f32; 200]; // 100 frames * 2 channels
        let n = source.pull_samples(&mut out);
        assert_eq!(n, 200);
        assert!(
            out[..200].iter().all(|&s| s == 0.0),
            "silence frames are zeroed"
        );
    }

    /// A stream with zero channels can't form frames, so silence-frame pushing is
    /// a no-op rather than dividing by zero.
    #[test]
    fn push_silence_frames_blocking_zero_channels_is_noop() {
        let (mut sink, _source, _ready) = create_track_stream_pair_with_capacity(44100, 0, 100);
        assert_eq!(sink.push_silence_frames_blocking(50), 0);
    }
}
