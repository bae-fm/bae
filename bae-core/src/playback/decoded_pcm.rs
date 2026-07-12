//! Decoded PCM audio for re-encoding and export.

/// A fully-decoded PCM buffer. Held in memory; consumers (export, re-encode)
/// read the samples directly. For sample-by-sample streaming during playback,
/// use `TrackStream` instead.
pub struct DecodedPcm {
    /// Interleaved full-range i32 samples.
    samples: Vec<i32>,
    channels: u32,
    /// Hz.
    sample_rate: u32,
}

impl DecodedPcm {
    pub fn new(samples: Vec<i32>, sample_rate: u32, channels: u32) -> Self {
        Self {
            samples,
            channels,
            sample_rate,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// The full-range interleaved samples, for export / re-encoding.
    pub fn raw_samples(&self) -> &[i32] {
        &self.samples
    }
}
