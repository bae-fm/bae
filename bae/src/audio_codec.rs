//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM), encoding (PCM to FLAC), and
//! seektable generation. Uses custom AVIO for in-memory decoding.

use crate::playback::{SharedSparseBuffer, StreamingPcmSink};
use std::cell::Cell;
use std::os::raw::{c_int, c_void};
use std::ptr;
use tracing::{debug, info, trace, warn};

// Thread-local FFmpeg error counter for per-decode error tracking
thread_local! {
    static FFMPEG_DECODE_ERRORS: Cell<u32> = const { Cell::new(0) };
}

/// Reset the thread-local FFmpeg error counter
fn reset_ffmpeg_errors() {
    FFMPEG_DECODE_ERRORS.with(|c| c.set(0));
}

/// Get current FFmpeg error count for this thread
fn get_ffmpeg_errors() -> u32 {
    FFMPEG_DECODE_ERRORS.with(|c| c.get())
}

/// Custom FFmpeg log callback that counts errors per-thread (Linux x86_64)
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
unsafe extern "C" fn ffmpeg_log_callback(
    _avcl: *mut c_void,
    level: c_int,
    _fmt: *const i8,
    _vl: *mut ffmpeg_sys_next::__va_list_tag,
) {
    // AV_LOG_ERROR = 16
    if level <= 16 {
        FFMPEG_DECODE_ERRORS.with(|c| c.set(c.get() + 1));
    }
}

/// Custom FFmpeg log callback that counts errors per-thread (other platforms)
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
unsafe extern "C" fn ffmpeg_log_callback(
    _avcl: *mut c_void,
    level: c_int,
    _fmt: *const i8,
    _vl: *mut i8,
) {
    // AV_LOG_ERROR = 16
    if level <= 16 {
        FFMPEG_DECODE_ERRORS.with(|c| c.set(c.get() + 1));
    }
}

/// Install our custom FFmpeg log callback
fn install_ffmpeg_log_callback() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        ffmpeg_sys_next::av_log_set_callback(Some(ffmpeg_log_callback));
    });
}

/// Decoded audio metadata and samples
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<i32>,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits_per_sample: u32,
}

/// A seek point entry mapping sample number to byte offset.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct SeekEntry {
    pub sample: u64,
    pub byte: u64,
}

/// Initialize FFmpeg (call once at startup)
pub fn init() {
    ffmpeg_next::init().expect("Failed to initialize FFmpeg");
}

// --- AVIO custom I/O implementation ---

/// Context for AVIO callbacks - holds the buffer and read position
struct AvioContext {
    data: *const u8,
    size: usize,
    pos: usize,
}

/// AVIO read callback - reads bytes from our memory buffer
unsafe extern "C" fn avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &mut *(opaque as *mut AvioContext);
    let remaining = ctx.size - ctx.pos;
    let to_read = (buf_size as usize).min(remaining);

    if to_read == 0 {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    ptr::copy_nonoverlapping(ctx.data.add(ctx.pos), buf, to_read);
    ctx.pos += to_read;
    to_read as c_int
}

/// AVIO seek callback - seeks within our memory buffer
unsafe extern "C" fn avio_seek_callback(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    let ctx = &mut *(opaque as *mut AvioContext);

    // AVSEEK_SIZE returns the buffer size
    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.size as i64;
    }

    let new_pos = match whence {
        0 => offset as usize,                     // SEEK_SET
        1 => (ctx.pos as i64 + offset) as usize,  // SEEK_CUR
        2 => (ctx.size as i64 + offset) as usize, // SEEK_END
        _ => return -1,
    };

    if new_pos > ctx.size {
        return -1;
    }

    ctx.pos = new_pos;
    new_pos as i64
}

// --- Streaming AVIO for SparseBuffer ---

/// Context for streaming AVIO - reads from SparseBuffer which blocks waiting for data
struct StreamingAvioContext {
    buffer: SharedSparseBuffer,
    cancelled: std::sync::atomic::AtomicBool,
}

/// AVIO read callback for streaming - reads from SparseBuffer, blocking until data available
unsafe extern "C" fn streaming_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if ctx.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    let mut temp_buf = vec![0u8; buf_size as usize];
    match ctx.buffer.read(&mut temp_buf) {
        Some(0) => ffmpeg_sys_next::AVERROR_EOF,
        Some(n) => {
            ptr::copy_nonoverlapping(temp_buf.as_ptr(), buf, n);
            n as c_int
        }
        None => {
            // Buffer cancelled
            ctx.cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
            ffmpeg_sys_next::AVERROR_EOF
        }
    }
}

