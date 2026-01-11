//! Streaming PCM source/sink using a lock-free ring buffer.
//!
//! This module provides a producer/consumer pair for streaming audio samples:
//! - `StreamingPcmSink`: Producer side, receives decoded f32 samples from decoder
//! - `StreamingPcmSource`: Consumer side, feeds samples to cpal audio callback
//!
//! Uses `rtrb` for lock-free SPSC communication, safe for real-time audio.

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Default ring buffer duration in milliseconds.
/// Buffer holds this much audio regardless of sample rate.
const DEFAULT_BUFFER_MS: u32 = 100;

/// Shared state between sink and source
pub struct StreamingState {
    /// Audio sample rate
    sample_rate: AtomicU32,
    /// Number of channels
    channels: AtomicU32,
    /// Current playback position in samples (per channel)
    position_samples: AtomicU64,
    /// Whether the producer has finished (EOF reached)
    finished: AtomicBool,
    /// Whether playback was cancelled
    cancelled: AtomicBool,
    /// Count of FFmpeg decode errors (frames that failed to decode)
    decode_error_count: AtomicU32,
    /// Total samples decoded (for verifying decode actually produced audio)
    samples_decoded: AtomicU64,
}

impl StreamingState {
    fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            sample_rate: AtomicU32::new(sample_rate),
            channels: AtomicU32::new(channels),
            position_samples: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            decode_error_count: AtomicU32::new(0),
            samples_decoded: AtomicU64::new(0),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn channels(&self) -> u32 {
        self.channels.load(Ordering::Relaxed)
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

    pub fn set_samples_decoded(&self, count: u64) {
        self.samples_decoded.store(count, Ordering::Relaxed);
    }
}

/// Producer side of the streaming audio pipeline.
///
/// Receives decoded f32 samples and pushes them to the ring buffer.
pub struct StreamingPcmSink {
    producer: Producer<f32>,
    state: Arc<StreamingState>,
    /// One-shot sender to notify when buffer is ready for playback (50% full)
    ready_tx: Option<oneshot::Sender<()>>,
    /// Buffer capacity (needed to calculate 50% threshold)
    capacity: usize,
    /// Samples pushed so far (to know when we hit threshold)
    samples_pushed: usize,
}

impl StreamingPcmSink {
    /// Push samples to the ring buffer.
    ///
    /// Returns the number of samples actually pushed. If the buffer is full,
    /// this will push as many as possible and return early.
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
        pushed
    }

    /// Push samples, blocking until all are pushed or cancelled.
    ///
    /// The decoder produces samples much faster than realtime, so when the ~100ms
    /// ring buffer fills up, we sleep to avoid busy-waiting. The audio callback
    /// drains the buffer every ~10-20ms, so 1ms sleeps give responsive backpressure
    /// without spinning the CPU. Using yield_now() here would cause 100%+ CPU usage.
    ///
    /// Signals readiness (via oneshot) when buffer reaches 50% capacity.
    pub fn push_samples_blocking(&mut self, samples: &[f32]) -> usize {
        let mut pushed = 0;
        for &sample in samples {
            loop {
                if self.state.is_cancelled() {
                    return pushed;
                }
                match self.producer.push(sample) {
                    Ok(()) => {
                        pushed += 1;
                        self.samples_pushed += 1;

                        // Signal ready when buffer is 50% full
                        if self.ready_tx.is_some() && self.samples_pushed >= self.capacity / 2 {
                            if let Some(tx) = self.ready_tx.take() {
                                let _ = tx.send(()); // Ignore error if receiver dropped
                            }
                        }
                        break;
                    }
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                }
            }
        }
        pushed
    }

    /// Signal that all samples have been pushed (EOF).
    /// Also signals ready if we haven't already (for short files).
    pub fn mark_finished(&mut self) {
        // Signal ready if we haven't already (file might be shorter than 50% buffer)
        if let Some(tx) = self.ready_tx.take() {
            let _ = tx.send(());
        }
        self.state.finished.store(true, Ordering::Release);
    }

