//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM, streamed from a sparse buffer),
//! encoding (PCM to FLAC/MP3/AAC/Opus/WAV/AIFF, streamed frame by frame into an
//! output sink), and seektable generation.

use std::fmt;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use tracing::debug;

mod avio;
mod decode;
mod probe;

#[cfg(test)]
mod tests;

use avio::{
    avio_write_callback, avio_write_seek_callback, free_custom_avio_context, WriteAvioContext,
};

pub(crate) use decode::decode_audio_to_sink_with_seek;
pub use decode::{decode_audio, decode_audio_streaming, decode_audio_to_sink};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use probe::seek_landing_bytes;
pub use probe::{probe_audio_from_path, ProbeResult};

/// Buffer size for FFmpeg custom-IO (`avio`) contexts. The standard 32 KiB
/// FFmpeg uses for its own file IO.
const AVIO_BUFFER_SIZE: usize = 32768;

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
    fn set_decode_error_count(&mut self, _count: u32) {}
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

/// The encoder's output codec + container, with its codec-specific knobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeFormat {
    Flac { bits_per_sample: u32 },
    Mp3 { bitrate_kbps: u32 },
    Aac { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
    PcmWav { bits_per_sample: u32 },
    PcmAiff { bits_per_sample: u32 },
}

struct EncodeCodecContext {
    ptr: *mut ffmpeg_sys_next::AVCodecContext,
}

impl Drop for EncodeCodecContext {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::avcodec_free_context(&mut self.ptr);
        }
    }
}

struct EncodeMuxer {
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
    write_trailer_on_drop: bool,
}

impl Drop for EncodeMuxer {
    fn drop(&mut self) {
        unsafe {
            if self.write_trailer_on_drop {
                ffmpeg_sys_next::av_write_trailer(self.fmt_ctx);
            }
            ffmpeg_sys_next::avformat_free_context(self.fmt_ctx);
            free_custom_avio_context(self.avio);
        }
    }
}

struct EncodeFramePacket {
    frame: *mut ffmpeg_sys_next::AVFrame,
    packet: *mut ffmpeg_sys_next::AVPacket,
}

impl Drop for EncodeFramePacket {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::av_frame_free(&mut self.frame);
            ffmpeg_sys_next::av_packet_free(&mut self.packet);
        }
    }
}

