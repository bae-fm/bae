//! An audio file's format, as a scan reads it and the library stores it.

/// The parts the UI composes into a one-line label ("FLAC · 44.1 kHz · 16-bit ·
/// stereo"): the codec is a proper noun, the channel count maps to a localized word,
/// and the numbers format per locale. A present `bits_per_sample` means lossless —
/// show the bit depth; absent means lossy — show `bitrate_kbps`, an average,
/// instead.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}
