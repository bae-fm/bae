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

use avio::{avio_write_callback, avio_write_seek_callback, WriteAvioContext};

pub use decode::{decode_audio, decode_audio_streaming, decode_audio_to_sink};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use probe::{frame_byte_offsets, seek_landing_bytes};
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

/// Free the FFmpeg encode resources on a post-header error path: the packet,
/// the frame, then finalize and free the format + codec contexts. Used by the
/// encode loops' failure returns. The cancel path deliberately does NOT use
/// this — it must skip `av_write_trailer` on a partial stream.
macro_rules! free_encode_resources {
    ($packet:expr, $frame:expr, $fmt_ctx:expr, $codec_ctx:expr) => {{
        av_packet_free(&mut ($packet as *mut _));
        av_frame_free(&mut ($frame as *mut _));
        av_write_trailer($fmt_ctx);
        avformat_free_context($fmt_ctx);
        avcodec_free_context(&mut ($codec_ctx as *mut _));
    }};
}

/// Decoded audio metadata and samples
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
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
    fn on_format(&mut self, sample_rate: u32, channels: u32, bits_per_sample: u32);
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
/// Takes interleaved i32 samples and returns the encoded FLAC data as bytes.
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
    unsafe { encode_to_flac_avio(samples, sample_rate, channels, bits_per_sample, cancel) }
}