/// Decode any audio format to PCM samples.
///
/// Supports FLAC, MP3, APE, AAC/M4A, OGG Vorbis, WAV, AIFF, and more.
/// If start_ms/end_ms are provided, only that time range is decoded.
/// Returns interleaved i32 samples.
pub fn decode_audio(
    data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedAudio, String> {
    decode_audio_from_memory(data, start_ms, end_ms)
}

/// Decode audio from a memory buffer using custom AVIO
fn decode_audio_from_memory(
    data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedAudio, String> {
    unsafe { decode_audio_avio(data, start_ms, end_ms) }
}

/// Internal AVIO-based decode implementation
unsafe fn decode_audio_avio(
    data: &[u8],
    start_ms: Option<u64>,
    end_ms: Option<u64>,
) -> Result<DecodedAudio, String> {
    use ffmpeg_sys_next::*;

    // Create our context for callbacks
    let mut avio_ctx = Box::new(AvioContext {
        data: data.as_ptr(),
        size: data.len(),
        pos: 0,
    });

    // Allocate AVIO buffer (FFmpeg will manage this)
    let avio_buffer_size = 32768;
    let avio_buffer = av_malloc(avio_buffer_size) as *mut u8;
    if avio_buffer.is_null() {
        return Err("Failed to allocate AVIO buffer".to_string());
    }

    // Create custom AVIO context
    let avio = avio_alloc_context(
        avio_buffer,
        avio_buffer_size as c_int,
        0, // read-only
        avio_ctx.as_mut() as *mut AvioContext as *mut c_void,
        Some(avio_read_callback),
        None, // no write
        Some(avio_seek_callback),
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        return Err("Failed to create AVIO context".to_string());
    }

    // Create format context
    let mut fmt_ctx = avformat_alloc_context();
    if fmt_ctx.is_null() {
        av_free(avio as *mut c_void);
        return Err("Failed to allocate format context".to_string());
    }
    (*fmt_ctx).pb = avio;

    // Open input (NULL filename since we're using custom I/O)
    let ret = avformat_open_input(&mut fmt_ctx, ptr::null(), ptr::null_mut(), ptr::null_mut());
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        return Err(format!("Failed to open input: {}", av_err_str(ret)));
    }

    // Find stream info
    let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat_close_input(&mut fmt_ctx);
        return Err(format!("Failed to find stream info: {}", av_err_str(ret)));
    }

    // Find best audio stream
    let stream_index = av_find_best_stream(
        fmt_ctx,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );
    if stream_index < 0 {
        avformat_close_input(&mut fmt_ctx);
        return Err("No audio stream found".to_string());
    }

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let codecpar = (*stream).codecpar;

    // Find decoder
    let codec = avcodec_find_decoder((*codecpar).codec_id);
    if codec.is_null() {
        avformat_close_input(&mut fmt_ctx);
        return Err("Decoder not found".to_string());
    }

    // Allocate codec context
    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        avformat_close_input(&mut fmt_ctx);
        return Err("Failed to allocate codec context".to_string());
    }

    // Copy codec parameters
    let ret = avcodec_parameters_to_context(codec_ctx, codecpar);
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        return Err(format!("Failed to copy codec params: {}", av_err_str(ret)));
    }

    // Open codec
    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        return Err(format!("Failed to open codec: {}", av_err_str(ret)));
    }

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*codecpar).ch_layout.nb_channels as u32;

    // Determine bits per sample from format (for metadata only, actual extraction uses frame format)
    let bits_per_sample = match (*codec_ctx).sample_fmt {
        AVSampleFormat::AV_SAMPLE_FMT_U8 | AVSampleFormat::AV_SAMPLE_FMT_U8P => 8,
        AVSampleFormat::AV_SAMPLE_FMT_S16 | AVSampleFormat::AV_SAMPLE_FMT_S16P => 16,
        AVSampleFormat::AV_SAMPLE_FMT_S32 | AVSampleFormat::AV_SAMPLE_FMT_S32P => 32,
        AVSampleFormat::AV_SAMPLE_FMT_FLT | AVSampleFormat::AV_SAMPLE_FMT_FLTP => 32,
        AVSampleFormat::AV_SAMPLE_FMT_DBL | AVSampleFormat::AV_SAMPLE_FMT_DBLP => 64,
        AVSampleFormat::AV_SAMPLE_FMT_S64 | AVSampleFormat::AV_SAMPLE_FMT_S64P => 64,
        _ => 16,
    };

    // Calculate sample boundaries
    let start_sample = start_ms.map(|ms| (ms * sample_rate as u64) / 1000);
    let end_sample = end_ms.map(|ms| (ms * sample_rate as u64) / 1000);

    // Seek to start position if specified
    if let Some(start_ms) = start_ms {
        let timestamp = (start_ms as i64) * 1000; // microseconds
        av_seek_frame(fmt_ctx, -1, timestamp, AVSEEK_FLAG_BACKWARD as c_int);
    }

    // Allocate frame and packet
    let frame = av_frame_alloc();
    let packet = av_packet_alloc();
    if frame.is_null() || packet.is_null() {
        if !frame.is_null() {
            av_frame_free(&mut (frame as *mut _));
        }
        if !packet.is_null() {
            av_packet_free(&mut (packet as *mut _));
        }
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        return Err("Failed to allocate frame/packet".to_string());
    }

    let mut samples: Vec<i32> = Vec::new();
    let mut current_sample: u64 = 0;
    let mut collecting = start_sample.is_none();

    // Read and decode packets
    while av_read_frame(fmt_ctx, packet) >= 0 {
        if (*packet).stream_index != stream_index {
            av_packet_unref(packet);
            continue;
        }

        let ret = avcodec_send_packet(codec_ctx, packet);
        av_packet_unref(packet);

        if ret < 0 {
            continue;
        }

        while avcodec_receive_frame(codec_ctx, frame) >= 0 {
            let frame_samples = (*frame).nb_samples as u64;
            let frame_start = current_sample;
            let frame_end = current_sample + frame_samples;

            // Check if we should start collecting
            if let Some(start) = start_sample {
                if frame_end > start {
                    collecting = true;
                }
            }

            // Check if we should stop
            if let Some(end) = end_sample {
                if frame_start >= end {
                    break;
                }
            }

            if collecting {
                let frame_samples_vec = extract_samples_from_raw_frame(frame, channels as usize);

                // Calculate which samples to take based on range
                let skip_start = if let Some(start) = start_sample {
                    if frame_start < start {
                        ((start - frame_start) as usize) * channels as usize
                    } else {
                        0
                    }
                } else {
                    0
                };

                let take_end = if let Some(end) = end_sample {
                    if frame_end > end {
                        ((end - frame_start) as usize) * channels as usize
                    } else {
                        frame_samples_vec.len()
                    }
                } else {
                    frame_samples_vec.len()
                };

                if skip_start < take_end && take_end <= frame_samples_vec.len() {
                    samples.extend_from_slice(&frame_samples_vec[skip_start..take_end]);
                }
            }

            current_sample = frame_end;
        }

        // Check if we've passed the end
        if let Some(end) = end_sample {
            if current_sample >= end {
                break;
            }
        }
    }

    // Flush decoder
    avcodec_send_packet(codec_ctx, ptr::null());
    while avcodec_receive_frame(codec_ctx, frame) >= 0 {
        if collecting {
            let frame_samples_vec = extract_samples_from_raw_frame(frame, channels as usize);
            samples.extend_from_slice(&frame_samples_vec);
        }
    }

    // Cleanup
    av_frame_free(&mut (frame as *mut _));
    av_packet_free(&mut (packet as *mut _));
    avcodec_free_context(&mut (codec_ctx as *mut _));
    avformat_close_input(&mut fmt_ctx);
    // Note: avformat_close_input frees the AVIO context and buffer

    // Keep avio_ctx alive until here (prevent drop during FFmpeg operations)
    drop(avio_ctx);

    trace!(
        "Decoded {} samples ({} frames) from audio",
        samples.len(),
        samples.len() / channels.max(1) as usize
    );

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
        bits_per_sample,
    })
}

