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

/// Default ring buffer capacity: ~100ms at 44.1kHz stereo
const DEFAULT_CAPACITY_SAMPLES: usize = 44100 * 2 / 10;

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
}

impl StreamingState {
    fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            sample_rate: AtomicU32::new(sample_rate),
            channels: AtomicU32::new(channels),
            position_samples: AtomicU64::new(0),
            finished: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
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
}

/// Producer side of the streaming audio pipeline.
///
/// Receives decoded f32 samples and pushes them to the ring buffer.
pub struct StreamingPcmSink {
    producer: Producer<f32>,
    state: Arc<StreamingState>,
}

impl StreamingPcmSink {
    /// Push samples to the ring buffer.
    ///
    /// Returns the number of samples actually pushed. If the buffer is full,
    /// this will push as many as possible and return early.
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
    /// Uses spin-wait with yield for backpressure.
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
                        break;
                    }
                    Err(_) => {
                        std::thread::yield_now();
                    }
                }
            }
        }
        pushed
    }

    /// Signal that all samples have been pushed (EOF).
    pub fn mark_finished(&self) {
        self.state.finished.store(true, Ordering::Release);
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
    }

    /// Get number of slots available in the buffer.
    pub fn available(&self) -> usize {
        self.producer.slots()
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
    pub fn producer_finished(&self) -> bool {
        self.state.is_finished()
    }

    /// Cancel playback.
    pub fn cancel(&self) {
        self.state.cancelled.store(true, Ordering::Release);
    }

    /// Check if cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.state.is_cancelled()
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

    /// Get number of samples available in buffer.
    pub fn available(&self) -> usize {
        self.consumer.slots()
    }

    /// Reset position counter (for seek operations).
    pub fn reset_position(&self, position_samples: u64) {
        self.state
            .position_samples
            .store(position_samples, Ordering::Relaxed);
    }
}

/// Create a streaming source/sink pair with default capacity.
pub fn create_streaming_pair(sample_rate: u32, channels: u32) -> (StreamingPcmSink, StreamingPcmSource) {
    create_streaming_pair_with_capacity(sample_rate, channels, DEFAULT_CAPACITY_SAMPLES)
}

/// Create a streaming source/sink pair with specified capacity.
pub fn create_streaming_pair_with_capacity(
    sample_rate: u32,
    channels: u32,
    capacity_samples: usize,
) -> (StreamingPcmSink, StreamingPcmSource) {
    let (producer, consumer) = RingBuffer::new(capacity_samples);
    let state = Arc::new(StreamingState::new(sample_rate, channels));

    let sink = StreamingPcmSink {
        producer,
        state: state.clone(),
    };

    let source = StreamingPcmSource {
        consumer,
        state,
    };

    (sink, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pull_samples() {
        let (mut sink, mut source) = create_streaming_pair(44100, 2);

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
        let (sink, source) = create_streaming_pair(44100, 2);

        assert!(!source.is_finished());
        assert!(!source.producer_finished());

        sink.mark_finished();

        assert!(source.producer_finished());
        assert!(source.is_finished()); // Empty buffer + finished = done
    }

    #[test]
    fn test_position_tracking() {
        let (mut sink, mut source) = create_streaming_pair_with_capacity(44100, 2, 10000);

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
        let (sink, source) = create_streaming_pair(44100, 2);

        assert!(!sink.is_cancelled());
        assert!(!source.is_cancelled());

        source.cancel();

        assert!(sink.is_cancelled());
        assert!(source.is_cancelled());
    }

    #[test]
    fn test_buffer_full() {
        let (mut sink, _source) = create_streaming_pair_with_capacity(44100, 2, 10);

        // Try to push more than capacity
        let samples = vec![0.5; 20];
        let pushed = sink.push_samples(&samples);

        // Should only push up to capacity
        assert!(pushed <= 10);
    }

    #[test]
    fn test_buffer_empty() {
        let (_sink, mut source) = create_streaming_pair(44100, 2);

        let mut output = vec![0.0; 10];
        let pulled = source.pull_samples(&mut output);

        assert_eq!(pulled, 0);
    }
}
