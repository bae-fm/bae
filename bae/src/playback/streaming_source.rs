//! Streaming PCM source using a lock-free ring buffer.
//!
//! Separates the producer (decoder thread) from the consumer (cpal audio callback)
//! using a single-producer single-consumer ring buffer. This enables:
//! - Starting playback before entire track is decoded
//! - Bounded memory usage regardless of track length
//! - No blocking in the real-time audio thread

use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Default buffer size: ~200ms at 44.1kHz stereo
/// 44100 * 2 channels * 0.2 seconds = 17640 samples
const DEFAULT_BUFFER_SAMPLES: usize = 44100 * 2 / 5; // ~200ms

/// Shared state between producer and consumer
pub struct StreamingState {
    /// True when decoder has finished and no more samples will be produced
    pub finished: AtomicBool,
    /// True when consumer is starving (buffer empty but not finished)
    pub starving: AtomicBool,
    /// Total frames consumed (for position tracking)
    pub frames_consumed: AtomicU64,
    /// Sample rate for position calculation
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u32,
}

impl StreamingState {
    fn new(sample_rate: u32, channels: u32) -> Self {
        Self {
            finished: AtomicBool::new(false),
            starving: AtomicBool::new(false),
            frames_consumed: AtomicU64::new(0),
            sample_rate,
            channels,
        }
    }

    /// Get current playback position based on consumed frames
    pub fn position(&self) -> Duration {
        let frames = self.frames_consumed.load(Ordering::Relaxed);
        Duration::from_secs_f64(frames as f64 / self.sample_rate as f64)
    }
}

/// Producer side - owned by decoder thread
pub struct StreamingPcmSink {
    producer: Producer<f32>,
    state: Arc<StreamingState>,
}

#[allow(dead_code)] // Methods will be used when PlaybackService streaming is wired up
impl StreamingPcmSink {
    /// Push samples to the ring buffer.
    /// Blocks if buffer is full, waiting for consumer to drain.
    /// Returns number of samples written.
    pub fn push_samples(&mut self, samples: &[f32]) -> usize {
        let mut written = 0;
        for &sample in samples {
            // Spin-wait if buffer is full
            loop {
                match self.producer.push(sample) {
                    Ok(()) => {
                        written += 1;
                        break;
                    }
                    Err(_) => {
                        // Buffer full, yield and retry
                        std::thread::yield_now();
                    }
                }
            }
        }
        written
    }

    /// Push samples without blocking. Returns number actually written.
    pub fn try_push_samples(&mut self, samples: &[f32]) -> usize {
        let mut written = 0;
        for &sample in samples {
            match self.producer.push(sample) {
                Ok(()) => written += 1,
                Err(_) => break, // Buffer full
            }
        }
        written
    }

    /// Check if buffer has space for more samples
    pub fn has_space(&self) -> bool {
        !self.producer.is_full()
    }

    /// Get number of slots available in buffer
    pub fn available_slots(&self) -> usize {
        self.producer.slots()
    }

    /// Signal that decoding is complete - no more samples will be pushed
    pub fn mark_finished(&self) {
        self.state.finished.store(true, Ordering::Release);
    }

    /// Get shared state reference
    pub fn state(&self) -> &Arc<StreamingState> {
        &self.state
    }
}

/// Consumer side - owned by cpal audio callback
pub struct StreamingPcmSource {
    consumer: Consumer<f32>,
    state: Arc<StreamingState>,
}

#[allow(dead_code)] // Methods will be used when PlaybackService streaming is wired up
impl StreamingPcmSource {
    /// Pull samples from the ring buffer into the output slice.
    /// Returns the number of samples actually read.
    /// If buffer is empty and not finished, marks as starving.
    pub fn pull_samples(&mut self, output: &mut [f32]) -> usize {
        let mut read = 0;

        for sample in output.iter_mut() {
            match self.consumer.pop() {
                Ok(s) => {
                    *sample = s;
                    read += 1;
                }
                Err(_) => {
                    // Buffer empty
                    break;
                }
            }
        }

        // Update position tracking
        if read > 0 {
            let frames = read / self.state.channels as usize;
            self.state
                .frames_consumed
                .fetch_add(frames as u64, Ordering::Relaxed);
            self.state.starving.store(false, Ordering::Relaxed);
        } else if !self.state.finished.load(Ordering::Acquire) {
            // Empty but not finished = starving
            self.state.starving.store(true, Ordering::Relaxed);
        }

        read
    }