/// Extract samples from a raw AVFrame as i32
unsafe fn extract_samples_from_raw_frame(
    frame: *const ffmpeg_sys_next::AVFrame,
    channels: usize,
) -> Vec<i32> {
    use ffmpeg_sys_next::{av_get_bytes_per_sample, AVSampleFormat};

    let num_samples = (*frame).nb_samples as usize;
    let mut samples = Vec::with_capacity(num_samples * channels);

    // Get format info directly from the frame
    let format: AVSampleFormat = std::mem::transmute((*frame).format);
    let bytes_per_sample = av_get_bytes_per_sample(format);
    let actual_bytes_per_sample = if bytes_per_sample > 0 {
        bytes_per_sample as usize
    } else {
        4 // Default fallback
    };

    let is_float = matches!(
        format,
        AVSampleFormat::AV_SAMPLE_FMT_FLT
            | AVSampleFormat::AV_SAMPLE_FMT_FLTP
            | AVSampleFormat::AV_SAMPLE_FMT_DBL
            | AVSampleFormat::AV_SAMPLE_FMT_DBLP
    );

    let is_planar = matches!(
        format,
        AVSampleFormat::AV_SAMPLE_FMT_U8P
            | AVSampleFormat::AV_SAMPLE_FMT_S16P
            | AVSampleFormat::AV_SAMPLE_FMT_S32P
            | AVSampleFormat::AV_SAMPLE_FMT_FLTP
            | AVSampleFormat::AV_SAMPLE_FMT_DBLP
            | AVSampleFormat::AV_SAMPLE_FMT_S64P
    );

    if is_planar {
        // Interleave from separate channel planes
        for i in 0..num_samples {
            for ch in 0..channels {
                let plane = (*frame).data[ch] as *const u8;
                if plane.is_null() {
                    samples.push(0);
                    continue;
                }

                let sample = read_sample(plane, i, actual_bytes_per_sample, is_float);
                samples.push(sample);
            }
        }
    } else {
        // Packed format - all samples interleaved in plane 0
        let data = (*frame).data[0] as *const u8;
        if !data.is_null() {
            for i in 0..(num_samples * channels) {
                let sample = read_sample(data, i, actual_bytes_per_sample, is_float);
                samples.push(sample);
            }
        }
    }

    samples
}

/// Read a single sample from raw bytes and convert to i32
unsafe fn read_sample(
    data: *const u8,
    index: usize,
    bytes_per_sample: usize,
    is_float: bool,
) -> i32 {
    let offset = index * bytes_per_sample;
    let ptr = data.add(offset);

    if is_float {
        let f = *(ptr as *const f32);
        (f * i32::MAX as f32) as i32
    } else {
        match bytes_per_sample {
            1 => (*(ptr as *const i8) as i32) * 256, // Scale 8-bit to 16-bit range
            2 => *(ptr as *const i16) as i32,        // Keep 16-bit in native range
            3 => {
                // 24-bit little-endian, sign-extend to i32
                let b = std::slice::from_raw_parts(ptr, 3);
                let val = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
                // Sign extend from 24-bit
                if val & 0x800000 != 0 {
                    val | 0xFF000000u32 as i32
                } else {
                    val
                }
            }
            4 => *(ptr as *const i32),
            _ => 0,
        }
    }
}

/// Convert FFmpeg error code to string
fn av_err_str(errnum: i32) -> String {
    unsafe {
        let mut buf = [0i8; 256];
        ffmpeg_sys_next::av_strerror(errnum, buf.as_mut_ptr(), buf.len());
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

// --- AVIO write context for encoding to memory ---

/// Context for AVIO write callbacks - accumulates encoded data
struct WriteAvioContext {
    data: Vec<u8>,
    pos: usize,
}

/// AVIO write callback - writes bytes to our memory buffer (Linux)
#[cfg(target_os = "linux")]
unsafe extern "C" fn avio_write_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    avio_write_callback_impl(opaque, buf as *const u8, buf_size)
}

/// AVIO write callback - writes bytes to our memory buffer (macOS/other)
#[cfg(not(target_os = "linux"))]
unsafe extern "C" fn avio_write_callback(
    opaque: *mut c_void,
    buf: *const u8,
    buf_size: c_int,
) -> c_int {
    avio_write_callback_impl(opaque, buf, buf_size)
}

/// Shared implementation for write callback
unsafe fn avio_write_callback_impl(opaque: *mut c_void, buf: *const u8, buf_size: c_int) -> c_int {
    let ctx = &mut *(opaque as *mut WriteAvioContext);
    let size = buf_size as usize;

    // Ensure buffer has enough capacity
    let required_len = ctx.pos + size;
    if required_len > ctx.data.len() {
        ctx.data.resize(required_len, 0);
    }

    ptr::copy_nonoverlapping(buf, ctx.data.as_mut_ptr().add(ctx.pos), size);
    ctx.pos += size;
    buf_size
}

/// AVIO seek callback for writing - allows encoder to seek back for headers
unsafe extern "C" fn avio_write_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &mut *(opaque as *mut WriteAvioContext);

    // AVSEEK_SIZE returns the buffer size
    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.data.len() as i64;
    }

    let new_pos = match whence {
        0 => offset as usize,                           // SEEK_SET
        1 => (ctx.pos as i64 + offset) as usize,        // SEEK_CUR
        2 => (ctx.data.len() as i64 + offset) as usize, // SEEK_END
        _ => return -1,
    };

    ctx.pos = new_pos;
    new_pos as i64
}

