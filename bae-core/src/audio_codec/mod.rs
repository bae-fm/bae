//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM, streamed from a sparse buffer),
//! encoding (PCM to FLAC/MP3/AAC/Opus/WAV/AIFF, streamed frame by frame into an
//! output sink), and seektable generation.

use std::fmt;

mod avio;
mod decode;
mod encode;
mod probe;
mod resample;

#[cfg(test)]
mod tests;

/// Buffer size for FFmpeg custom-IO (`avio`) contexts. The standard 32 KiB
/// FFmpeg uses for its own file IO.
const AVIO_BUFFER_SIZE: usize = 32768;

// Only the desktop-gated save path (StreamDecodeParams::run_to_sink) consumes
// this re-export; on iOS/Android that caller is compiled out, so the import is
// unused there and fails the deny(warnings) mobile clippy build.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use decode::decode_audio_to_sink_with_seek;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use decode::verification::decode_audio_to_verifying_sink;
pub use decode::{decode_audio, decode_audio_streaming, decode_audio_to_sink};
#[cfg(any(test, feature = "test-utils"))]
pub use encode::encode_i32;
pub use encode::{EncodeFormat, StreamEncodeFormat, StreamingEncoder, WriteSeek};
#[cfg(test)]
pub(crate) use probe::probe_opens_for;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use probe::seek_landing_bytes;
pub use probe::{probe_audio_from_path, ProbeResult};
pub use resample::Resampler;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingDecodeError {
    InputCancelled,
    Decode(String),
}

impl StreamingDecodeError {
    fn decode(message: impl Into<String>) -> Self {
        Self::Decode(message.into())
    }

    fn input_error(
        cancel_status: &std::sync::atomic::AtomicBool,
        message: impl Into<String>,
    ) -> Self {
        if cancel_status.load(std::sync::atomic::Ordering::Relaxed) {
            Self::InputCancelled
        } else {
            Self::Decode(message.into())
        }
    }
}

impl fmt::Display for StreamingDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputCancelled => write!(f, "streaming input cancelled"),
            Self::Decode(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StreamingDecodeError {}

/// A whole decode: interleaved i32 samples plus the format they're in.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
}

/// Receives a decode as it streams: the format once, then interleaved-i32 chunks.
///
/// `decode_audio_to_sink` pushes each frame straight in instead of accumulating a
/// `Vec`, so a consumer (loudness measurement) can process the audio as it arrives
/// and never hold the whole track's PCM at once. A sink that fails mid-stream
/// records the failure internally and keeps absorbing calls (the unsafe decoder
/// can't unwind through it); the caller checks the sink afterward.
pub trait DecodedSink {
    /// Called once after the stream is probed, before any samples.
    fn on_format(&mut self, sample_rate: u32, channels: u32);
    /// One interleaved-i32 chunk, already trimmed to the requested
    /// `[start_sample, end_sample)` window.
    fn on_samples(&mut self, samples: &[i32]);
    /// The count of fatal FFmpeg errors during the decode, reported once after the
    /// stream ends. Default: ignore it. A verifying sink captures it to flag a
    /// track whose bytes failed to decode. `0` for a clean decode.
    fn add_decode_error_count(&mut self, _count: u32) {}
    /// Invalid compressed packets discarded while the remaining stream kept
    /// decoding. A verifying sink combines this count with decoded-frame
    /// completeness; strict decode callers reject the packet instead.
    fn add_discarded_packet_count(&mut self, _count: u32) {}
}

/// Call once at startup.
pub fn init() {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");
}

/// An FFmpeg error code as its message.
pub(crate) fn av_err_str(errnum: i32) -> String {
    unsafe {
        let mut buf = [0 as std::ffi::c_char; 256];
        ffmpeg_sys_next::av_strerror(errnum, buf.as_mut_ptr(), buf.len());
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}