impl EncodeFormat {
    fn label(self) -> &'static str {
        match self {
            Self::Flac { .. } => "FLAC",
            Self::Mp3 { .. } => "MP3",
            Self::Aac { .. } => "AAC",
            Self::OpusOgg { .. } => "Opus/Ogg",
            Self::PcmWav { .. } => "WAV",
            Self::PcmAiff { .. } => "AIFF",
        }
    }

    fn format_name(self) -> *const std::ffi::c_char {
        match self {
            Self::Flac { .. } => c"flac".as_ptr(),
            Self::Mp3 { .. } => c"mp3".as_ptr(),
            // The .m4a flavor of the MP4 muxer.
            Self::Aac { .. } => c"ipod".as_ptr(),
            Self::OpusOgg { .. } => c"ogg".as_ptr(),
            Self::PcmWav { .. } => c"wav".as_ptr(),
            Self::PcmAiff { .. } => c"aiff".as_ptr(),
        }
    }

    fn codec_id(self) -> ffmpeg_sys_next::AVCodecID {
        match self {
            Self::Flac { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_FLAC,
            Self::Mp3 { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_MP3,
            Self::Aac { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_AAC,
            Self::OpusOgg { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_OPUS,
            Self::PcmWav { bits_per_sample } => pcm_codec_id(bits_per_sample, false),
            Self::PcmAiff { bits_per_sample } => pcm_codec_id(bits_per_sample, true),
        }
    }

    fn sample_format(self) -> Result<ffmpeg_sys_next::AVSampleFormat, String> {
        use ffmpeg_sys_next::AVSampleFormat;
        match self {
            Self::Flac { bits_per_sample }
            | Self::PcmWav { bits_per_sample }
            | Self::PcmAiff { bits_per_sample } => match bits_per_sample {
                1..=16 => Ok(AVSampleFormat::AV_SAMPLE_FMT_S16),
                17..=32 => Ok(AVSampleFormat::AV_SAMPLE_FMT_S32),
                _ => Err(format!("Unsupported PCM bit depth: {bits_per_sample}")),
            },
            Self::Mp3 { .. } => Ok(AVSampleFormat::AV_SAMPLE_FMT_S16P),
            // FFmpeg's native aac encoder accepts only planar float.
            Self::Aac { .. } => Ok(AVSampleFormat::AV_SAMPLE_FMT_FLTP),
            Self::OpusOgg { .. } => Ok(AVSampleFormat::AV_SAMPLE_FMT_S16),
        }
    }

    fn default_frame_size(self) -> usize {
        match self {
            Self::Flac { .. } => 4096,
            Self::Mp3 { .. } => 1152,
            Self::Aac { .. } => 1024,
            Self::OpusOgg { .. } => 960,
            Self::PcmWav { .. } | Self::PcmAiff { .. } => 4096,
        }
    }

    fn encoder_missing_message(self) -> &'static str {
        match self {
            Self::Flac { .. } => "FLAC encoder not found",
            Self::Mp3 { .. } => "MP3 encoder not found (libmp3lame)",
            Self::Aac { .. } => "AAC encoder not found",
            Self::OpusOgg { .. } => "Opus encoder not found (libopus)",
            Self::PcmWav { .. } => "WAV PCM encoder not found",
            Self::PcmAiff { .. } => "AIFF PCM encoder not found",
        }
    }

    unsafe fn configure_codec_context(
        self,
        codec_ctx: *mut ffmpeg_sys_next::AVCodecContext,
        sample_format: ffmpeg_sys_next::AVSampleFormat,
        sample_rate: u32,
        channels: u32,
    ) {
        use ffmpeg_sys_next::*;

        (*codec_ctx).sample_rate = sample_rate as c_int;
        (*codec_ctx).time_base = AVRational {
            num: 1,
            den: sample_rate as c_int,
        };
        (*codec_ctx).sample_fmt = sample_format;
        match self {
            Self::Flac { bits_per_sample } => {
                (*codec_ctx).bits_per_raw_sample = bits_per_sample as c_int;
            }
            Self::Mp3 { bitrate_kbps } => {
                (*codec_ctx).bit_rate = bitrate_kbps as i64 * 1000;
            }
            Self::Aac { bitrate_kbps } => {
                (*codec_ctx).bit_rate = bitrate_kbps as i64 * 1000;
            }
            Self::OpusOgg { bitrate_kbps } => {
                (*codec_ctx).bit_rate = bitrate_kbps as i64 * 1000;
            }
            Self::PcmWav { bits_per_sample } | Self::PcmAiff { bits_per_sample } => {
                (*codec_ctx).bits_per_raw_sample = bits_per_sample as c_int;
            }
        }

        let mut ch_layout: AVChannelLayout = std::mem::zeroed();
        av_channel_layout_default(&mut ch_layout, channels as c_int);
        (*codec_ctx).ch_layout = ch_layout;
    }

    unsafe fn write_samples(
        self,
        frame: *mut ffmpeg_sys_next::AVFrame,
        samples: &[i32],
        sample_offset: usize,
        chunk_samples: usize,
        chunk_frames: usize,
        channels: usize,
    ) {
        match self {
            Self::Flac { bits_per_sample }
            | Self::PcmWav { bits_per_sample }
            | Self::PcmAiff { bits_per_sample } => {
                let frame_data = (*frame).data[0];
                match bits_per_sample {
                    1..=16 => {
                        let dst = frame_data as *mut i16;
                        for i in 0..chunk_samples {
                            *dst.add(i) = (samples[sample_offset + i] >> 16) as i16;
                        }
                    }
                    17..=32 => {
                        let dst = frame_data as *mut i32;
                        for i in 0..chunk_samples {
                            *dst.add(i) = samples[sample_offset + i];
                        }
                    }
                    _ => unreachable!("FLAC bit depth was validated before encoding"),
                }
            }
            Self::Mp3 { .. } => {
                for ch in 0..channels {
                    let dst = (*frame).data[ch] as *mut i16;
                    for i in 0..chunk_frames {
                        let src_idx = sample_offset + i * channels + ch;
                        *dst.add(i) = (samples[src_idx] >> 16) as i16;
                    }
                }
            }
            Self::Aac { .. } => {
                // Planar float, one buffer per channel: the full-range i32
                // sample maps to [-1, 1) by dividing by 2^31.
                const SCALE: f32 = 2_147_483_648.0;
                for ch in 0..channels {
                    let dst = (*frame).data[ch] as *mut f32;
                    for i in 0..chunk_frames {
                        let src_idx = sample_offset + i * channels + ch;
                        *dst.add(i) = samples[src_idx] as f32 / SCALE;
                    }
                }
            }
            Self::OpusOgg { .. } => {
                let dst = (*frame).data[0] as *mut i16;
                for i in 0..chunk_samples {
                    *dst.add(i) = (samples[sample_offset + i] >> 16) as i16;
                }
            }
        }
    }
}

fn pcm_codec_id(bits_per_sample: u32, big_endian: bool) -> ffmpeg_sys_next::AVCodecID {
    match (bits_per_sample, big_endian) {
        (1..=16, false) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S16LE,
        (1..=16, true) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S16BE,
        (17..=24, false) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S24LE,
        (17..=24, true) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S24BE,
        (25..=32, false) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S32LE,
        (25..=32, true) => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_PCM_S32BE,
        _ => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_NONE,
    }
}

/// Build the muxer's AVIO over the encoder's sink. The seek callback is
/// installed only for a seekable sink; without one FFmpeg takes each muxer's
/// streaming path.
unsafe fn allocate_write_avio(
    write_ctx: &mut WriteAvioContext,
    seekable: bool,
) -> Result<*mut ffmpeg_sys_next::AVIOContext, String> {
    use ffmpeg_sys_next::*;

    let avio_buffer = av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
    if avio_buffer.is_null() {
        return Err("Failed to allocate AVIO buffer".to_string());
    }
    let avio = avio_alloc_context(
        avio_buffer,
        AVIO_BUFFER_SIZE as c_int,
        1,
        write_ctx as *mut WriteAvioContext as *mut c_void,
        None,
        Some(avio_write_callback),
        if seekable {
            Some(avio_write_seek_callback)
        } else {
            None
        },
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        return Err("Failed to create AVIO context".to_string());
    }
    Ok(avio)
}

unsafe fn receive_and_write_packets(
    codec_ctx: *mut ffmpeg_sys_next::AVCodecContext,
    packet: *mut ffmpeg_sys_next::AVPacket,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    loop {
        let ret = avcodec_receive_packet(codec_ctx, packet);
        if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
            break;
        }
        if ret < 0 {
            return Err(format!("Failed to receive packet: {}", av_err_str(ret)));
        }

        (*packet).stream_index = 0;
        let ret = av_interleaved_write_frame(fmt_ctx, packet);
        if ret < 0 {
            return Err(format!("Failed to write packet: {}", av_err_str(ret)));
        }
    }

    Ok(())
}

/// A seekable byte sink for the encoder: a file, or an in-memory cursor in
/// tests. Every encode format accepts one — the muxer can seek back and patch
/// its header.
pub trait WriteSeek: std::io::Write + std::io::Seek + Send {}
impl<T: std::io::Write + std::io::Seek + Send> WriteSeek for T {}

/// Formats whose muxer has a true streaming path — no header patch-back — so
/// they may write to a non-seekable sink.
///
/// - **Ogg** streams natively.
/// - **MP3** streams once its Xing/LAME VBR header is disabled (`write_xing=0`,
///   set in [`StreamingEncoder::open_encoder`] for the non-seekable sink),
///   leaving a plain CBR frame stream with nothing to seek back and patch.
///
/// FLAC's STREAMINFO (total samples, md5), the RIFF/FORM sizes of WAV/AIFF,
/// and the AAC/.m4a sample-table `moov` (written whole on finalize) have no
/// such streaming mode — they are always patched by seeking back over the
/// header — so they stay out of this enum: pairing a header-patching muxer
/// with a sink it cannot patch is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamEncodeFormat {
    Mp3 { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
}

impl From<StreamEncodeFormat> for EncodeFormat {
    fn from(format: StreamEncodeFormat) -> Self {
        match format {
            StreamEncodeFormat::Mp3 { bitrate_kbps } => EncodeFormat::Mp3 { bitrate_kbps },
            StreamEncodeFormat::OpusOgg { bitrate_kbps } => EncodeFormat::OpusOgg { bitrate_kbps },
        }
    }
}

/// The FFmpeg half of a live encode, opened on the first `on_format`.
struct OpenEncoder {
    codec_ctx: EncodeCodecContext,
    muxer: EncodeMuxer,
    frame_packet: EncodeFramePacket,
    /// Persistent resampler (input rate → the codec's rate) for Opus at a
    /// non-48 kHz input; null otherwise. Converts chunk by chunk; its tail is
    /// drained at `finish`.
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    /// The announced input shape; a later `on_format` must match it.
    sample_rate: u32,
    channels: u32,
    samples_per_frame: usize,
    /// Post-resample samples awaiting a full codec frame.
    pending: Vec<i32>,
    pts: i64,
}

impl Drop for OpenEncoder {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_sys_next::swr_free(&mut self.swr_ctx);
        }
    }
}

