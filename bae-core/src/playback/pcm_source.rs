//! PCM audio source for export.
//!
//! Holds decoded PCM samples for re-encoding and export.

use std::sync::Arc;

/// Decoded PCM audio for export
pub struct PcmSource {
    /// Interleaved samples (i32)
    samples: Arc<Vec<i32>>,
    /// Number of channels
    channels: u32,
    /// Sample rate in Hz
    sample_rate: u32,
    /// Bits per sample
    bits_per_sample: u32,
}

impl PcmSource {
    /// Create a new PCM source from decoded audio
    pub fn new(samples: Vec<i32>, sample_rate: u32, channels: u32, bits_per_sample: u32) -> Self {
        Self {
            samples: Arc::new(samples),
            channels,
            sample_rate,
            bits_per_sample,
        }
    }

    /// Get the sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get the number of channels
    pub fn channels(&self) -> u32 {
        self.channels
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