/// Internal AVIO-based FLAC encoding implementation
unsafe fn encode_to_flac_avio(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    use ffmpeg_sys_next::*;

    // Create write context
    let mut write_ctx = Box::new(WriteAvioContext {
        data: Vec::with_capacity(samples.len() * 2), // Rough estimate
        pos: 0,
    });

    // Allocate AVIO buffer
    let avio_buffer_size = AVIO_BUFFER_SIZE;
    let avio_buffer = av_malloc(avio_buffer_size) as *mut u8;
    if avio_buffer.is_null() {
        return Err("Failed to allocate AVIO buffer".to_string());
    }

    // Create custom AVIO context for writing
    let avio = avio_alloc_context(
        avio_buffer,
        avio_buffer_size as c_int,
        1, // write flag
        write_ctx.as_mut() as *mut WriteAvioContext as *mut c_void,
        None, // no read
        Some(avio_write_callback),
        Some(avio_write_seek_callback),
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        return Err("Failed to create AVIO context".to_string());
    }

    // Find FLAC encoder
    let codec = avcodec_find_encoder(AVCodecID::AV_CODEC_ID_FLAC);
    if codec.is_null() {
        avio_context_free(&mut (avio as *mut _));
        return Err("FLAC encoder not found".to_string());
    }

    // Allocate codec context
    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        avio_context_free(&mut (avio as *mut _));
        return Err("Failed to allocate codec context".to_string());
    }

    // Configure encoder
    (*codec_ctx).sample_rate = sample_rate as c_int;
    (*codec_ctx).time_base = AVRational {
        num: 1,
        den: sample_rate as c_int,
    };

    // Set sample format based on bits per sample
    // 24-bit uses S32 container with bits_per_raw_sample=24
    (*codec_ctx).sample_fmt = match bits_per_sample {
        16 => AVSampleFormat::AV_SAMPLE_FMT_S16,
        24 | 32 => AVSampleFormat::AV_SAMPLE_FMT_S32,
        _ => AVSampleFormat::AV_SAMPLE_FMT_S16,
    };
    (*codec_ctx).bits_per_raw_sample = bits_per_sample as c_int;

    // Set channel layout
    let mut ch_layout: AVChannelLayout = std::mem::zeroed();
    av_channel_layout_default(&mut ch_layout, channels as c_int);
    (*codec_ctx).ch_layout = ch_layout;

    // Open encoder
    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avio_context_free(&mut (avio as *mut _));
        return Err(format!("Failed to open encoder: {}", av_err_str(ret)));
    }

    // Create output format context
    let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
    let ret =
        avformat_alloc_output_context2(&mut fmt_ctx, ptr::null(), c"flac".as_ptr(), ptr::null());
    if ret < 0 || fmt_ctx.is_null() {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avio_context_free(&mut (avio as *mut _));
        return Err("Failed to create output context".to_string());
    }

    // Use our custom AVIO
    (*fmt_ctx).pb = avio;
    (*fmt_ctx).flags |= AVFMT_FLAG_CUSTOM_IO as c_int;

    // Add audio stream
    let stream = avformat_new_stream(fmt_ctx, ptr::null());
    if stream.is_null() {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to create stream".to_string());
    }

    // Copy codec parameters to stream
    let ret = avcodec_parameters_from_context((*stream).codecpar, codec_ctx);
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(format!("Failed to copy codec params: {}", av_err_str(ret)));
    }

    // Write header
    let ret = avformat_write_header(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(format!("Failed to write header: {}", av_err_str(ret)));
    }

    // Allocate frame
    let frame = av_frame_alloc();
    if frame.is_null() {
        av_write_trailer(fmt_ctx);
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to allocate frame".to_string());
    }

    (*frame).format = (*codec_ctx).sample_fmt as c_int;
    (*frame).ch_layout = (*codec_ctx).ch_layout;
    (*frame).sample_rate = sample_rate as c_int;

    // Allocate packet
    let packet = av_packet_alloc();
    if packet.is_null() {
        av_frame_free(&mut (frame as *mut _));
        av_write_trailer(fmt_ctx);
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to allocate packet".to_string());
    }

    // Process samples in chunks matching encoder's frame size
    let frame_size = if (*codec_ctx).frame_size > 0 {
        (*codec_ctx).frame_size as usize
    } else {
        4096 // Default for variable frame size codecs
    };

    let samples_per_frame = frame_size * channels as usize;
    let mut sample_offset = 0;
    let mut pts: i64 = 0;

    while sample_offset < samples.len() {
        // Skip av_write_trailer on cancel: muxers seek-back-patch trailer
        // data (FLAC STREAMINFO, MP3 Xing) from frame state populated only
        // by completed frames, so invoking it on a partial stream is
        // unsafe. The output is discarded anyway. avformat_free_context
        // frees the AVIO since AVFMT_FLAG_CUSTOM_IO is set.
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            av_packet_free(&mut (packet as *mut _));
            av_frame_free(&mut (frame as *mut _));
            avformat_free_context(fmt_ctx);
            avcodec_free_context(&mut (codec_ctx as *mut _));
            return Err("encoding cancelled".to_string());
        }

        let remaining = samples.len() - sample_offset;
        let chunk_samples = remaining.min(samples_per_frame);
        let chunk_frames = chunk_samples / channels as usize;

        (*frame).nb_samples = chunk_frames as c_int;

        // Allocate frame buffer
        let ret = av_frame_get_buffer(frame, 0);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!(
                "Failed to allocate frame buffer: {}",
                av_err_str(ret)
            ));
        }

        // Make frame writable
        let ret = av_frame_make_writable(frame);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!(
                "Failed to make frame writable: {}",
                av_err_str(ret)
            ));
        }

        // Copy samples to frame (interleaved format)
        // For 24-bit, left-shift by 8 to fill S32 (matches FFmpeg's internal format)
        let frame_data = (*frame).data[0];
        match bits_per_sample {
            16 => {
                let dst = frame_data as *mut i16;
                for i in 0..chunk_samples {
                    *dst.add(i) = samples[sample_offset + i] as i16;
                }
            }
            24 => {
                // 24-bit uses S32 container, values left-shifted by 8
                let dst = frame_data as *mut i32;
                for i in 0..chunk_samples {
                    *dst.add(i) = samples[sample_offset + i] << 8;
                }
            }
            32 => {
                let dst = frame_data as *mut i32;
                for i in 0..chunk_samples {
                    *dst.add(i) = samples[sample_offset + i];
                }
            }
            _ => {
                let dst = frame_data as *mut i16;
                for i in 0..chunk_samples {
                    *dst.add(i) = samples[sample_offset + i] as i16;
                }
            }
        }

        (*frame).pts = pts;
        pts += chunk_frames as i64;

        // Send frame to encoder
        let ret = avcodec_send_frame(codec_ctx, frame);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!("Failed to send frame: {}", av_err_str(ret)));
        }

        // Receive and write packets
        loop {
            let ret = avcodec_receive_packet(codec_ctx, packet);
            if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
                break;
            }
            if ret < 0 {
                free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
                return Err(format!("Failed to receive packet: {}", av_err_str(ret)));
            }

            (*packet).stream_index = 0;
            let ret = av_interleaved_write_frame(fmt_ctx, packet);
            if ret < 0 {
                free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
                return Err(format!("Failed to write packet: {}", av_err_str(ret)));
            }
        }

        sample_offset += chunk_samples;
    }

    // Flush encoder
    avcodec_send_frame(codec_ctx, ptr::null());
    loop {
        let ret = avcodec_receive_packet(codec_ctx, packet);
        if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
            break;
        }
        if ret < 0 {
            break;
        }
        (*packet).stream_index = 0;
        av_interleaved_write_frame(fmt_ctx, packet);
    }

    // Write trailer
    av_write_trailer(fmt_ctx);

    // Flush AVIO buffer
    avio_flush(avio);

    // Cleanup (don't free avio - avformat_free_context handles it when CUSTOM_IO is set)
    av_packet_free(&mut (packet as *mut _));
    av_frame_free(&mut (frame as *mut _));
    avcodec_free_context(&mut (codec_ctx as *mut _));

    // Get the data before freeing format context
    let result = write_ctx.data[..write_ctx.pos].to_vec();

    // Free format context (this also frees avio since we set CUSTOM_IO flag)
    avformat_free_context(fmt_ctx);

    debug!("Encoded {} bytes of FLAC data", result.len());

    Ok(result)
}