    /// Check if playback has finished (buffer empty and decoder done)
    pub fn is_finished(&self) -> bool {
        self.consumer.is_empty() && self.state.finished.load(Ordering::Acquire)
    }

    /// Check if buffer is starving (empty but decoder not done)
    pub fn is_starving(&self) -> bool {
        self.state.starving.load(Ordering::Relaxed)
    }

    /// Get current playback position
    pub fn position(&self) -> Duration {
        self.state.position()
    }

    /// Get shared state reference
    pub fn state(&self) -> &Arc<StreamingState> {
        &self.state
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate
    }

    /// Get number of channels
    pub fn channels(&self) -> u32 {
        self.state.channels
    }

    /// Get number of samples available in buffer
    pub fn available_samples(&self) -> usize {
        self.consumer.slots()
    }
}

/// Create a streaming source/sink pair.
///
/// Returns (sink, source) where:
/// - sink: owned by decoder thread, pushes samples
/// - source: owned by cpal callback, pulls samples
pub fn create_streaming_pair(
    sample_rate: u32,
    channels: u32,
) -> (StreamingPcmSink, StreamingPcmSource) {
    create_streaming_pair_with_capacity(sample_rate, channels, DEFAULT_BUFFER_SAMPLES)
}

/// Create a streaming pair with custom buffer capacity.
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

    let source = StreamingPcmSource { consumer, state };

    (sink, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_push_pull() {
        let (mut sink, mut source) = create_streaming_pair(44100, 2);

        // Push some samples
        let samples = vec![0.5f32; 100];
        let written = sink.push_samples(&samples);
        assert_eq!(written, 100);

        // Pull them back
        let mut output = vec![0.0f32; 100];
        let read = source.pull_samples(&mut output);
        assert_eq!(read, 100);
        assert!(output.iter().all(|&s| s == 0.5));
    }

    #[test]
    fn test_empty_buffer_not_finished() {
        let (sink, mut source) = create_streaming_pair(44100, 2);

        // Buffer is empty but not finished
        let mut output = vec![0.0f32; 10];
        let read = source.pull_samples(&mut output);
        assert_eq!(read, 0);
        assert!(source.is_starving());
        assert!(!source.is_finished());

        drop(sink); // sink not marked finished, just dropped
    }

    #[test]
    fn test_finished_state() {
        let (mut sink, mut source) = create_streaming_pair(44100, 2);

        // Push some samples
        sink.push_samples(&[0.5f32; 10]);
        sink.mark_finished();

        // Drain the buffer
        let mut output = vec![0.0f32; 10];
        source.pull_samples(&mut output);

        // Now should be finished
        assert!(source.is_finished());
        assert!(!source.is_starving());
    }

    #[test]
    fn test_position_tracking() {
        // Use a buffer large enough to hold all samples (no blocking)
        let (mut sink, mut source) = create_streaming_pair_with_capacity(44100, 2, 50000);

        // Push 44100 samples (stereo = 22050 frames = 0.5 seconds)
        let samples = vec![0.0f32; 44100];
        sink.push_samples(&samples);

        // Pull all samples
        let mut output = vec![0.0f32; 44100];
        source.pull_samples(&mut output);

        // Position should be ~0.5 seconds
        let pos = source.position();
        assert!((pos.as_secs_f64() - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_try_push_no_block() {
        let (mut sink, _source) = create_streaming_pair_with_capacity(44100, 2, 100);

        // Fill the buffer
        let samples = vec![0.5f32; 100];
        let written = sink.try_push_samples(&samples);
        assert_eq!(written, 100);

        // Try to push more - should not block, just return 0
        let written = sink.try_push_samples(&[0.5f32; 10]);
        assert_eq!(written, 0);
    }
}