impl OpenEncoder {
    /// Encode the first `chunk_samples` of `pending` as one frame and drain the
    /// muxer's packets.
    unsafe fn encode_pending_frame(
        &mut self,
        format: EncodeFormat,
        chunk_samples: usize,
    ) -> Result<(), String> {
        use ffmpeg_sys_next::*;

        let channels = self.channels as usize;
        let chunk_frames = chunk_samples / channels;

        (*self.frame_packet.frame).format = (*self.codec_ctx.ptr).sample_fmt as c_int;
        (*self.frame_packet.frame).ch_layout = (*self.codec_ctx.ptr).ch_layout;
        (*self.frame_packet.frame).sample_rate = (*self.codec_ctx.ptr).sample_rate;
        (*self.frame_packet.frame).nb_samples = chunk_frames as c_int;

        let ret = av_frame_get_buffer(self.frame_packet.frame, 0);
        if ret < 0 {
            return Err(format!(
                "Failed to allocate frame buffer: {}",
                av_err_str(ret)
            ));
        }

        let ret = av_frame_make_writable(self.frame_packet.frame);
        if ret < 0 {
            return Err(format!(
                "Failed to make frame writable: {}",
                av_err_str(ret)
            ));
        }

        format.write_samples(
            self.frame_packet.frame,
            &self.pending,
            0,
            chunk_samples,
            chunk_frames,
            channels,
        );

        (*self.frame_packet.frame).pts = self.pts;
        self.pts += chunk_frames as i64;

        let ret = avcodec_send_frame(self.codec_ctx.ptr, self.frame_packet.frame);
        if ret < 0 {
            return Err(format!("Failed to send frame: {}", av_err_str(ret)));
        }
        av_frame_unref(self.frame_packet.frame);

        receive_and_write_packets(
            self.codec_ctx.ptr,
            self.frame_packet.packet,
            self.muxer.fmt_ctx,
        )?;

        self.pending.drain(..chunk_samples);
        Ok(())
    }