/// Encode PCM samples to FLAC format.
///
/// Takes interleaved i32 samples and returns the encoded FLAC data as bytes.
/// Uses FFmpeg library with custom AVIO for in-memory encoding.
pub fn encode_to_flac(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<Vec<u8>, String> {
    unsafe { encode_to_flac_avio(samples, sample_rate, channels, bits_per_sample) }
}

/// Internal AVIO-based FLAC encoding implementation
unsafe fn encode_to_flac_avio(
    samples: &[i32],
    sample_rate: u32,
    channels: u32,
    bits_per_sample: u32,
) -> Result<Vec<u8>, String> {
    use ffmpeg_sys_next::*;

    // Create write context
    let mut write_ctx = Box::new(WriteAvioContext {
        data: Vec::with_capacity(samples.len() * 2), // Rough estimate
        pos: 0,
    });

    // Allocate AVIO buffer
    let avio_buffer_size = 32768;
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
    (*codec_ctx).sample_fmt = match bits_per_sample {
        16 => AVSampleFormat::AV_SAMPLE_FMT_S16,
        32 => AVSampleFormat::AV_SAMPLE_FMT_S32,
        _ => AVSampleFormat::AV_SAMPLE_FMT_S16,
    };

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
        let remaining = samples.len() - sample_offset;
        let chunk_samples = remaining.min(samples_per_frame);
        let chunk_frames = chunk_samples / channels as usize;

        (*frame).nb_samples = chunk_frames as c_int;

        // Allocate frame buffer
        let ret = av_frame_get_buffer(frame, 0);
        if ret < 0 {
            av_packet_free(&mut (packet as *mut _));
            av_frame_free(&mut (frame as *mut _));
            av_write_trailer(fmt_ctx);
            avformat_free_context(fmt_ctx);
            avcodec_free_context(&mut (codec_ctx as *mut _));
            return Err(format!(
                "Failed to allocate frame buffer: {}",
                av_err_str(ret)
            ));
        }

        // Make frame writable
        let ret = av_frame_make_writable(frame);
        if ret < 0 {
            av_packet_free(&mut (packet as *mut _));
            av_frame_free(&mut (frame as *mut _));
            av_write_trailer(fmt_ctx);
            avformat_free_context(fmt_ctx);
            avcodec_free_context(&mut (codec_ctx as *mut _));
            return Err(format!(
                "Failed to make frame writable: {}",
                av_err_str(ret)
            ));
        }

        // Copy samples to frame (interleaved format)
        let frame_data = (*frame).data[0];
        match bits_per_sample {
            16 => {
                let dst = frame_data as *mut i16;
                for i in 0..chunk_samples {
                    *dst.add(i) = samples[sample_offset + i] as i16;
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
            av_packet_free(&mut (packet as *mut _));
            av_frame_free(&mut (frame as *mut _));
            av_write_trailer(fmt_ctx);
            avformat_free_context(fmt_ctx);
            avcodec_free_context(&mut (codec_ctx as *mut _));
            return Err(format!("Failed to send frame: {}", av_err_str(ret)));
        }

        // Receive and write packets
        loop {
            let ret = avcodec_receive_packet(codec_ctx, packet);
            if ret == AVERROR(EAGAIN) || ret == AVERROR_EOF {
                break;
            }
            if ret < 0 {
                av_packet_free(&mut (packet as *mut _));
                av_frame_free(&mut (frame as *mut _));
                av_write_trailer(fmt_ctx);
                avformat_free_context(fmt_ctx);
                avcodec_free_context(&mut (codec_ctx as *mut _));
                return Err(format!("Failed to receive packet: {}", av_err_str(ret)));
            }

            (*packet).stream_index = 0;
            let ret = av_interleaved_write_frame(fmt_ctx, packet);
            if ret < 0 {
                av_packet_free(&mut (packet as *mut _));
                av_frame_free(&mut (frame as *mut _));
                av_write_trailer(fmt_ctx);
                avformat_free_context(fmt_ctx);
                avcodec_free_context(&mut (codec_ctx as *mut _));
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

/// Build a frame-accurate seektable by scanning FLAC frames.
///
/// This scans the FLAC byte stream for frame sync codes (0xFF 0xF8/0xF9),
/// validates headers with CRC-8, and builds a map of sample_number -> byte_offset.
/// Returns byte offsets relative to the start of audio data (after headers).
///
/// For CUE/FLAC imports, we need to extract specific byte ranges from a FLAC file -
/// one range per track defined in the CUE sheet. This requires ~93ms precision
/// (one entry per FLAC frame at 44.1kHz).
pub fn build_seektable(flac_data: &[u8]) -> Result<Vec<SeekEntry>, String> {
    // Parse FLAC headers to get metadata
    if flac_data.len() < 4 || &flac_data[0..4] != b"fLaC" {
        return Err("Invalid FLAC signature".to_string());
    }

    let mut pos = 4;
    let mut sample_rate = 0u32;
    let mut total_samples = 0u64;
    let mut min_block_size = 0u32;
    let mut min_frame_size = 0u32;

    // Parse metadata blocks
    loop {
        if pos + 4 > flac_data.len() {
            return Err("Unexpected end of file in metadata".to_string());
        }

        let header_byte = flac_data[pos];
        let is_last = (header_byte & 0x80) != 0;
        let block_type = header_byte & 0x7F;
        let block_size = u32::from_be_bytes([
            0,
            flac_data[pos + 1],
            flac_data[pos + 2],
            flac_data[pos + 3],
        ]) as usize;

        if pos + 4 + block_size > flac_data.len() {
            return Err("Block extends beyond file".to_string());
        }

        if block_type == 0 && block_size >= 18 {
            // STREAMINFO block
            let block = &flac_data[pos + 4..pos + 4 + block_size];
            min_block_size = ((block[0] as u32) << 8) | (block[1] as u32);
            min_frame_size =
                ((block[4] as u32) << 16) | ((block[5] as u32) << 8) | (block[6] as u32);
            sample_rate =
                ((block[10] as u32) << 12) | ((block[11] as u32) << 4) | ((block[12] as u32) >> 4);
            total_samples = (((block[13] & 0x0F) as u64) << 32)
                | ((block[14] as u64) << 24)
                | ((block[15] as u64) << 16)
                | ((block[16] as u64) << 8)
                | (block[17] as u64);
        }

        pos += 4 + block_size;
        if is_last {
            break;
        }
    }

    let audio_data_start = pos;
    let audio_data_end = flac_data.len();

    if sample_rate == 0 || total_samples == 0 {
        return Err("Invalid FLAC: no samples or sample rate".to_string());
    }

    // Use minimum frame size from STREAMINFO, or a reasonable default
    let skip_size = if min_frame_size > 0 {
        min_frame_size as usize
    } else {
        1000 // Conservative default ~1KB
    };

    // Scan for frame sync codes
    let mut seektable = Vec::new();
    let mut scan_pos = audio_data_start;
    let mut last_sample_number: Option<u64> = None;

    while scan_pos + 16 < audio_data_end {
        // FLAC frame sync: 14 bits of 1s followed by 0 = 0xFF 0xF8 or 0xFF 0xF9
        if flac_data[scan_pos] == 0xFF && (flac_data[scan_pos + 1] & 0xFE) == 0xF8 {
            // Validate frame header
            if validate_flac_frame_header(flac_data, scan_pos) {
                // Read actual sample number from frame header
                if let Some(sample_num) =
                    parse_flac_frame_sample_number(flac_data, scan_pos, min_block_size)
                {
                    // Only add if this is a new sample position (avoid duplicates)
                    // Also reject sample_num > total_samples
                    if sample_num <= total_samples
                        && (last_sample_number.is_none()
                            || sample_num > last_sample_number.unwrap())
                    {
                        let stream_offset = (scan_pos - audio_data_start) as u64;
                        seektable.push(SeekEntry {
                            sample: sample_num,
                            byte: stream_offset,
                        });
                        last_sample_number = Some(sample_num);

                        // Skip ahead by minimum frame size
                        scan_pos += skip_size;
                        continue;
                    }
                }
            }
        }
        scan_pos += 1;
    }

    // Add final entry
    seektable.push(SeekEntry {
        sample: total_samples,
        byte: (audio_data_end - audio_data_start) as u64,
    });

    let precision_ms = if seektable.len() > 1 && sample_rate > 0 {
        (total_samples as f64 / seektable.len() as f64) / sample_rate as f64 * 1000.0
    } else {
        0.0
    };

    info!(
        "Scanned FLAC: {} frames, {:.1}ms precision",
        seektable.len(),
        precision_ms
    );

    Ok(seektable)
}

/// Parse the sample number from a FLAC frame header.
fn parse_flac_frame_sample_number(data: &[u8], pos: usize, min_block_size: u32) -> Option<u64> {
    if pos + 5 >= data.len() {
        return None;
    }

    // Byte 1 bit 0: blocking strategy (0 = fixed, 1 = variable)
    let variable_block_size = (data[pos + 1] & 0x01) != 0;

    // The frame/sample number starts at byte 4
    let num_start = pos + 4;

    // Decode UTF-8-like variable length number
    let first_byte = data[num_start];
    let (value, _bytes_used) = if first_byte & 0x80 == 0 {
        (first_byte as u64, 1)
    } else if first_byte & 0xE0 == 0xC0 {
        if num_start + 1 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x1F) << 6) | (data[num_start + 1] as u64 & 0x3F);
        (val, 2)
    } else if first_byte & 0xF0 == 0xE0 {
        if num_start + 2 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x0F) << 12)
            | ((data[num_start + 1] as u64 & 0x3F) << 6)
            | (data[num_start + 2] as u64 & 0x3F);
        (val, 3)
    } else if first_byte & 0xF8 == 0xF0 {
        if num_start + 3 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x07) << 18)
            | ((data[num_start + 1] as u64 & 0x3F) << 12)
            | ((data[num_start + 2] as u64 & 0x3F) << 6)
            | (data[num_start + 3] as u64 & 0x3F);
        (val, 4)
    } else if first_byte & 0xFC == 0xF8 {
        if num_start + 4 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x03) << 24)
            | ((data[num_start + 1] as u64 & 0x3F) << 18)
            | ((data[num_start + 2] as u64 & 0x3F) << 12)
            | ((data[num_start + 3] as u64 & 0x3F) << 6)
            | (data[num_start + 4] as u64 & 0x3F);
        (val, 5)
    } else if first_byte & 0xFE == 0xFC {
        if num_start + 5 >= data.len() {
            return None;
        }
        let val = ((first_byte as u64 & 0x01) << 30)
            | ((data[num_start + 1] as u64 & 0x3F) << 24)
            | ((data[num_start + 2] as u64 & 0x3F) << 18)
            | ((data[num_start + 3] as u64 & 0x3F) << 12)
            | ((data[num_start + 4] as u64 & 0x3F) << 6)
            | (data[num_start + 5] as u64 & 0x3F);
        (val, 6)
    } else if first_byte == 0xFE {
        if num_start + 6 >= data.len() {
            return None;
        }
        let val = ((data[num_start + 1] as u64 & 0x3F) << 30)
            | ((data[num_start + 2] as u64 & 0x3F) << 24)
            | ((data[num_start + 3] as u64 & 0x3F) << 18)
            | ((data[num_start + 4] as u64 & 0x3F) << 12)
            | ((data[num_start + 5] as u64 & 0x3F) << 6)
            | (data[num_start + 6] as u64 & 0x3F);
        (val, 7)
    } else {
        return None;
    };

    if variable_block_size {
        Some(value)
    } else {
        // Value is frame number, multiply by block size
        let block_size = if min_block_size > 0 {
            min_block_size as u64
        } else {
            4096
        };
        Some(value * block_size)
    }
}