    /// Set the decode error count (called at end of decode with FFmpeg error count)
    pub fn set_decode_error_count(&self, count: u32) {
        self.state.set_decode_error_count(count);
    }

    /// Set the total samples decoded (called at end of decode)
    pub fn set_samples_decoded(&self, count: u64) {
        self.state.set_samples_decoded(count);
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }
}

/// Consumer side of the streaming audio pipeline.
///
/// Pulls f32 samples from the ring buffer to feed to cpal.
pub struct StreamingPcmSource {
    consumer: Consumer<f32>,
    state: Arc<StreamingState>,
}

impl StreamingPcmSource {
    /// Pull samples from the ring buffer into the output slice.
    ///
    /// Returns the number of samples actually pulled. If the buffer is empty,
    /// returns 0 immediately (non-blocking).
    pub fn pull_samples(&mut self, output: &mut [f32]) -> usize {
        let mut pulled = 0;
        for slot in output.iter_mut() {
            match self.consumer.pop() {
                Ok(sample) => {
                    *slot = sample;
                    pulled += 1;
                }
                Err(_) => break, // Buffer empty
            }
        }

        // Update position based on samples pulled
        if pulled > 0 {
            let channels = self.state.channels() as u64;
            if channels > 0 {
                let frames = pulled as u64 / channels;
                self.state
                    .position_samples
                    .fetch_add(frames, Ordering::Relaxed);
            }
        }

        pulled
    }

    /// Check if the producer has finished and buffer is empty.
    pub fn is_finished(&self) -> bool {
        self.state.is_finished() && self.consumer.is_empty()
    }

    /// Check if the producer signaled finished (may still have buffered data).
    #[cfg(test)]
    pub fn producer_finished(&self) -> bool {
        self.state.is_finished()
    }

    /// Cancel playback.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
    }

    /// Check if cancelled.
    #[cfg(test)]
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Get the count of FFmpeg decode errors that occurred
    pub fn decode_error_count(&self) -> u32 {
        self.state.decode_error_count()
    }

    /// Get the total samples decoded
    pub fn samples_decoded(&self) -> u64 {
        self.state.samples_decoded()
    }

    /// Get current playback position as Duration.
    pub fn position(&self) -> std::time::Duration {
        let samples = self.state.position_samples();
        let sample_rate = self.state.sample_rate() as u64;
        if sample_rate == 0 {
            return std::time::Duration::ZERO;
        }
        std::time::Duration::from_secs_f64(samples as f64 / sample_rate as f64)
    }

    /// Get sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate()
    }

    /// Get number of channels.
    pub fn channels(&self) -> u32 {
        self.state.channels()
    }
}

/// Receiver for buffer readiness notification.
/// Resolves when buffer is 50% full or producer finishes (whichever comes first).
pub type ReadyReceiver = oneshot::Receiver<()>;

/// Create a streaming source/sink pair with capacity based on sample rate.
/// Buffer holds DEFAULT_BUFFER_MS milliseconds of audio regardless of sample rate.
/// Returns a ready receiver that resolves when buffer is 50% full.
pub fn create_streaming_pair(
    sample_rate: u32,
    channels: u32,
) -> (StreamingPcmSink, StreamingPcmSource, ReadyReceiver) {
    // Calculate capacity for DEFAULT_BUFFER_MS milliseconds of audio
    let capacity_samples =
        (sample_rate as usize * channels as usize * DEFAULT_BUFFER_MS as usize) / 1000;
    create_streaming_pair_with_capacity(sample_rate, channels, capacity_samples)
}