    /// Convert an input chunk through the resampler (if any) into `pending`.
    unsafe fn absorb(&mut self, samples: &[i32]) -> Result<(), String> {
        use ffmpeg_sys_next::*;

        if self.swr_ctx.is_null() {
            self.pending.extend_from_slice(samples);
            return Ok(());
        }

        let channels = self.channels as usize;
        let input_frames = samples.len() / channels;
        let capacity_frames = swr_get_out_samples(self.swr_ctx, input_frames as c_int);
        if capacity_frames < 0 {
            return Err(format!(
                "Failed to size resampler output: {}",
                av_err_str(capacity_frames)
            ));
        }
        let out_offset = self.pending.len();
        self.pending
            .resize(out_offset + capacity_frames as usize * channels, 0);
        let input_ptr = samples.as_ptr() as *const u8;
        let output_ptr = self.pending[out_offset..].as_mut_ptr() as *mut u8;
        let converted = swr_convert(
            self.swr_ctx,
            &output_ptr,
            capacity_frames,
            &input_ptr,
            input_frames as c_int,
        );
        if converted < 0 {
            return Err(format!("Failed to resample: {}", av_err_str(converted)));
        }
        self.pending
            .truncate(out_offset + converted as usize * channels);
        Ok(())
    }

    /// Drain the resampler's buffered tail into `pending` (no new input).
    unsafe fn drain_resampler(&mut self) -> Result<(), String> {
        use ffmpeg_sys_next::*;

        if self.swr_ctx.is_null() {
            return Ok(());
        }
        let channels = self.channels as usize;
        loop {
            let capacity_frames = swr_get_out_samples(self.swr_ctx, 0);
            if capacity_frames < 0 {
                return Err(format!(
                    "Failed to size resampler tail: {}",
                    av_err_str(capacity_frames)
                ));
            }
            if capacity_frames == 0 {
                return Ok(());
            }
            let out_offset = self.pending.len();
            self.pending
                .resize(out_offset + capacity_frames as usize * channels, 0);
            let output_ptr = self.pending[out_offset..].as_mut_ptr() as *mut u8;
            let converted = swr_convert(self.swr_ctx, &output_ptr, capacity_frames, ptr::null(), 0);
            if converted < 0 {
                return Err(format!(
                    "Failed to drain resampler: {}",
                    av_err_str(converted)
                ));
            }
            self.pending
                .truncate(out_offset + converted as usize * channels);
            if converted == 0 {
                return Ok(());
            }
        }
    }
}