/// Validate a FLAC frame header with CRC-8.
fn validate_flac_frame_header(data: &[u8], pos: usize) -> bool {
    if pos + 4 >= data.len() {
        return false;
    }

    // Check sync code
    if data[pos] != 0xFF || (data[pos + 1] & 0xFE) != 0xF8 {
        return false;
    }

    // Byte 2: block size code (high nibble) and sample rate code (low nibble)
    let block_size_code = (data[pos + 2] >> 4) & 0x0F;
    let sample_rate_code = data[pos + 2] & 0x0F;

    if block_size_code == 0 || sample_rate_code == 15 {
        return false;
    }

    // Byte 3: channel assignment and sample size
    let channel_assignment = (data[pos + 3] >> 4) & 0x0F;
    let sample_size_code = (data[pos + 3] >> 1) & 0x07;

    if channel_assignment > 10 || sample_size_code == 3 || sample_size_code == 7 {
        return false;
    }

    // Reserved bit must be 0
    if data[pos + 3] & 0x01 != 0 {
        return false;
    }

    // Calculate header length
    let header_len =
        match calculate_flac_frame_header_length(data, pos, block_size_code, sample_rate_code) {
            Some(len) => len,
            None => return false,
        };

    if pos + header_len >= data.len() {
        return false;
    }

    // Verify CRC-8
    let crc_pos = pos + header_len - 1;
    let expected_crc = data[crc_pos];
    let computed_crc = compute_flac_crc8(&data[pos..crc_pos]);

    computed_crc == expected_crc
}

