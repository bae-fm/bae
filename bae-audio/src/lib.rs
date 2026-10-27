//! bae's audio codec over FFmpeg, and the buffers it reads from and writes to.
//!
//! Provides decoding (any format to PCM, streamed from a [`sparse_buffer`]; the
//! import's verifying decode also takes scrub points), encoding (PCM to
//! FLAC/MP3/AAC/Opus/WAV/AIFF, streamed frame by frame into an output sink),
//! and probing (a file's audio facts, and the byte a seek to a sample lands
//! on). A decode for playback writes into one track's [`track_stream`].

mod avio;
mod decode;
mod encode;
mod probe;
mod resample;
mod sample_clock;
pub mod sparse_buffer;
#[cfg(any(test, feature = "test-instrumentation"))]
pub mod test_reads;
pub mod track_stream;
#[cfg(any(test, feature = "test-utils"))]
mod wav_read;

#[cfg(test)]
mod test_source;
#[cfg(test)]
mod tests;

/// Buffer size for FFmpeg custom-IO (`avio`) contexts. The standard 32 KiB
/// FFmpeg uses for its own file IO.
pub const AVIO_BUFFER_SIZE: usize = 32768;

pub use decode::byte_jump_keeps_place;
#[cfg(desktop)]
pub use decode::verification::decode_audio_to_verifying_sink;
pub use decode::DecodeStart;
pub use decode::{
    decode_audio, decode_audio_streaming, decode_audio_to_sink, decode_audio_to_sink_with_seek,
};
#[cfg(any(test, feature = "test-utils"))]
pub use encode::encode_i32;
pub use encode::{EncodeFormat, StreamEncodeFormat, StreamingEncoder, WriteSeek};
#[cfg(desktop)]
pub use probe::seek_landing_bytes;
pub use probe::{probe_audio_from_path, ProbeResult};
pub use resample::Resampler;
#[cfg(any(test, feature = "test-utils"))]
pub use wav_read::{parse_streamed_wav, StreamedWav};

/// Why a decode stopped short of its window.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DecodeError {
    /// The input was cancelled: teardown, or the caller aborting. Nothing
    /// failed.
    #[error("decode input cancelled")]
    InputCancelled,
    /// The source's bytes could not be read -- a missing file, a full disk, a
    /// dropped network volume, a failed cloud fetch. Says nothing about the
    /// audio: the bytes never reached the decoder. Carries the read's own error.
    #[error("source read failed: {0}")]
    SourceRead(sparse_buffer::SourceReadError),
    /// The bytes that arrived don't decode as audio.
    #[error("{0}")]
    Decode(String),
}

impl DecodeError {
    fn decode(message: impl Into<String>) -> Self {
        Self::Decode(message.into())
    }

    /// Classify a failure after the input was opened: a read failure the AVIO
    /// callback recorded is the cause, whatever FFmpeg reported for it;
    /// otherwise it is the decode's own failure.
    fn after_read(
        read_failure: Option<sparse_buffer::SourceReadError>,
        message: impl Into<String>,
    ) -> Self {
        match read_failure {
            Some(error) => Self::SourceRead(error),
            None => Self::decode(message),
        }
    }
}

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
    /// The stream the decode opened, once per decode, before its first
    /// packet. Default: ignore it. Import's verifying sink reads it to decide
    /// whether the packets that follow give scrub points.
    fn on_stream(&mut self, _stream: &DecodedStream) {}
    /// Each packet the decoder accepted, in file order. Default: ignore it.
    fn on_packet(&mut self, _packet: DecodedPacket) {}
}

/// The stream a sink decode opened, as its sink sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedStream {
    /// The codec the decode found.
    pub content_type: bae_util::content_type::ContentType,
    /// Whether the seek index the demuxer built from the file's header holds
    /// an entry past the stream's first sample. For a FLAC that index is its
    /// SEEKTABLE, and it is what FFmpeg seeks a sample through: without one, a
    /// FLAC seek binary-searches the file, and a table of only placeholder
    /// points gives one entry before the first sample, so its seek lands on
    /// the first frame and decodes everything up to the target.
    pub header_index_passes_start: bool,
    /// Where the decode started reading the stream. A lossy stream's sink
    /// decode starts at the stream's start whatever it was asked.
    pub start: DecodeStart,
    pub sample_rate: u32,
    /// Where the decode's window ends: frames from here on belong to what
    /// follows it.
    pub stop_at_sample: Option<u64>,
}

/// A packet the decoder accepted, as the demuxer placed it: one frame, for
/// FLAC and MP3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedPacket {
    /// The byte the packet begins at in the file; `None` when the demuxer
    /// doesn't know it.
    pub byte: Option<u64>,
    /// The packet's first sample on the stream's own count, which puts an
    /// MP3's first frame before sample 0 (its encoder delay); `None` when the
    /// packet carries no timestamp.
    pub sample: Option<i64>,
    /// Its length in bytes.
    pub size: u64,
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