/// PCM in → encoded bytes out, one frame at a time, as a [`DecodedSink`]: the
/// decode's `on_format` opens the codec and muxer and writes the header, each
/// `on_samples` chunk fills frames and drains packets into the output sink, and
/// [`Self::finish`] flushes the codec, writes the trailer, and surfaces any
/// failure recorded mid-stream (the trait's methods cannot return errors — the
/// unsafe decoder can't unwind through the sink). Resident memory is one codec
/// frame plus the 32 KiB AVIO buffer.
///
/// The two constructors are the two output types: [`Self::seekable`] accepts
/// every format (the muxer may seek back and patch its header);
/// [`Self::streaming`] accepts only [`StreamEncodeFormat`] — pairing a
/// header-patching muxer with a sink it cannot patch is unrepresentable, so a
/// file with a lying header cannot be written.
pub struct StreamingEncoder {
    /// FFmpeg state; declared before `sink` so its muxer (whose AVIO points at
    /// the sink box) is torn down first.
    open: Option<OpenEncoder>,
    format: EncodeFormat,
    /// Heap-pinned: the muxer's AVIO holds a raw pointer to it.
    sink: Box<WriteAvioContext>,
    seekable_sink: bool,
    /// Checked per encoded frame. Once set, the encoder records "encoding
    /// cancelled" and stops writing — no trailer, since a muxer patches its
    /// trailer from frame state only completed streams populate.
    cancel: Arc<std::sync::atomic::AtomicBool>,
    /// First failure recorded during the stream; later calls are absorbed.
    error: Option<String>,
    /// Interleaved samples accepted through `on_samples` (pre-resample).
    samples_accepted: u64,
}

impl StreamingEncoder {
    /// Encode `format` into a seekable sink (a file; an in-memory cursor in
    /// tests). The muxer's header patch-back works, so totals and sizes in the
    /// header are real.
    pub fn seekable(
        format: EncodeFormat,
        out: Box<dyn WriteSeek>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self::new(format, WriteAvioContext::Seekable(out), true, cancel)
    }

