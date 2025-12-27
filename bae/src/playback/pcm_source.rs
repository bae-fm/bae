//! PCM audio source for playback.
//!
//! Holds decoded PCM samples and provides streaming access for cpal output.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Decoded PCM audio ready for playback
pub struct PcmSource {
    /// Interleaved samples (i32, will be converted to f32 during playback)
    samples: Arc<Vec<i32>>,
    /// Current position in samples (per channel, not total)
    position: Arc<AtomicUsize>,
    /// Number of channels
    channels: u32,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Bits per sample (for conversion to f32)
    bits_per_sample: u32,
    /// Total frames decoded (for position tracking)
    decoded_frames: Arc<AtomicU64>,
}

impl PcmSource {
    /// Create a new PCM source from decoded audio
    pub fn new(samples: Vec<i32>, sample_rate: u32, channels: u32, bits_per_sample: u32) -> Self {
        Self {
            samples: Arc::new(samples),
            position: Arc::new(AtomicUsize::new(0)),
            channels,
            sample_rate,
            bits_per_sample,
            decoded_frames: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get the next batch of samples as f32
    /// Returns None when end of audio is reached
    pub fn next_samples(&self, count: usize) -> Option<Vec<f32>> {
        let pos = self.position.load(Ordering::Relaxed);
        if pos >= self.samples.len() {
            return None;
        }

        let end = (pos + count).min(self.samples.len());
        let batch = &self.samples[pos..end];

        // Convert i32 to f32 based on bits_per_sample
        let scale = match self.bits_per_sample {
            16 => 32768.0,
            24 => 8388608.0,
            32 => 2147483648.0,
            _ => 32768.0, // Default to 16-bit scaling
        };

        let f32_samples: Vec<f32> = batch.iter().map(|&s| s as f32 / scale).collect();

        self.position.store(end, Ordering::Relaxed);

        // Update decoded frames
        let frames = (end - pos) / self.channels as usize;
        self.decoded_frames
            .fetch_add(frames as u64, Ordering::Relaxed);

        Some(f32_samples)
    }

    /// Get current playback position
    pub fn position(&self) -> std::time::Duration {
        let frames = self.decoded_frames.load(Ordering::Relaxed);
        let seconds = frames as f64 / self.sample_rate as f64;
        std::time::Duration::from_secs_f64(seconds)
    }

    /// Seek to a specific position
    pub fn seek(&self, position: std::time::Duration) {
        let target_frame = (position.as_secs_f64() * self.sample_rate as f64) as usize;
        let target_sample = target_frame * self.channels as usize;
        let clamped = target_sample.min(self.samples.len());
        self.position.store(clamped, Ordering::Relaxed);
        self.decoded_frames
            .store(target_frame as u64, Ordering::Relaxed);
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels
    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// Get total duration
    pub fn duration(&self) -> std::time::Duration {
        let total_frames = self.samples.len() / self.channels as usize;
        let seconds = total_frames as f64 / self.sample_rate as f64;
        std::time::Duration::from_secs_f64(seconds)
    }

    /// Get bits per sample
    pub fn bits_per_sample(&self) -> u32 {
        self.bits_per_sample
    }

    /// Get raw samples (for export/re-encoding)
    pub fn raw_samples(&self) -> &[i32] {
        &self.samples
    }
}