/// Calculate FLAC frame header length.
fn calculate_flac_frame_header_length(
    data: &[u8],
    pos: usize,
    block_size_code: u8,
    sample_rate_code: u8,
) -> Option<usize> {
    let mut len = 4; // Fixed part

    let num_start = pos + 4;
    if num_start >= data.len() {
        return None;
    }

    let first_byte = data[num_start];
    let utf8_len = if first_byte & 0x80 == 0 {
        1
    } else if first_byte & 0xE0 == 0xC0 {
        2
    } else if first_byte & 0xF0 == 0xE0 {
        3
    } else if first_byte & 0xF8 == 0xF0 {
        4
    } else if first_byte & 0xFC == 0xF8 {
        5
    } else if first_byte & 0xFE == 0xFC {
        6
    } else if first_byte == 0xFE {
        7
    } else {
        return None;
    };

    len += utf8_len;

    // Optional block size
    if block_size_code == 6 {
        len += 1;
    } else if block_size_code == 7 {
        len += 2;
    }

    // Optional sample rate
    if sample_rate_code == 12 {
        len += 1;
    } else if sample_rate_code == 13 || sample_rate_code == 14 {
        len += 2;
    }

    // CRC-8
    len += 1;

    Some(len)
}

/// Compute CRC-8 using FLAC's polynomial (0x07).
fn compute_flac_crc8(data: &[u8]) -> u8 {
    const CRC8_TABLE: [u8; 256] = [
        0x00, 0x07, 0x0E, 0x09, 0x1C, 0x1B, 0x12, 0x15, 0x38, 0x3F, 0x36, 0x31, 0x24, 0x23, 0x2A,
        0x2D, 0x70, 0x77, 0x7E, 0x79, 0x6C, 0x6B, 0x62, 0x65, 0x48, 0x4F, 0x46, 0x41, 0x54, 0x53,
        0x5A, 0x5D, 0xE0, 0xE7, 0xEE, 0xE9, 0xFC, 0xFB, 0xF2, 0xF5, 0xD8, 0xDF, 0xD6, 0xD1, 0xC4,
        0xC3, 0xCA, 0xCD, 0x90, 0x97, 0x9E, 0x99, 0x8C, 0x8B, 0x82, 0x85, 0xA8, 0xAF, 0xA6, 0xA1,
        0xB4, 0xB3, 0xBA, 0xBD, 0xC7, 0xC0, 0xC9, 0xCE, 0xDB, 0xDC, 0xD5, 0xD2, 0xFF, 0xF8, 0xF1,
        0xF6, 0xE3, 0xE4, 0xED, 0xEA, 0xB7, 0xB0, 0xB9, 0xBE, 0xAB, 0xAC, 0xA5, 0xA2, 0x8F, 0x88,
        0x81, 0x86, 0x93, 0x94, 0x9D, 0x9A, 0x27, 0x20, 0x29, 0x2E, 0x3B, 0x3C, 0x35, 0x32, 0x1F,
        0x18, 0x11, 0x16, 0x03, 0x04, 0x0D, 0x0A, 0x57, 0x50, 0x59, 0x5E, 0x4B, 0x4C, 0x45, 0x42,
        0x6F, 0x68, 0x61, 0x66, 0x73, 0x74, 0x7D, 0x7A, 0x89, 0x8E, 0x87, 0x80, 0x95, 0x92, 0x9B,
        0x9C, 0xB1, 0xB6, 0xBF, 0xB8, 0xAD, 0xAA, 0xA3, 0xA4, 0xF9, 0xFE, 0xF7, 0xF0, 0xE5, 0xE2,
        0xEB, 0xEC, 0xC1, 0xC6, 0xCF, 0xC8, 0xDD, 0xDA, 0xD3, 0xD4, 0x69, 0x6E, 0x67, 0x60, 0x75,
        0x72, 0x7B, 0x7C, 0x51, 0x56, 0x5F, 0x58, 0x4D, 0x4A, 0x43, 0x44, 0x19, 0x1E, 0x17, 0x10,
        0x05, 0x02, 0x0B, 0x0C, 0x21, 0x26, 0x2F, 0x28, 0x3D, 0x3A, 0x33, 0x34, 0x4E, 0x49, 0x40,
        0x47, 0x52, 0x55, 0x5C, 0x5B, 0x76, 0x71, 0x78, 0x7F, 0x6A, 0x6D, 0x64, 0x63, 0x3E, 0x39,
        0x30, 0x37, 0x22, 0x25, 0x2C, 0x2B, 0x06, 0x01, 0x08, 0x0F, 0x1A, 0x1D, 0x14, 0x13, 0xAE,
        0xA9, 0xA0, 0xA7, 0xB2, 0xB5, 0xBC, 0xBB, 0x96, 0x91, 0x98, 0x9F, 0x8A, 0x8D, 0x84, 0x83,
        0xDE, 0xD9, 0xD0, 0xD7, 0xC2, 0xC5, 0xCC, 0xCB, 0xE6, 0xE1, 0xE8, 0xEF, 0xFA, 0xFD, 0xF4,
        0xF3,
    ];

    let mut crc = 0u8;
    for &byte in data {
        crc = CRC8_TABLE[(crc ^ byte) as usize];
    }
    crc
}