    /// Encode a streaming-safe format into a non-seekable sink (a socket). No
    /// seek callback is installed, so FFmpeg takes the muxer's streaming path.
    pub fn streaming(
        format: StreamEncodeFormat,
        out: Box<dyn std::io::Write + Send>,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self::new(
            format.into(),
            WriteAvioContext::Streaming(out),
            false,
            cancel,
        )
    }

    fn new(
        format: EncodeFormat,
        sink: WriteAvioContext,
        seekable_sink: bool,
        cancel: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            open: None,
            format,
            sink: Box::new(sink),
            seekable_sink,
            cancel,
            error: None,
            samples_accepted: 0,
        }
    }

    /// Interleaved frames accepted through `on_samples` so far, at the input
    /// rate. The CUE-image save reads per-track deltas off this for its INDEX
    /// lines.
    pub fn frames_accepted(&self) -> u64 {
        match &self.open {
            Some(open) => self.samples_accepted / u64::from(open.channels.max(1)),
            None => 0,
        }
    }

    /// The first failure recorded during the stream, if any. Lets a driver
    /// running several decodes into one encoder stop at the failing one
    /// instead of feeding a dead encoder to the end.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    fn record_error(&mut self, message: String) {
        // A failed stream must not get a trailer: a muxer patches it from
        // frame state only completed streams populate.
        if let Some(open) = &mut self.open {
            open.muxer.write_trailer_on_drop = false;
        }
        if self.error.is_none() {
            self.error = Some(message);
        }
    }

    /// Open the codec + muxer for the announced input shape and write the
    /// header.
    unsafe fn open_encoder(
        &mut self,
        sample_rate: u32,
        channels: u32,
    ) -> Result<OpenEncoder, String> {
        use ffmpeg_sys_next::*;

        if channels == 0 {
            return Err("channel count must be greater than zero".to_string());
        }
        let format = self.format;
        // Opus encodes at 48 kHz regardless of the input rate; the resampler
        // below converts each chunk.
        let output_rate = match format {
            EncodeFormat::OpusOgg { .. } => 48_000,
            _ => sample_rate,
        };
        let sample_format = format.sample_format()?;

        let codec = avcodec_find_encoder(format.codec_id());
        if codec.is_null() {
            return Err(format.encoder_missing_message().to_string());
        }

        let codec_ctx = avcodec_alloc_context3(codec);
        if codec_ctx.is_null() {
            return Err("Failed to allocate codec context".to_string());
        }
        let codec_ctx = EncodeCodecContext { ptr: codec_ctx };
        format.configure_codec_context(codec_ctx.ptr, sample_format, output_rate, channels);
        let ret = avcodec_open2(codec_ctx.ptr, codec, ptr::null_mut());
        if ret < 0 {
            return Err(format!(
                "Failed to open {} encoder: {}",
                format.label(),
                av_err_str(ret)
            ));
        }

        let avio = allocate_write_avio(self.sink.as_mut(), self.seekable_sink)?;

        let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
        let ret = avformat_alloc_output_context2(
            &mut fmt_ctx,
            ptr::null(),
            format.format_name(),
            ptr::null(),
        );
        if ret < 0 || fmt_ctx.is_null() {
            free_custom_avio_context(avio);
            return Err("Failed to create output context".to_string());
        }
        (*fmt_ctx).pb = avio;
        (*fmt_ctx).flags |= AVFMT_FLAG_CUSTOM_IO as c_int;
        let mut muxer = EncodeMuxer {
            fmt_ctx,
            avio,
            write_trailer_on_drop: false,
        };

        let stream = avformat_new_stream(muxer.fmt_ctx, ptr::null());
        if stream.is_null() {
            return Err("Failed to create stream".to_string());
        }
        let ret = avcodec_parameters_from_context((*stream).codecpar, codec_ctx.ptr);
        if ret < 0 {
            return Err(format!("Failed to copy codec params: {}", av_err_str(ret)));
        }
        // The MP3 muxer writes a Xing/LAME VBR header frame it patches by seeking
        // back at trailer time. A non-seekable sink (a socket) can't be patched,
        // so drop the header entirely — the result is a plain CBR frame stream
        // that needs no seek-back. Only the streaming sink sets this; the seekable
        // save path keeps the Xing header.
        let mut muxer_options: *mut AVDictionary = ptr::null_mut();
        if !self.seekable_sink && matches!(format, EncodeFormat::Mp3 { .. }) {
            av_dict_set(&mut muxer_options, c"write_xing".as_ptr(), c"0".as_ptr(), 0);
        }
        let ret = avformat_write_header(muxer.fmt_ctx, &mut muxer_options);
        av_dict_free(&mut muxer_options);
        if ret < 0 {
            return Err(format!("Failed to write header: {}", av_err_str(ret)));
        }
        muxer.write_trailer_on_drop = true;

        let frame = av_frame_alloc();
        if frame.is_null() {
            return Err("Failed to allocate frame".to_string());
        }
        let packet = av_packet_alloc();
        if packet.is_null() {
            av_frame_free(&mut (frame as *mut _));
            return Err("Failed to allocate packet".to_string());
        }
        let frame_packet = EncodeFramePacket { frame, packet };

        let mut swr_ctx: *mut SwrContext = ptr::null_mut();
        if output_rate != sample_rate {
            let mut ch_layout: AVChannelLayout = std::mem::zeroed();
            av_channel_layout_default(&mut ch_layout, channels as c_int);
            let ret = swr_alloc_set_opts2(
                &mut swr_ctx,
                &ch_layout,
                AVSampleFormat::AV_SAMPLE_FMT_S32,
                output_rate as c_int,
                &ch_layout,
                AVSampleFormat::AV_SAMPLE_FMT_S32,
                sample_rate as c_int,
                0,
                ptr::null_mut(),
            );
            if ret < 0 || swr_ctx.is_null() {
                return Err(format!("Failed to allocate resampler: {}", av_err_str(ret)));
            }
            let ret = swr_init(swr_ctx);
            if ret < 0 {
                swr_free(&mut swr_ctx);
                return Err(format!("Failed to init resampler: {}", av_err_str(ret)));
            }
        }

        let frame_size = if (*codec_ctx.ptr).frame_size > 0 {
            (*codec_ctx.ptr).frame_size as usize
        } else {
            format.default_frame_size()
        };

        Ok(OpenEncoder {
            codec_ctx,
            muxer,
            frame_packet,
            swr_ctx,
            sample_rate,
            channels,
            samples_per_frame: frame_size * channels as usize,
            pending: Vec::new(),
            pts: 0,
        })
    }

    /// Flush the codec, drain the resampler tail, write the trailer, and flush
    /// the sink. Surfaces the first error recorded during the stream, and fails
    /// on a cancelled encode (no trailer is written in either case).
    pub fn finish(mut self) -> Result<(), String> {
        use ffmpeg_sys_next::*;

        if let Some(error) = self.error.take() {
            return Err(error);
        }
        let Some(mut open) = self.open.take() else {
            return Err("nothing encoded: the decode announced no format".to_string());
        };

        // SAFETY: all pointers below were allocated by `open_encoder` and are
        // torn down exactly once via the RAII wrappers in `open`.
        unsafe {
            open.drain_resampler().inspect_err(|_| {
                open.muxer.write_trailer_on_drop = false;
            })?;

            if open.pending.len() % open.channels as usize != 0 {
                open.muxer.write_trailer_on_drop = false;
                return Err("sample count must be divisible by channel count".to_string());
            }
            if !open.pending.is_empty() {
                let remaining = open.pending.len();
                open.encode_pending_frame(self.format, remaining)
                    .inspect_err(|_| {
                        open.muxer.write_trailer_on_drop = false;
                    })?;
            }

            if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                open.muxer.write_trailer_on_drop = false;
                return Err("encoding cancelled".to_string());
            }

            let ret = avcodec_send_frame(open.codec_ctx.ptr, ptr::null());
            if ret < 0 {
                open.muxer.write_trailer_on_drop = false;
                return Err(format!("Failed to flush encoder: {}", av_err_str(ret)));
            }
            receive_and_write_packets(
                open.codec_ctx.ptr,
                open.frame_packet.packet,
                open.muxer.fmt_ctx,
            )
            .inspect_err(|_| {
                open.muxer.write_trailer_on_drop = false;
            })?;

            let ret = av_write_trailer(open.muxer.fmt_ctx);
            open.muxer.write_trailer_on_drop = false;
            if ret < 0 {
                return Err(format!("Failed to write trailer: {}", av_err_str(ret)));
            }

            avio_flush(open.muxer.avio);
        }

        // Free the muxer (and its AVIO over the sink) before flushing and
        // dropping the sink itself.
        drop(open);
        self.sink
            .flush()
            .map_err(|e| format!("Failed to flush encode sink: {e}"))?;

        debug!("Encoded {} stream finished", self.format.label());
        Ok(())
    }
}