/// Encode PCM samples to MP3 format (320kbps CBR via libmp3lame).
///
/// Takes interleaved i32 samples and returns the encoded MP3 data as bytes.
/// Uses FFmpeg library with custom AVIO for in-memory encoding.
/// Input samples are treated as 16-bit regardless of bits_per_sample (MP3 is lossy).
pub fn encode_to_mp3(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    bitrate: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    unsafe {
        encode_to_mp3_avio(
            samples,
            sample_rate,
            channels,
            bits_per_sample,
            bitrate,
            cancel,
        )
    }
}

/// Internal AVIO-based MP3 encoding implementation
unsafe fn encode_to_mp3_avio(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
    bitrate: u32,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<u8>, String> {
    use ffmpeg_sys_next::*;

    // Create write context
    let mut write_ctx = Box::new(WriteAvioContext {
        data: Vec::with_capacity(samples.len()), // Rough estimate
        pos: 0,
    });

    // Allocate AVIO buffer
    let avio_buffer_size = AVIO_BUFFER_SIZE;
    let avio_buffer = av_malloc(avio_buffer_size) as *mut u8;
    if avio_buffer.is_null() {
        return Err("Failed to allocate AVIO buffer".to_string());
    }

    // Create custom AVIO context for writing
    let avio = avio_alloc_context(
        avio_buffer,
        avio_buffer_size as c_int,
        1, // write flag
        write_ctx.as_mut() as *mut WriteAvioContext as *mut c_void,
        None, // no read
        Some(avio_write_callback),
        Some(avio_write_seek_callback),
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        return Err("Failed to create AVIO context".to_string());
    }

    // Find MP3 encoder (libmp3lame)
    let codec = avcodec_find_encoder(AVCodecID::AV_CODEC_ID_MP3);
    if codec.is_null() {
        avio_context_free(&mut (avio as *mut _));
        return Err("MP3 encoder not found (libmp3lame)".to_string());
    }

    // Allocate codec context
    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        avio_context_free(&mut (avio as *mut _));
        return Err("Failed to allocate codec context".to_string());
    }

    // Configure encoder
    (*codec_ctx).sample_rate = sample_rate as c_int;
    (*codec_ctx).time_base = AVRational {
        num: 1,
        den: sample_rate as c_int,
    };
    (*codec_ctx).bit_rate = bitrate as i64;

    // libmp3lame requires S16P (planar signed 16-bit) or FLTP (planar float).
    // S16P is simplest since our input is integer samples.
    (*codec_ctx).sample_fmt = AVSampleFormat::AV_SAMPLE_FMT_S16P;

    // Set channel layout
    let mut ch_layout: AVChannelLayout = std::mem::zeroed();
    av_channel_layout_default(&mut ch_layout, channels as c_int);
    (*codec_ctx).ch_layout = ch_layout;

    // Open encoder
    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avio_context_free(&mut (avio as *mut _));
        return Err(format!("Failed to open MP3 encoder: {}", av_err_str(ret)));
    }

    // Create output format context
    let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
    let ret =
        avformat_alloc_output_context2(&mut fmt_ctx, ptr::null(), c"mp3".as_ptr(), ptr::null());
    if ret < 0 || fmt_ctx.is_null() {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avio_context_free(&mut (avio as *mut _));
        return Err("Failed to create output context".to_string());
    }

    // Use our custom AVIO
    (*fmt_ctx).pb = avio;
    (*fmt_ctx).flags |= AVFMT_FLAG_CUSTOM_IO as c_int;

    // Add audio stream
    let stream = avformat_new_stream(fmt_ctx, ptr::null());
    if stream.is_null() {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to create stream".to_string());
    }

    // Copy codec parameters to stream
    let ret = avcodec_parameters_from_context((*stream).codecpar, codec_ctx);
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(format!("Failed to copy codec params: {}", av_err_str(ret)));
    }

    // Write header
    let ret = avformat_write_header(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(format!("Failed to write header: {}", av_err_str(ret)));
    }

    // Allocate frame
    let frame = av_frame_alloc();
    if frame.is_null() {
        av_write_trailer(fmt_ctx);
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to allocate frame".to_string());
    }

    (*frame).format = (*codec_ctx).sample_fmt as c_int;
    (*frame).ch_layout = (*codec_ctx).ch_layout;
    (*frame).sample_rate = sample_rate as c_int;

    // Allocate packet
    let packet = av_packet_alloc();
    if packet.is_null() {
        av_frame_free(&mut (frame as *mut _));
        av_write_trailer(fmt_ctx);
        avformat_free_context(fmt_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err("Failed to allocate packet".to_string());
    }

    // Process samples in chunks matching encoder's frame size
    let frame_size = if (*codec_ctx).frame_size > 0 {
        (*codec_ctx).frame_size as usize
    } else {
        1152 // Standard MP3 frame size
    };

    let samples_per_frame = frame_size * channels as usize;
    let mut sample_offset = 0;
    let mut pts: i64 = 0;

    while sample_offset < samples.len() {
        // Skip av_write_trailer on cancel: muxers seek-back-patch trailer
        // data (FLAC STREAMINFO, MP3 Xing) from frame state populated only
        // by completed frames, so invoking it on a partial stream is
        // unsafe. The output is discarded anyway. avformat_free_context
        // frees the AVIO since AVFMT_FLAG_CUSTOM_IO is set.
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            av_packet_free(&mut (packet as *mut _));
            av_frame_free(&mut (frame as *mut _));
            avformat_free_context(fmt_ctx);
            avcodec_free_context(&mut (codec_ctx as *mut _));
            return Err("encoding cancelled".to_string());
        }

        let remaining = samples.len() - sample_offset;
        let chunk_samples = remaining.min(samples_per_frame);
        let chunk_frames = chunk_samples / channels as usize;

        (*frame).nb_samples = chunk_frames as c_int;

        // Allocate frame buffer
        let ret = av_frame_get_buffer(frame, 0);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!(
                "Failed to allocate frame buffer: {}",
                av_err_str(ret)
            ));
        }

        // Make frame writable
        let ret = av_frame_make_writable(frame);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!(
                "Failed to make frame writable: {}",
                av_err_str(ret)
            ));
        }

        // Copy samples to frame (planar format: deinterleave into per-channel planes)
        // Samples are i32 at native bit depth — shift down to 16-bit for MP3
        let shift = match bits_per_sample {
            16 => 0,
            24 => 8,
            32 => 16,
            _ => (bits_per_sample as i32 - 16).max(0) as u32,
        };

        for ch in 0..channels as usize {
            let dst = (*frame).data[ch] as *mut i16;
            for i in 0..chunk_frames {
                let src_idx = sample_offset + i * channels as usize + ch;
                if src_idx < samples.len() {
                    *dst.add(i) = (samples[src_idx] >> shift) as i16;
                }
            }
        }

        (*frame).pts = pts;
        pts += chunk_frames as i64;

        // Send frame to encoder
        let ret = avcodec_send_frame(codec_ctx, frame);
        if ret < 0 {
            free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
            return Err(format!("Failed to send frame: {}", av_err_str(ret)));
        }

        // Receive and write packets
        loop {
            let ret = avcodec_receive_packet(codec_ctx, packet);
            if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
                break;
            }
            if ret < 0 {
                free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
                return Err(format!("Failed to receive packet: {}", av_err_str(ret)));
            }

            (*packet).stream_index = 0;
            let ret = av_interleaved_write_frame(fmt_ctx, packet);
            if ret < 0 {
                free_encode_resources!(packet, frame, fmt_ctx, codec_ctx);
                return Err(format!("Failed to write packet: {}", av_err_str(ret)));
            }
        }

        sample_offset += chunk_samples;
    }

    // Flush encoder
    avcodec_send_frame(codec_ctx, ptr::null());
    loop {
        let ret = avcodec_receive_packet(codec_ctx, packet);
        if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
            break;
        }
        if ret < 0 {
            break;
        }
        (*packet).stream_index = 0;
        av_interleaved_write_frame(fmt_ctx, packet);
    }

    // Write trailer
    av_write_trailer(fmt_ctx);

    // Flush AVIO buffer
    avio_flush(avio);

    // Cleanup (don't free avio - avformat_free_context handles it when CUSTOM_IO is set)
    av_packet_free(&mut (packet as *mut _));
    av_frame_free(&mut (frame as *mut _));
    avcodec_free_context(&mut (codec_ctx as *mut _));

    // Get the data before freeing format context
    let result = write_ctx.data[..write_ctx.pos].to_vec();

    // Free format context (this also frees avio since we set CUSTOM_IO flag)
    avformat_free_context(fmt_ctx);

    debug!("Encoded {} bytes of MP3 data", result.len());

    Ok(result)
}