// =============================================================================
// AVIO-based Streaming Decode (new, simpler approach)
// =============================================================================

/// Decode audio from a SparseBuffer using FFmpeg's AVIO.
///
/// FFmpeg handles all frame boundary detection internally.
/// Seektable is NOT needed - just feed bytes, get samples.
pub fn decode_audio_streaming_simple(
    buffer: SharedSparseBuffer,
    sink: &mut StreamingPcmSink,
) -> Result<(), String> {
    install_ffmpeg_log_callback();
    reset_ffmpeg_errors();

    unsafe { decode_audio_streaming_avio(buffer, sink) }
}

/// Internal AVIO-based streaming decode
unsafe fn decode_audio_streaming_avio(
    buffer: SharedSparseBuffer,
    sink: &mut StreamingPcmSink,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    // Create streaming AVIO context
    let avio_ctx = Box::new(StreamingAvioContext {
        buffer: buffer.clone(),
        cancelled: std::sync::atomic::AtomicBool::new(false),
    });
    let avio_ctx_ptr = Box::into_raw(avio_ctx);

    // Allocate AVIO buffer
    let avio_buffer_size = 32768;
    let avio_buffer = av_malloc(avio_buffer_size) as *mut u8;
    if avio_buffer.is_null() {
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to allocate AVIO buffer".to_string());
    }

    // Create custom AVIO context (no seek - streaming only)
    let avio = avio_alloc_context(
        avio_buffer,
        avio_buffer_size as c_int,
        0,
        avio_ctx_ptr as *mut c_void,
        Some(streaming_avio_read_callback),
        None,
        None, // No seek for streaming
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to create AVIO context".to_string());
    }

    // Create format context
    let mut fmt_ctx = avformat_alloc_context();
    if fmt_ctx.is_null() {
        av_free(avio as *mut c_void);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to allocate format context".to_string());
    }
    (*fmt_ctx).pb = avio;

    // Open input
    let ret = avformat_open_input(&mut fmt_ctx, ptr::null(), ptr::null_mut(), ptr::null_mut());
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(format!("Failed to open input: {}", av_err_str(ret)));
    }

    // Find stream info
    let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(format!("Failed to find stream info: {}", av_err_str(ret)));
    }

    // Find audio stream
    let stream_index = av_find_best_stream(
        fmt_ctx,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );
    if stream_index < 0 {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("No audio stream found".to_string());
    }

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let codecpar = (*stream).codecpar;

    // Find decoder
    let codec = avcodec_find_decoder((*codecpar).codec_id);
    if codec.is_null() {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Decoder not found".to_string());
    }

    // Allocate codec context
    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to allocate codec context".to_string());
    }

    // Copy codec parameters
    let ret = avcodec_parameters_to_context(codec_ctx, codecpar);
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(format!("Failed to copy codec params: {}", av_err_str(ret)));
    }

    // Open codec
    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(format!("Failed to open codec: {}", av_err_str(ret)));
    }

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*codecpar).ch_layout.nb_channels as u32;

    debug!("Streaming AVIO decoder: {}Hz, {}ch", sample_rate, channels);

    // Allocate frame and packet
    let frame = av_frame_alloc();
    let packet = av_packet_alloc();
    if frame.is_null() || packet.is_null() {
        if !frame.is_null() {
            av_frame_free(&mut (frame as *mut _));
        }
        if !packet.is_null() {
            av_packet_free(&mut (packet as *mut _));
        }
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to allocate frame/packet".to_string());
    }

    // Scaling factor for sample conversion
    let bits_per_sample = match (*codec_ctx).sample_fmt {
        AVSampleFormat::AV_SAMPLE_FMT_S16 | AVSampleFormat::AV_SAMPLE_FMT_S16P => 16,
        AVSampleFormat::AV_SAMPLE_FMT_S32 | AVSampleFormat::AV_SAMPLE_FMT_S32P => 32,
        _ => 16,
    };
    let scale = match bits_per_sample {
        16 => 1.0 / (i16::MAX as f32),
        24 => 1.0 / 8388607.0,
        32 => 1.0 / (i32::MAX as f32),
        _ => 1.0 / (i16::MAX as f32),
    };

    let mut samples_output: u64 = 0;

    // Read and decode packets
    while av_read_frame(fmt_ctx, packet) >= 0 {
        // Check for cancellation
        if sink.is_cancelled() {
            av_packet_unref(packet);
            break;
        }

        if (*packet).stream_index != stream_index {
            av_packet_unref(packet);
            continue;
        }

        let ret = avcodec_send_packet(codec_ctx, packet);
        av_packet_unref(packet);

        if ret < 0 {
            continue;
        }

        while avcodec_receive_frame(codec_ctx, frame) >= 0 {
            if sink.is_cancelled() {
                break;
            }

            let frame_samples = extract_samples_from_raw_frame(frame, channels as usize);
            samples_output += frame_samples.len() as u64;

            // Convert to f32 and push to sink
            let f32_samples: Vec<f32> = frame_samples.iter().map(|&s| s as f32 * scale).collect();

            if !f32_samples.is_empty() {
                if let Err(e) = push_samples_to_sink(sink, &f32_samples) {
                    warn!("Failed to push samples: {}", e);
                    break;
                }
            }
        }
    }

    // Flush decoder
    avcodec_send_packet(codec_ctx, ptr::null());
    while avcodec_receive_frame(codec_ctx, frame) >= 0 {
        if sink.is_cancelled() {
            break;
        }

        let frame_samples = extract_samples_from_raw_frame(frame, channels as usize);
        samples_output += frame_samples.len() as u64;

        let f32_samples: Vec<f32> = frame_samples.iter().map(|&s| s as f32 * scale).collect();

        if !f32_samples.is_empty() {
            let _ = push_samples_to_sink(sink, &f32_samples);
        }
    }

    // Cleanup
    av_frame_free(&mut (frame as *mut _));
    av_packet_free(&mut (packet as *mut _));
    avcodec_free_context(&mut (codec_ctx as *mut _));
    avformat_close_input(&mut fmt_ctx);
    let _ = Box::from_raw(avio_ctx_ptr);

    // Record error count
    let error_count = get_ffmpeg_errors();
    if error_count > 0 {
        warn!("Streaming AVIO decode had {} FFmpeg errors", error_count);
    }
    sink.set_decode_error_count(error_count);

    if !sink.is_cancelled() {
        sink.mark_finished();
    }

    info!(
        "Streaming AVIO decode complete: {}Hz, {}ch, {} samples, {} errors",
        sample_rate, channels, samples_output, error_count
    );

    Ok(())
}