impl DecodedSink for StreamingEncoder {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        if self.error.is_some() {
            return;
        }
        match &self.open {
            Some(open) => {
                if open.sample_rate != sample_rate || open.channels != channels {
                    self.record_error(format!(
                        "PCM shape changed mid-encode: {}Hz/{}ch then {sample_rate}Hz/{channels}ch",
                        open.sample_rate, open.channels
                    ));
                }
            }
            None => {
                // SAFETY: first open; all allocated pointers are owned by the
                // returned OpenEncoder's RAII wrappers.
                match unsafe { self.open_encoder(sample_rate, channels) } {
                    Ok(open) => self.open = Some(open),
                    Err(e) => self.record_error(e),
                }
            }
        }
    }

    fn on_samples(&mut self, samples: &[i32]) {
        if self.error.is_some() || samples.is_empty() {
            return;
        }
        if self.open.is_none() {
            self.record_error("samples arrived before a format was announced".to_string());
            return;
        }
        self.samples_accepted += samples.len() as u64;

        // SAFETY: `open` holds valid FFmpeg state built by `open_encoder`.
        let result = unsafe {
            let open = self.open.as_mut().expect("checked above");
            let mut outcome = open.absorb(samples);
            while outcome.is_ok() && open.pending.len() >= open.samples_per_frame {
                if self.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    outcome = Err("encoding cancelled".to_string());
                    break;
                }
                let chunk = open.samples_per_frame;
                outcome = open.encode_pending_frame(self.format, chunk);
            }
            outcome
        };
        if let Err(e) = result {
            self.record_error(e);
        }
    }
}