/// Create a streaming source/sink pair with specified capacity.
/// Returns a ready receiver that resolves when buffer is 50% full.
pub fn create_streaming_pair_with_capacity(
    sample_rate: u32,
    channels: u32,
    capacity_samples: usize,
) -> (StreamingPcmSink, StreamingPcmSource, ReadyReceiver) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    let state = Arc::new(StreamingState::new(sample_rate, channels));
    let (ready_tx, ready_rx) = oneshot::channel();

    let sink = StreamingPcmSink {
        producer,
        state: state.clone(),
        ready_tx: Some(ready_tx),
        capacity: capacity_samples,
        samples_pushed: 0,
    };

    let source = StreamingPcmSource { consumer, state };

    (sink, source, ready_rx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pull_samples() {
        let (mut sink, mut source, _ready) = create_streaming_pair(44100, 2);

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
        let (mut sink, source, _ready) = create_streaming_pair(44100, 2);

        assert!(!source.is_finished());
        assert!(!source.producer_finished());

        sink.mark_finished();

        assert!(source.producer_finished());
        assert!(source.is_finished()); // Empty buffer + finished = done
    }

    #[test]
    fn test_position_tracking() {
        let (mut sink, mut source, _ready) = create_streaming_pair_with_capacity(44100, 2, 10000);

        // Push 1000 stereo samples (500 frames)
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 * 0.001).collect();
        sink.push_samples(&samples);

        // Pull them
        let mut output = vec![0.0; 1000];
        source.pull_samples(&mut output);

        // Position should be 500 samples (frames)
        assert_eq!(source.state.position_samples(), 500);

        // Duration = 500 / 44100 ≈ 11.3ms
        let pos = source.position();
        assert!(pos.as_millis() >= 11 && pos.as_millis() <= 12);
    }

    #[test]
    fn test_cancel() {
        let (sink, source, _ready) = create_streaming_pair(44100, 2);

        assert!(!sink.is_cancelled());
        assert!(!source.is_cancelled());

        source.cancel();

        assert!(sink.is_cancelled());
        assert!(source.is_cancelled());
    }

    #[test]
    fn test_buffer_full() {
        let (mut sink, _source, _ready) = create_streaming_pair_with_capacity(44100, 2, 10);

        // Try to push more than capacity
        let samples = vec![0.5; 20];
        let pushed = sink.push_samples(&samples);

        // Should only push up to capacity
        assert!(pushed <= 10);
    }

    #[test]
    fn test_buffer_empty() {
        let (_sink, mut source, _ready) = create_streaming_pair(44100, 2);

        let mut output = vec![0.0; 10];
        let pulled = source.pull_samples(&mut output);

        assert_eq!(pulled, 0);
    }

    #[tokio::test]
    async fn test_ready_signal_fires_at_threshold() {
        use std::thread;
        use std::time::Duration;

        let (mut sink, _source, ready_rx) = create_streaming_pair_with_capacity(44100, 2, 100);

        // Spawn thread to push samples
        thread::spawn(move || {
            // Push samples up to 50% threshold
            let samples: Vec<f32> = (0..50).map(|i| i as f32 * 0.01).collect();
            sink.push_samples_blocking(&samples);
        });

        // Should receive ready signal within reasonable time
        let result = tokio::time::timeout(Duration::from_millis(100), ready_rx).await;
        assert!(result.is_ok(), "Ready signal should fire at 50%");
        assert!(result.unwrap().is_ok(), "Oneshot should succeed");
    }

    #[tokio::test]
    async fn test_ready_signal_on_finish() {
        use std::thread;
        use std::time::Duration;

        // Create buffer with capacity 1000 (50% = 500)
        let (mut sink, _source, ready_rx) = create_streaming_pair_with_capacity(44100, 2, 1000);

        thread::spawn(move || {
            // Push only 100 samples (< 50%), then finish
            let samples: Vec<f32> = (0..100).map(|i| i as f32 * 0.001).collect();
            sink.push_samples_blocking(&samples);
            sink.mark_finished();
        });

        // Should still receive ready signal (because mark_finished sends it)
        let result = tokio::time::timeout(Duration::from_millis(100), ready_rx).await;
        assert!(result.is_ok(), "Ready signal should fire on finish");
        assert!(result.unwrap().is_ok(), "Oneshot should succeed");
    }
}