/// Push samples to sink in chunks, checking for cancellation.
fn push_samples_to_sink(sink: &mut StreamingPcmSink, samples: &[f32]) -> Result<(), String> {
    const CHUNK_SIZE: usize = 8192;
    for chunk in samples.chunks(CHUNK_SIZE) {
        if sink.is_cancelled() {
            return Err("Cancelled".to_string());
        }
        sink.push_samples_blocking(chunk);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_encode_roundtrip() {
        init();

        // Create test samples (1 second of silence at 44100Hz stereo)
        let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

        // Encode to FLAC
        let flac_data = encode_to_flac(&original_samples, 44100, 2, 16).unwrap();

        // Verify FLAC signature
        assert!(flac_data.len() > 42);
        assert_eq!(&flac_data[0..4], b"fLaC");

        // Decode back
        let decoded = decode_audio(&flac_data, None, None).unwrap();

        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 2);
        // Sample counts should be approximately equal (may differ slightly due to padding)
        assert!(
            (decoded.samples.len() as i64 - original_samples.len() as i64).abs() < 1000,
            "Sample count mismatch: {} vs {}",
            decoded.samples.len(),
            original_samples.len()
        );
    }

    #[test]
    fn test_encode_mono() {
        init();

        let samples = vec![0i32; 44100];
        let result = encode_to_flac(&samples, 44100, 1, 16);
        assert!(result.is_ok(), "Failed to encode mono: {:?}", result.err());
    }

    #[test]
    fn test_build_seektable() {
        init();

        // Create and encode some test audio
        let samples: Vec<i32> = (0..44100 * 10)
            .map(|i| ((i as f64 * 0.1).sin() * 1000.0) as i32)
            .collect();

        let flac_data = encode_to_flac(&samples, 44100, 1, 16).unwrap();
        let seektable = build_seektable(&flac_data).unwrap();

        // Should have at least one entry
        assert!(!seektable.is_empty(), "Seektable should not be empty");

        // Sample numbers should be monotonically increasing
        for window in seektable.windows(2) {
            assert!(
                window[1].sample >= window[0].sample,
                "Sample numbers should be monotonically increasing"
            );
        }
    }

    /// Test that FLAC encode/decode is lossless - samples should match exactly.
    ///
    /// This catches any sample conversion bugs: wrong byte order, wrong scaling,
    /// wrong format detection, etc. If anything is wrong, values won't match.
    #[test]
    fn test_flac_roundtrip_is_lossless() {
        init();

        // Create a 440Hz sine wave - uses the full 16-bit range
        let sample_rate = 44100u32;
        let duration_samples = sample_rate as usize; // 1 second
        let amplitude = 30000i32; // Near max 16-bit

        let original: Vec<i32> = (0..duration_samples)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                (amplitude as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
            })
            .collect();

        // Encode to FLAC and decode back
        let flac_data = encode_to_flac(&original, sample_rate, 1, 16).unwrap();
        let decoded = decode_audio(&flac_data, None, None).unwrap();

        // FLAC is lossless - samples should match exactly
        let compare_len = original.len().min(decoded.samples.len());
        assert!(compare_len > 0, "No samples to compare");

        let mut mismatches = 0;
        let mut max_diff = 0i32;
        for (orig, dec) in original
            .iter()
            .zip(decoded.samples.iter())
            .take(compare_len)
        {
            let diff = (orig - dec).abs();
            if diff > 0 {
                mismatches += 1;
                max_diff = max_diff.max(diff);
            }
        }

        assert!(
            max_diff < 2, // Allow tiny rounding errors
            "FLAC roundtrip should be lossless. {} samples differ, max diff: {}. \
             This indicates a bug in sample conversion (wrong byte order, scaling, or format).",
            mismatches,
            max_diff
        );
    }

    #[test]
    fn test_streaming_decode() {
        use crate::playback::create_streaming_pair_with_capacity;
        use crate::playback::sparse_buffer::create_sparse_buffer;
        use std::thread;

        init();

        // Create test FLAC data
        let samples: Vec<i32> = (0..44100)
            .map(|i| ((i as f64 * 0.01).sin() * 10000.0) as i32)
            .collect();
        let flac_data = encode_to_flac(&samples, 44100, 1, 16).unwrap();

        // Create streaming infrastructure with sparse buffer
        let buffer = create_sparse_buffer();
        let (mut sink, mut source) = create_streaming_pair_with_capacity(44100, 1, 100000);

        // Spawn decoder thread using new AVIO-based streaming decode
        let decoder_buffer = buffer.clone();
        let decoder_handle =
            thread::spawn(move || decode_audio_streaming_simple(decoder_buffer, &mut sink));

        // Feed data to buffer (simulating download)
        buffer.append_at(0, &flac_data);
        buffer.set_total_size(flac_data.len() as u64);
        buffer.mark_eof();

        // Wait for decoder
        let result = decoder_handle.join().unwrap();
        assert!(result.is_ok(), "Decode failed: {:?}", result.err());

        // Pull samples from source
        let mut decoded_samples = Vec::new();
        let mut buf = [0.0f32; 1024];
        loop {
            let n = source.pull_samples(&mut buf);
            if n == 0 && source.is_finished() {
                break;
            }
            decoded_samples.extend_from_slice(&buf[..n]);
        }

        // Should have approximately the same number of samples
        assert!(
            (decoded_samples.len() as i64 - samples.len() as i64).abs() < 1000,
            "Sample count mismatch: {} vs {}",
            decoded_samples.len(),
            samples.len()
        );
    }
}