/// Whole-buffer convenience over [`StreamingEncoder`] for fixtures and tests:
/// encode interleaved i32 PCM into an in-memory seekable sink.
#[cfg(any(test, feature = "test-utils"))]
pub fn encode_i32(
    format: EncodeFormat,
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
) -> Result<Vec<u8>, String> {
    use std::io::{Cursor, Seek, SeekFrom, Write};
    use std::sync::{Arc as StdArc, Mutex};

    /// Hands the encoder a boxed sink while keeping a handle to read the bytes
    /// back after `finish` drops the encoder's box.
    #[derive(Clone)]
    struct SharedCursor(StdArc<Mutex<Cursor<Vec<u8>>>>);
    impl Write for SharedCursor {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }
    impl Seek for SharedCursor {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            self.0.lock().unwrap().seek(pos)
        }
    }

    let shared = SharedCursor(StdArc::new(Mutex::new(Cursor::new(Vec::new()))));
    let mut encoder = StreamingEncoder::seekable(
        format,
        Box::new(shared.clone()),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );
    encoder.on_format(sample_rate, channels);
    encoder.on_samples(samples);
    encoder.finish()?;
    let cursor = StdArc::try_unwrap(shared.0)
        .map_err(|_| "encode sink still shared after finish".to_string())?;
    Ok(cursor.into_inner().unwrap().into_inner())
}
