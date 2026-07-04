//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM), encoding (PCM to FLAC), and
//! seektable generation. Uses custom AVIO for in-memory decoding.

use std::fmt;
use std::os::raw::{c_int, c_void};
use std::ptr;
use tracing::debug;

mod avio;
mod decode;
mod probe;

#[cfg(test)]
mod tests;

use avio::{
    avio_write_callback, avio_write_seek_callback, free_custom_avio_context, WriteAvioContext,
};

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

/// Decoded audio metadata and samples
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
}

impl DecodedAudio {
    /// Convert full-range i32 PCM samples to the f32 shape emitted by streaming playback.
    pub fn samples_as_f32(&self) -> Vec<f32> {
        let scale = i32::MAX as f32;
        self.samples
            .iter()
            .map(|&sample| sample as f32 / scale)
            .collect()
    }
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

/// Initialize FFmpeg (call once at startup)
pub fn init() {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");
}

/// Convert FFmpeg error code to string
pub(crate) fn av_err_str(errnum: i32) -> String {
    unsafe {
        let mut buf = [0 as std::ffi::c_char; 256];
        ffmpeg_sys_next::av_strerror(errnum, buf.as_mut_ptr(), buf.len());
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

/// Encode PCM samples to FLAC format.
///
/// Takes full-range interleaved i32 samples and returns the encoded FLAC data as
/// bytes. `bits_per_sample` is the target FLAC bit depth.
/// Uses FFmpeg library with custom AVIO for in-memory encoding.
///
/// `cancel` is checked between frames. When set, the encoder returns
/// `Err("encoding cancelled")` early — no partial output, the caller can
/// abandon the result and (if needed) clean up.
pub fn encode_to_flac(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    unsafe {
        encode_avio(
            samples,
            sample_rate,
            channels,
            EncodeFormat::Flac { bits_per_sample },
            cancel,
        )
    }
}

/// Encode PCM samples to MP3 format (320kbps CBR via libmp3lame).
///
/// Takes full-range interleaved i32 samples and returns the encoded MP3 data as bytes.
/// Uses FFmpeg library with custom AVIO for in-memory encoding.
pub fn encode_to_mp3(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bitrate: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    unsafe {
        encode_avio(
            samples,
            sample_rate,
            channels,
            EncodeFormat::Mp3 { bitrate },
            cancel,
        )
    }
}

#[derive(Clone, Copy)]
enum EncodeFormat {
    Flac { bits_per_sample: u32 },
    Mp3 { bitrate: u32 },
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
        }
    }

    fn format_name(self) -> *const std::ffi::c_char {
        match self {
            Self::Flac { .. } => c"flac".as_ptr(),
            Self::Mp3 { .. } => c"mp3".as_ptr(),
        }
    }

    fn codec_id(self) -> ffmpeg_sys_next::AVCodecID {
        match self {
            Self::Flac { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_FLAC,
            Self::Mp3 { .. } => ffmpeg_sys_next::AVCodecID::AV_CODEC_ID_MP3,
        }
    }

    fn sample_format(self) -> Result<ffmpeg_sys_next::AVSampleFormat, String> {
        use ffmpeg_sys_next::AVSampleFormat;
        match self {
            Self::Flac { bits_per_sample } => match bits_per_sample {
                1..=16 => Ok(AVSampleFormat::AV_SAMPLE_FMT_S16),
                17..=32 => Ok(AVSampleFormat::AV_SAMPLE_FMT_S32),
                _ => Err(format!("Unsupported FLAC bit depth: {bits_per_sample}")),
            },
            Self::Mp3 { .. } => Ok(AVSampleFormat::AV_SAMPLE_FMT_S16P),
        }
    }

    fn output_capacity(self, samples_len: usize) -> usize {
        match self {
            Self::Flac { .. } => samples_len * 2,
            Self::Mp3 { .. } => samples_len,
        }
    }

    fn default_frame_size(self) -> usize {
        match self {
            Self::Flac { .. } => 4096,
            Self::Mp3 { .. } => 1152,
        }
    }

    fn encoder_missing_message(self) -> &'static str {
        match self {
            Self::Flac { .. } => "FLAC encoder not found",
            Self::Mp3 { .. } => "MP3 encoder not found (libmp3lame)",
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
            Self::Mp3 { bitrate } => {
                (*codec_ctx).bit_rate = bitrate as i64;
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
            Self::Flac { bits_per_sample } => {
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
        }
    }
}

unsafe fn allocate_write_avio(
    write_ctx: &mut WriteAvioContext,
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
        Some(avio_write_seek_callback),
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

unsafe fn encode_avio(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    format: EncodeFormat,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    use ffmpeg_sys_next::*;

    if channels == 0 {
        return Err("channel count must be greater than zero".to_string());
    }
    let channels_usize = channels as usize;
    if !samples.len().is_multiple_of(channels_usize) {
        return Err("sample count must be divisible by channel count".to_string());
    }

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
    format.configure_codec_context(codec_ctx.ptr, sample_format, sample_rate, channels);
    let ret = avcodec_open2(codec_ctx.ptr, codec, ptr::null_mut());
    if ret < 0 {
        return Err(format!(
            "Failed to open {} encoder: {}",
            format.label(),
            av_err_str(ret)
        ));
    }

    let mut write_ctx =
        WriteAvioContext::new(Vec::with_capacity(format.output_capacity(samples.len())));
    let avio = allocate_write_avio(&mut write_ctx)?;

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
    let ret = avformat_write_header(muxer.fmt_ctx, ptr::null_mut());
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

    let frame_size = if (*codec_ctx.ptr).frame_size > 0 {
        (*codec_ctx.ptr).frame_size as usize
    } else {
        format.default_frame_size()
    };
    let samples_per_frame = frame_size * channels_usize;
    let mut sample_offset = 0;
    let mut pts: i64 = 0;

    while sample_offset < samples.len() {
        // Skip av_write_trailer on cancel: muxers seek-back-patch trailer
        // data (FLAC STREAMINFO, MP3 Xing) from frame state populated only
        // by completed frames, so invoking it on a partial stream is
        // unsafe. The output is discarded anyway.
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            muxer.write_trailer_on_drop = false;
            return Err("encoding cancelled".to_string());
        }

        let remaining = samples.len() - sample_offset;
        let chunk_samples = remaining.min(samples_per_frame);
        let chunk_frames = chunk_samples / channels_usize;

        (*frame_packet.frame).format = (*codec_ctx.ptr).sample_fmt as c_int;
        (*frame_packet.frame).ch_layout = (*codec_ctx.ptr).ch_layout;
        (*frame_packet.frame).sample_rate = sample_rate as c_int;
        (*frame_packet.frame).nb_samples = chunk_frames as c_int;

        let ret = av_frame_get_buffer(frame_packet.frame, 0);
        if ret < 0 {
            return Err(format!(
                "Failed to allocate frame buffer: {}",
                av_err_str(ret)
            ));
        }

        let ret = av_frame_make_writable(frame_packet.frame);
        if ret < 0 {
            return Err(format!(
                "Failed to make frame writable: {}",
                av_err_str(ret)
            ));
        }

        format.write_samples(
            frame_packet.frame,
            samples,
            sample_offset,
            chunk_samples,
            chunk_frames,
            channels_usize,
        );

        (*frame_packet.frame).pts = pts;
        pts += chunk_frames as i64;

        let ret = avcodec_send_frame(codec_ctx.ptr, frame_packet.frame);
        if ret < 0 {
            return Err(format!("Failed to send frame: {}", av_err_str(ret)));
        }
        av_frame_unref(frame_packet.frame);

        receive_and_write_packets(codec_ctx.ptr, frame_packet.packet, muxer.fmt_ctx)?;

        sample_offset += chunk_samples;
    }

    let ret = avcodec_send_frame(codec_ctx.ptr, ptr::null());
    if ret < 0 {
        return Err(format!("Failed to flush encoder: {}", av_err_str(ret)));
    }
    receive_and_write_packets(codec_ctx.ptr, frame_packet.packet, muxer.fmt_ctx)?;

    let ret = av_write_trailer(muxer.fmt_ctx);
    muxer.write_trailer_on_drop = false;
    if ret < 0 {
        return Err(format!("Failed to write trailer: {}", av_err_str(ret)));
    }

    avio_flush(muxer.avio);

    drop(muxer);
    let result = write_ctx.into_inner();

    debug!("Encoded {} bytes of {} data", result.len(), format.label());

    Ok(result)
}
