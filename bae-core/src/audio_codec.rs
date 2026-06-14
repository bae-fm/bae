//! Unified audio codec module using FFmpeg.
//!
//! Provides decoding (any format to PCM), encoding (PCM to FLAC), and
//! seektable generation. Uses custom AVIO for in-memory decoding.

use crate::playback::{SharedSparseBuffer, TrackSink};
use crate::util::content_type::ContentType;
use std::cell::Cell;
use std::fmt;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use tracing::{debug, info, trace, warn};

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

/// The `va_list` parameter type of FFmpeg's log callback, as bindgen renders it
/// per target ABI. Linux and Android share calling conventions: SysV x86_64 ->
/// pointer-to-`__va_list_tag`; AAPCS64 aarch64 -> a 32-byte register-save struct
/// (`__BindgenOpaqueArray<u64, 4>`). macOS/Windows keep the `c_char` pointer
/// (Apple's arm64 ABI passes varargs on the stack, so its `va_list` is `char*`).
/// The callback never reads `_vl`; this type only has to match the signature
/// bindgen generated for `av_log_set_callback` on the target, or assignment
/// below won't type-check.
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    target_arch = "x86_64"
))]
type FfmpegVaList = *mut ffmpeg_sys_next::__va_list_tag;
#[cfg(all(
    any(target_os = "linux", target_os = "android"),
    target_arch = "aarch64"
))]
type FfmpegVaList = ffmpeg_sys_next::__BindgenOpaqueArray<u64, 4>;
#[cfg(not(all(
    any(target_os = "linux", target_os = "android"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
type FfmpegVaList = *mut std::ffi::c_char;

/// Custom FFmpeg log callback that counts fatal errors per-thread.
unsafe extern "C" fn ffmpeg_log_callback(
    _avcl: *mut c_void,
    level: c_int,
    _fmt: *const std::ffi::c_char,
    _vl: FfmpegVaList,
) {
    // Only count AV_LOG_FATAL (8) and AV_LOG_PANIC (0).
    // AV_LOG_ERROR (16) includes recoverable sync errors during seeking.
    if level <= 8 {
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

/// Context for streaming AVIO - reads from a BufferReader which blocks waiting for data
pub(crate) struct StreamingAvioContext {
    reader: std::sync::Mutex<crate::playback::sparse_buffer::BufferReader>,
    buffer: SharedSparseBuffer, // kept for total_size queries
    /// Per-decoder cancellation token. Set by the playback service to stop this
    /// specific decoder without affecting other decoders on the same buffer.
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
}

/// AVIO read callback for streaming - reads from SparseBuffer, blocking until data available
pub(crate) unsafe extern "C" fn streaming_avio_read_callback(
    opaque: *mut c_void,
    buf: *mut u8,
    buf_size: c_int,
) -> c_int {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if ctx.cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
        return ffmpeg_sys_next::AVERROR_EOF;
    }

    let mut temp_buf = vec![0u8; buf_size as usize];
    match ctx.reader.lock().unwrap().read(&mut temp_buf) {
        Some(0) => ffmpeg_sys_next::AVERROR_EOF,
        Some(n) => {
            ptr::copy_nonoverlapping(temp_buf.as_ptr(), buf, n);
            n as c_int
        }
        None => {
            // Reader cancelled
            ctx.cancel_token
                .store(true, std::sync::atomic::Ordering::Relaxed);
            ffmpeg_sys_next::AVERROR_EOF
        }
    }
}

/// AVIO seek callback for streaming - seeks within SparseBuffer
pub(crate) unsafe extern "C" fn streaming_avio_seek_callback(
    opaque: *mut c_void,
    offset: i64,
    whence: c_int,
) -> i64 {
    let ctx = &*(opaque as *const StreamingAvioContext);

    if whence == ffmpeg_sys_next::AVSEEK_SIZE as c_int {
        return ctx.buffer.get_total_size() as i64;
    }

    let mut reader = ctx.reader.lock().unwrap();
    let new_pos = match whence {
        0 => offset as u64,                                    // SEEK_SET
        1 => (reader.get_read_pos() as i64 + offset) as u64,   // SEEK_CUR
        2 => (reader.get_total_size() as i64 + offset) as u64, // SEEK_END
        _ => return -1,
    };

    if reader.seek(new_pos) {
        new_pos as i64
    } else {
        -1
    }
}

/// Probe-discovered audio properties. Populated from `AVCodecID` and
/// `AVCodecParameters`, so every field reflects what FFmpeg actually sees
/// in the bytes rather than what the file extension promises.
pub struct ProbeResult {
    pub content_type: ContentType,
    pub duration: std::time::Duration,
    pub sample_rate: u32,
    pub bits_per_sample: Option<u32>,
    pub channels: u32,
}

/// Map a probe-reported `AVCodecID` to our codec-named `ContentType`.
/// Any codec we don't explicitly name lands in `Other(format!("codec:{...}"))`
/// so it survives DB round-tripping and is visibly not-one-of-ours.
fn content_type_from_codec_id(id: ffmpeg_sys_next::AVCodecID) -> ContentType {
    use ffmpeg_sys_next::AVCodecID;
    match id {
        AVCodecID::AV_CODEC_ID_FLAC => ContentType::Flac,
        AVCodecID::AV_CODEC_ID_MP3 => ContentType::Mp3,
        AVCodecID::AV_CODEC_ID_APE => ContentType::Ape,
        AVCodecID::AV_CODEC_ID_ALAC => ContentType::Alac,
        AVCodecID::AV_CODEC_ID_AAC => ContentType::Aac,
        other => ContentType::Other(format!("codec:{:?}", other)),
    }
}

/// Decode one frame to recover `bits_per_raw_sample` when the container
/// didn't surface it. ALAC streams report 0 on `codecpar` until the decoder
/// has parsed the magic cookie in its first frame. Returns `None` if the
/// decoder still can't determine the bit depth (shouldn't happen for valid
/// ALAC, but we never fabricate a value).
///
/// # Safety
///
/// Caller must ensure `fmt_ctx` is a valid open format context and
/// `stream_index` points at an audio stream with codecpar populated.
unsafe fn decode_one_frame_for_bit_depth(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
) -> Option<u32> {
    use ffmpeg_sys_next::*;

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let codecpar = (*stream).codecpar;

    let codec = avcodec_find_decoder((*codecpar).codec_id);
    if codec.is_null() {
        return None;
    }

    let mut codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        return None;
    }

    if avcodec_parameters_to_context(codec_ctx, codecpar) < 0 {
        avcodec_free_context(&mut codec_ctx);
        return None;
    }

    if avcodec_open2(codec_ctx, codec, ptr::null_mut()) < 0 {
        avcodec_free_context(&mut codec_ctx);
        return None;
    }

    let mut packet = av_packet_alloc();
    let mut frame = av_frame_alloc();
    let mut bits: Option<u32> = None;

    // Read packets until we decode a frame from our stream (or EOF).
    while av_read_frame(fmt_ctx, packet) >= 0 {
        if (*packet).stream_index != stream_index {
            av_packet_unref(packet);
            continue;
        }
        if avcodec_send_packet(codec_ctx, packet) >= 0
            && avcodec_receive_frame(codec_ctx, frame) >= 0
        {
            let raw = (*codec_ctx).bits_per_raw_sample;
            if raw > 0 {
                bits = Some(raw as u32);
            }
            av_packet_unref(packet);
            break;
        }
        av_packet_unref(packet);
    }

    av_frame_free(&mut frame);
    av_packet_free(&mut packet);
    avcodec_free_context(&mut codec_ctx);
    bits
}

/// Open `path` and find its best audio stream. Returns the format context --
/// which the caller must `avformat_close_input` when done -- and the stream
/// index, or `None` (already logged with `ctx` as the prefix) if the file can't
/// be opened, its stream info can't be read, or it has no audio stream.
unsafe fn open_best_audio_stream(
    path: &str,
    ctx: &str,
) -> Option<(*mut ffmpeg_sys_next::AVFormatContext, c_int)> {
    use ffmpeg_sys_next::*;

    let c_path =
        std::ffi::CString::new(path).expect("filesystem path must not contain interior NUL bytes");
    let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
    if avformat_open_input(
        &mut fmt_ctx,
        c_path.as_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
    ) < 0
    {
        warn!("{ctx}: failed to open input {path}");
        return None;
    }
    if avformat_find_stream_info(fmt_ctx, ptr::null_mut()) < 0 {
        warn!("{ctx}: failed to read stream info for {path}");
        avformat_close_input(&mut fmt_ctx);
        return None;
    }
    let stream_index = av_find_best_stream(
        fmt_ctx,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );
    if stream_index < 0 {
        warn!("{ctx}: no audio stream in {path}");
        avformat_close_input(&mut fmt_ctx);
        return None;
    }
    Some((fmt_ctx, stream_index))
}

pub fn probe_audio_from_path(path: &str) -> Option<ProbeResult> {
    unsafe {
        use ffmpeg_sys_next::*;

        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "probe_audio_from_path")?;
        let duration_us = (*fmt_ctx).duration;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let codecpar = (*stream).codecpar;
        let codec_id = (*codecpar).codec_id;
        let sample_rate = (*codecpar).sample_rate as u32;
        let mut bits_per_sample = if (*codecpar).bits_per_raw_sample > 0 {
            Some((*codecpar).bits_per_raw_sample as u32)
        } else if (*codecpar).bits_per_coded_sample > 0 {
            Some((*codecpar).bits_per_coded_sample as u32)
        } else {
            None
        };
        let channels = (*codecpar).ch_layout.nb_channels as u32;

        // ALAC-in-MP4 doesn't populate bits_per_raw_sample on codecpar until
        // the decoder has processed the magic cookie. Fall back to decoding
        // a single frame just to recover the real bit depth.
        if bits_per_sample.is_none() && codec_id == AVCodecID::AV_CODEC_ID_ALAC {
            bits_per_sample = decode_one_frame_for_bit_depth(fmt_ctx, stream_index);
        }

        avformat_close_input(&mut fmt_ctx);

        if duration_us > 0 {
            Some(ProbeResult {
                content_type: content_type_from_codec_id(codec_id),
                duration: std::time::Duration::from_micros(duration_us as u64),
                sample_rate,
                bits_per_sample,
                channels,
            })
        } else {
            debug!("probe_audio_from_path: no usable container duration for {path}");
            None
        }
    }
}

/// The byte offset in the file of the frame containing each of `samples`, found
/// by seeking -- not decoding. For each sample the demuxer is seeked to it and
/// the next frame's byte position is read back: the frame at or before the
/// sample, since FLAC and APE frames are independently seekable. Frame-granular
/// (the offset is at or just before the exact sample), which is all the read-ahead
/// ceiling needs. Used at import to record where each track of a single-file
/// album begins in the file. `samples` should be ascending. Returns `None` if the
/// file can't be opened, has no audio stream, or any sample's frame position
/// can't be read -- the caller then treats the whole file as one span.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn frame_byte_offsets(path: &str, samples: &[u64]) -> Option<Vec<u64>> {
    unsafe {
        use ffmpeg_sys_next::*;

        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "frame_byte_offsets")?;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let time_base = (*stream).time_base;
        let sample_rate = (*(*stream).codecpar).sample_rate as i64;

        let packet = av_packet_alloc();
        if packet.is_null() {
            warn!("frame_byte_offsets: failed to allocate packet for {path}");
            avformat_close_input(&mut fmt_ctx);
            return None;
        }

        // Compute every offset, or bail out (returning None) if any sample's
        // frame position can't be read -- a half-faked boundary list is worse
        // than falling back to the whole file.
        let mut offsets = Vec::with_capacity(samples.len());
        let mut failed: Option<String> = None;
        for &sample in samples {
            // Sample number -> timestamp in the stream's time base (1/sample_rate
            // for FLAC/APE, so the timestamp is the sample number).
            let target_ts = if time_base.num == 1 && time_base.den as i64 == sample_rate {
                sample as i64
            } else if sample_rate > 0 && time_base.num > 0 {
                (sample as i128 * time_base.den as i128
                    / (time_base.num as i128 * sample_rate as i128)) as i64
            } else {
                failed = Some(format!("invalid time base for sample {sample}"));
                break;
            };
            if av_seek_frame(
                fmt_ctx,
                stream_index,
                target_ts,
                AVSEEK_FLAG_BACKWARD as c_int,
            ) < 0
            {
                failed = Some(format!("seek to sample {sample} failed"));
                break;
            }

            // The next packet for our stream carries its byte position in `pos`.
            let mut pos = None;
            while av_read_frame(fmt_ctx, packet) >= 0 {
                let is_audio = (*packet).stream_index == stream_index;
                let p = (*packet).pos;
                av_packet_unref(packet);
                if is_audio {
                    if p >= 0 {
                        pos = Some(p as u64);
                    }
                    break;
                }
            }
            match pos {
                Some(p) => offsets.push(p),
                None => {
                    failed = Some(format!("no frame position at sample {sample}"));
                    break;
                }
            }
        }

        av_packet_free(&mut (packet as *mut _));
        avformat_close_input(&mut fmt_ctx);
        match failed {
            Some(reason) => {
                warn!("frame_byte_offsets: {reason} in {path}");
                None
            }
            None => Some(offsets),
        }
    }
}

/// Decode any audio format to PCM samples.
///
/// If start_ms/end_ms are provided, only that time range is decoded.
/// Returns interleaved i32 samples.
pub fn decode_audio(
    data: &[u8],
    start_sample: Option<u64>,
    end_sample: Option<u64>,
) -> Result<DecodedAudio, String> {
    // Safety: FFmpeg operations are contained within this function
    unsafe { decode_audio_avio(data, start_sample, end_sample) }
}

/// Internal AVIO-based decode implementation
unsafe fn decode_audio_avio(
    data: &[u8],
    start_sample: Option<u64>,
    end_sample: Option<u64>,
) -> Result<DecodedAudio, String> {
    use ffmpeg_sys_next::*;

    // Create our context for callbacks
    let mut avio_ctx = Box::new(AvioContext {
        data: data.as_ptr(),
        size: data.len(),
        pos: 0,
    });

    // Allocate AVIO buffer (FFmpeg will manage this)
    let avio_buffer_size = AVIO_BUFFER_SIZE;
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

    let time_base = (*stream).time_base;

    // Seek to start position if specified (using stream time_base for exact positioning)
    if let Some(sample_pos) = start_sample {
        av_seek_frame(
            fmt_ctx,
            stream_index,
            sample_pos as i64,
            AVSEEK_FLAG_BACKWARD as c_int,
        );
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
    let mut tracked_sample_pos: i64 = -1;

    // Read and decode packets, trimming by sample position
    let mut reached_end = false;
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
            let num_samples = (*frame).nb_samples as usize;
            let pts = (*frame).pts;

            // Determine frame's sample position from PTS (with fallback tracking)
            let frame_start = if pts != AV_NOPTS_VALUE {
                let sample_pos = if time_base.num == 1 && time_base.den == sample_rate as c_int {
                    pts
                } else {
                    (pts as f64 * time_base.num as f64 / time_base.den as f64 * sample_rate as f64)
                        as i64
                };
                tracked_sample_pos = sample_pos;
                sample_pos
            } else if tracked_sample_pos >= 0 {
                tracked_sample_pos
            } else {
                -1
            };
            let frame_end = frame_start + num_samples as i64;
            if tracked_sample_pos >= 0 {
                tracked_sample_pos = frame_end;
            }

            // Skip frames entirely before start
            if let Some(start) = start_sample {
                let start = start as i64;
                if frame_start >= 0 && frame_end <= start {
                    continue;
                }
            }

            // Stop at end
            if let Some(end) = end_sample {
                let end = end as i64;
                if frame_start >= 0 && frame_start >= end {
                    reached_end = true;
                    break;
                }
            }

            let frame_samples_vec = extract_samples_from_raw_frame(frame, channels as usize);

            // Trim start of frame
            let skip_start = if let Some(start) = start_sample {
                let start = start as i64;
                if frame_start >= 0 && frame_start < start {
                    let skip = (start - frame_start) as usize * channels as usize;
                    skip.min(frame_samples_vec.len())
                } else {
                    0
                }
            } else {
                0
            };

            // Trim end of frame
            let take_end = if let Some(end) = end_sample {
                let end = end as i64;
                if frame_start >= 0 && frame_end > end {
                    let keep = (end - frame_start) as usize * channels as usize;
                    reached_end = true;
                    keep.min(frame_samples_vec.len())
                } else {
                    frame_samples_vec.len()
                }
            } else {
                frame_samples_vec.len()
            };

            if skip_start < take_end {
                samples.extend_from_slice(&frame_samples_vec[skip_start..take_end]);
            }
        }

        if reached_end {
            break;
        }
    }

    // Flush decoder — only if we haven't reached end
    if !reached_end {
        avcodec_send_packet(codec_ctx, ptr::null());
        while avcodec_receive_frame(codec_ctx, frame) >= 0 {
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
    if bytes_per_sample <= 0 {
        warn!(
            "av_get_bytes_per_sample returned {} for format {:?}, skipping frame",
            bytes_per_sample,
            (*frame).format
        );
        return Vec::new();
    }
    let actual_bytes_per_sample = bytes_per_sample as usize;

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
pub(crate) fn av_err_str(errnum: i32) -> String {
    unsafe {
        let mut buf = [0 as std::ffi::c_char; 256];
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

/// AVIO write callback - writes bytes to our memory buffer
unsafe extern "C" fn avio_write_callback(
    opaque: *mut c_void,
    buf: *const u8,
    buf_size: c_int,
) -> c_int {
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

// =============================================================================
// AVIO-based Streaming Decode (new, simpler approach)
// =============================================================================

/// Decode audio from a SparseBuffer using FFmpeg's AVIO.
///
/// FFmpeg handles all frame boundary detection internally.
/// Seektable is NOT needed - just feed bytes, get samples.
/// Decode audio from a SparseBuffer, pushing f32 samples to a TrackSink.
///
/// The buffer holds the whole backing file. `seek_to_sample` jumps FFmpeg to the
/// track's start; `start_at_sample` trims the lead-in FFmpeg outputs before the
/// exact start (a frame may begin before it); `stop_at_sample` stops output at
/// the track's end. `end_byte` is the track's end byte offset -- the read-ahead
/// ceiling handed to the reader so the fill buffers the rest of this track;
/// `None` keeps the whole file (per-track / last track).
pub fn decode_audio_streaming(
    buffer: SharedSparseBuffer,
    sink: &mut TrackSink,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    end_byte: Option<u64>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), StreamingDecodeError> {
    install_ffmpeg_log_callback();
    reset_ffmpeg_errors();

    unsafe {
        decode_audio_streaming_impl(
            buffer,
            sink,
            seek_to_sample,
            start_at_sample,
            stop_at_sample,
            end_byte,
            cancel_token,
        )
    }
}

/// Internal AVIO-based streaming decode
unsafe fn decode_audio_streaming_impl(
    buffer: SharedSparseBuffer,
    sink: &mut TrackSink,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    end_byte: Option<u64>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), StreamingDecodeError> {
    use ffmpeg_sys_next::*;

    // Create streaming AVIO context. The reader's read-ahead ceiling is set
    // after the seek below, once it sits at the track's start -- so the fill
    // buffers the rest of the current track, not from byte 0 during probe.
    let reader = buffer.new_reader_with_cancel(cancel_token.clone());
    let cancel_status = cancel_token.clone();
    let avio_ctx = Box::new(StreamingAvioContext {
        reader: std::sync::Mutex::new(reader),
        buffer: buffer.clone(),
        cancel_token,
    });
    let avio_ctx_ptr = Box::into_raw(avio_ctx);

    // Allocate AVIO buffer
    let avio_buffer_size = AVIO_BUFFER_SIZE;
    let avio_buffer = av_malloc(avio_buffer_size) as *mut u8;
    if avio_buffer.is_null() {
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to allocate AVIO buffer",
        ));
    }

    // Create custom AVIO context with seek support
    let avio = avio_alloc_context(
        avio_buffer,
        avio_buffer_size as c_int,
        0,
        avio_ctx_ptr as *mut c_void,
        Some(streaming_avio_read_callback),
        None,
        Some(streaming_avio_seek_callback),
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to create AVIO context",
        ));
    }

    // Mark stream as seekable so avformat_seek_file works
    (*avio).seekable = AVIO_SEEKABLE_NORMAL as c_int;

    // Create format context
    let mut fmt_ctx = avformat_alloc_context();
    if fmt_ctx.is_null() {
        av_free(avio as *mut c_void);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to allocate format context",
        ));
    }
    (*fmt_ctx).pb = avio;

    // Open input
    let ret = avformat_open_input(&mut fmt_ctx, ptr::null(), ptr::null_mut(), ptr::null_mut());
    if ret < 0 {
        avformat_free_context(fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::input_error(
            &cancel_status,
            format!("Failed to open input: {}", av_err_str(ret)),
        ));
    }

    // Find stream info
    let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
    if ret < 0 {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::input_error(
            &cancel_status,
            format!("Failed to find stream info: {}", av_err_str(ret)),
        ));
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
        return Err(StreamingDecodeError::decode("No audio stream found"));
    }

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let codecpar = (*stream).codecpar;

    // Find decoder
    let codec = avcodec_find_decoder((*codecpar).codec_id);
    if codec.is_null() {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode("Decoder not found"));
    }

    // Allocate codec context
    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to allocate codec context",
        ));
    }

    // Copy codec parameters
    let ret = avcodec_parameters_to_context(codec_ctx, codecpar);
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(format!(
            "Failed to copy codec params: {}",
            av_err_str(ret)
        )));
    }

    // Open codec
    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(format!(
            "Failed to open codec: {}",
            av_err_str(ret)
        )));
    }

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*codecpar).ch_layout.nb_channels as u32;

    debug!("Streaming AVIO decoder: {}Hz, {}ch", sample_rate, channels);

    // Set up SwrContext to convert any input format to packed f32
    let in_ch_layout = (*codecpar).ch_layout;
    let codec_ctx_format = (*codec_ctx).sample_fmt;
    let mut swr_ctx: *mut SwrContext = std::ptr::null_mut();
    let ret = swr_alloc_set_opts2(
        &mut swr_ctx,
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_FLT,
        sample_rate as c_int,
        &in_ch_layout,
        codec_ctx_format,
        sample_rate as c_int,
        0,
        std::ptr::null_mut(),
    );
    if ret < 0 || swr_ctx.is_null() {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(format!(
            "Failed to allocate SwrContext: {}",
            av_err_str(ret)
        )));
    }
    let ret = swr_init(swr_ctx);
    if ret < 0 {
        swr_free(&mut swr_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(format!(
            "Failed to init SwrContext: {}",
            av_err_str(ret)
        )));
    }

    // Seek to target sample position if requested
    if let Some(sample_pos) = seek_to_sample {
        let target_ts = sample_pos as i64;
        let ret = avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target_ts, target_ts, 0);
        if ret < 0 {
            warn!(
                "avformat_seek_file failed ({}), decoding from start",
                av_err_str(ret)
            );
        } else {
            avcodec_flush_buffers(codec_ctx);
        }
    }

    // The reader now sits at the track's start (after the seek above). Set its
    // read-ahead ceiling -- the track's end byte, or the whole file when the
    // track runs to EOF (a per-track file or an album's last track) -- so the
    // fill buffers the rest of this track ahead of the playhead.
    let ceiling = match end_byte {
        Some(end) => end,
        None => buffer.get_total_size(),
    };
    (*avio_ctx_ptr)
        .reader
        .lock()
        .unwrap()
        .set_readahead_ceiling(ceiling);

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
        swr_free(&mut swr_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to allocate frame/packet",
        ));
    }

    let mut samples_output: u64 = 0;
    let mut reached_stop = false;
    let mut tracked_sample_pos: i64 = -1;

    // Read and decode packets
    while av_read_frame(fmt_ctx, packet) >= 0 {
        // Check for cancellation
        if sink.is_cancelled() || reached_stop {
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

            let num_samples = (*frame).nb_samples as usize;
            let mut output_buf = vec![0.0f32; num_samples * channels as usize];
            let out_ptr = output_buf.as_mut_ptr() as *mut u8;
            swr_convert(
                swr_ctx,
                &out_ptr,
                num_samples as c_int,
                (*frame).extended_data as *const *const u8,
                (*frame).nb_samples,
            );

            // Determine frame's sample position from PTS (with fallback tracking)
            let time_base = (*stream).time_base;
            let pts = (*frame).pts;
            let frame_start_sample = if pts != ffmpeg_sys_next::AV_NOPTS_VALUE {
                // PTS in stream time_base units — for audio this is typically sample number
                let sample_pos = if time_base.num == 1 && time_base.den == sample_rate as c_int {
                    pts
                } else {
                    // Convert PTS to sample position
                    (pts as f64 * time_base.num as f64 / time_base.den as f64 * sample_rate as f64)
                        as i64
                };
                tracked_sample_pos = sample_pos;
                sample_pos
            } else if tracked_sample_pos >= 0 {
                tracked_sample_pos
            } else {
                -1
            };
            let frame_end_sample = frame_start_sample + num_samples as i64;
            if tracked_sample_pos >= 0 {
                tracked_sample_pos = frame_end_sample;
            }

            // Determine which portion of this frame to output
            let mut skip_start: usize = 0;
            let mut take_end: usize = output_buf.len();

            // Trim start: skip samples before start_at_sample
            if let Some(start) = start_at_sample {
                let start = start as i64;
                if frame_start_sample >= 0 && frame_end_sample <= start {
                    continue;
                }
                if frame_start_sample >= 0 && frame_start_sample < start {
                    let skip = (start - frame_start_sample) as usize * channels as usize;
                    skip_start = skip.min(output_buf.len());
                }
            }

            // Trim end: stop at stop_at_sample
            if let Some(stop) = stop_at_sample {
                let stop = stop as i64;
                if frame_start_sample >= 0 && frame_start_sample >= stop {
                    reached_stop = true;
                    break;
                }
                if frame_start_sample >= 0 && frame_end_sample > stop {
                    let keep = (stop - frame_start_sample) as usize * channels as usize;
                    take_end = keep.min(output_buf.len());
                    reached_stop = true;
                }
            }

            let samples_to_output = if skip_start < take_end {
                &output_buf[skip_start..take_end]
            } else {
                continue;
            };

            samples_output += samples_to_output.len() as u64;

            if !samples_to_output.is_empty() && !push_samples_to_sink(sink, samples_to_output) {
                break;
            }
        }
    }

    // Flush decoder — only if we haven't reached stop_at
    if !reached_stop {
        avcodec_send_packet(codec_ctx, ptr::null());
        while avcodec_receive_frame(codec_ctx, frame) >= 0 {
            if sink.is_cancelled() {
                break;
            }

            let num_samples = (*frame).nb_samples as usize;
            let mut output_buf = vec![0.0f32; num_samples * channels as usize];
            let out_ptr = output_buf.as_mut_ptr() as *mut u8;
            swr_convert(
                swr_ctx,
                &out_ptr,
                num_samples as c_int,
                (*frame).extended_data as *const *const u8,
                (*frame).nb_samples,
            );

            samples_output += output_buf.len() as u64;

            if !output_buf.is_empty() && !push_samples_to_sink(sink, &output_buf) {
                break;
            }
        }
    }

    // Cleanup
    av_frame_free(&mut (frame as *mut _));
    av_packet_free(&mut (packet as *mut _));
    swr_free(&mut swr_ctx);
    avcodec_free_context(&mut (codec_ctx as *mut _));
    avformat_close_input(&mut fmt_ctx);
    let _ = Box::from_raw(avio_ctx_ptr);

    // Record fatal error count (AV_LOG_FATAL and worse)
    let error_count = get_ffmpeg_errors();
    if error_count > 0 {
        warn!(
            "Streaming AVIO decode had {} fatal FFmpeg errors",
            error_count
        );
    }
    sink.set_decode_error_count(error_count);
    sink.set_samples_decoded(samples_output);

    if !sink.is_cancelled() {
        sink.mark_finished();
    }

    info!(
        "Streaming AVIO decode complete: {}Hz, {}ch, {} samples, {} fatal errors",
        sample_rate, channels, samples_output, error_count
    );

    Ok(())
}

/// Push samples to sink in chunks, checking for cancellation between chunks.
/// Returns `false` when the sink is cancelled, signaling the decode loop to exit.
fn push_samples_to_sink(sink: &mut TrackSink, samples: &[f32]) -> bool {
    const CHUNK_SIZE: usize = 8192;
    for chunk in samples.chunks(CHUNK_SIZE) {
        if sink.is_cancelled() {
            return false;
        }
        sink.push_samples_blocking(chunk);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `frame_byte_offsets` actually opens the file, seeks (no decode), and
    /// reports ascending byte positions within the file that follow the sample
    /// positions -- what the import-time byte boundary computation relies on.
    #[test]
    fn frame_byte_offsets_track_sample_positions() {
        init();
        let sample_rate = 44100u32;
        let seconds = 4usize;
        let total = sample_rate as usize * seconds;
        let samples: Vec<i32> = (0..total)
            .map(|i| ((i as f64 * 0.02).sin() * 12000.0) as i32)
            .collect();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let flac = encode_to_flac(&samples, sample_rate, 1, 16, &cancel).unwrap();

        let mut file = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut file, &flac).unwrap();
        std::io::Write::flush(&mut file).unwrap();
        let path = file.path().to_str().unwrap();
        let file_size = flac.len() as u64;

        // Probe at second boundaries (0, 1s, 2s, 3s).
        let probes: Vec<u64> = (0..seconds as u64)
            .map(|s| s * sample_rate as u64)
            .collect();
        let offsets = frame_byte_offsets(path, &probes).expect("frame_byte_offsets");
        assert_eq!(offsets.len(), probes.len());

        // Non-decreasing and within the file.
        for w in offsets.windows(2) {
            assert!(
                w[1] >= w[0],
                "byte offsets must ascend with samples: {offsets:?}"
            );
        }
        assert!(
            *offsets.last().unwrap() < file_size,
            "offsets stay within the file ({offsets:?} vs {file_size})"
        );
        // A later sample is strictly later in the file, and the 3/4 sample is
        // well into the file -- the offsets follow the content, not a constant.
        assert!(offsets[seconds - 1] > offsets[0]);
        assert!(
            offsets[seconds - 1] as f64 > file_size as f64 * 0.4,
            "the 3/4 sample should be well into the file: {} of {file_size}",
            offsets[seconds - 1]
        );
    }

    /// `frame_byte_offsets` against the real single-file CUE/FLAC fixture:
    /// silence -> white noise -> brown noise, so the bitrate swings widely from
    /// track to track. The seek offsets follow the *content*, not time -- the
    /// silence track compresses to almost nothing, so the 10s track-2 boundary
    /// lands at a small slice of the file, far below where a time-proportional
    /// estimate puts it. That gap is why the offsets are computed at import by
    /// seeking rather than estimated from time at playback.
    #[test]
    fn frame_byte_offsets_follow_content_not_time_when_bitrate_swings() {
        init();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/cue_flac/Test Album.flac"
        );
        let probe = probe_audio_from_path(path).expect("probe fixture");
        let sr = probe.sample_rate as u64;
        let total_samples = (probe.duration.as_secs_f64() * sr as f64) as u64;
        let file_size = std::fs::metadata(path).unwrap().len();

        // Track starts from the CUE: 0s (silence), 10s (white noise), 20s (brown).
        let starts = [0u64, 10 * sr, 20 * sr];
        let exact = frame_byte_offsets(path, &starts).expect("offsets");

        // Time-proportional estimate -- what we'd get without seeking.
        let proportional: Vec<u64> = starts
            .iter()
            .map(|&s| (s as f64 / total_samples as f64 * file_size as f64) as u64)
            .collect();

        // The 10s boundary (end of the silence track) is where they diverge most:
        // silence is nearly free to store, so the exact offset is a small slice of
        // the file while the estimate puts it ~10/total of the way in.
        assert!(
            exact[1] < file_size / 5,
            "silence track should occupy a small byte slice; exact[1]={} of {file_size}",
            exact[1]
        );
        assert!(
            proportional[1] > exact[1] * 2,
            "the time estimate over-places the boundary: prop={} exact={}",
            proportional[1],
            exact[1]
        );
        // Ascending and within the file.
        assert!(exact[0] <= exact[1] && exact[1] <= exact[2] && exact[2] < file_size);
    }

    #[test]
    fn content_type_from_codec_id_maps_every_named_codec() {
        use ffmpeg_sys_next::AVCodecID;
        assert_eq!(
            content_type_from_codec_id(AVCodecID::AV_CODEC_ID_FLAC),
            ContentType::Flac
        );
        assert_eq!(
            content_type_from_codec_id(AVCodecID::AV_CODEC_ID_MP3),
            ContentType::Mp3
        );
        assert_eq!(
            content_type_from_codec_id(AVCodecID::AV_CODEC_ID_APE),
            ContentType::Ape
        );
        assert_eq!(
            content_type_from_codec_id(AVCodecID::AV_CODEC_ID_ALAC),
            ContentType::Alac
        );
        assert_eq!(
            content_type_from_codec_id(AVCodecID::AV_CODEC_ID_AAC),
            ContentType::Aac
        );
    }

    #[test]
    fn content_type_from_codec_id_unknown_falls_into_other() {
        use ffmpeg_sys_next::AVCodecID;
        // Any codec outside our whitelist must round-trip as `Other(...)` so
        // the DB preserves the ID and nothing downstream silently treats it
        // as a known format.
        let ct = content_type_from_codec_id(AVCodecID::AV_CODEC_ID_OPUS);
        match ct {
            ContentType::Other(s) => assert!(s.starts_with("codec:")),
            other => panic!("expected Other(codec:...), got {:?}", other),
        }
    }

    #[test]
    fn test_decode_encode_roundtrip() {
        init();

        // Create test samples (1 second of silence at 44100Hz stereo)
        let original_samples: Vec<i32> = vec![0i32; 44100 * 2];

        // Encode to FLAC
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let flac_data = encode_to_flac(&original_samples, 44100, 2, 16, &cancel).unwrap();

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
    fn test_encode_mp3() {
        init();

        // Create a 440Hz sine wave - 1 second stereo
        let sample_rate = 44100u32;
        let duration_samples = sample_rate as usize;
        let amplitude = 30000i32;

        let samples: Vec<i32> = (0..duration_samples * 2)
            .map(|i| {
                let t = (i / 2) as f64 / sample_rate as f64;
                (amplitude as f64 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i32
            })
            .collect();

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mp3_data = encode_to_mp3(&samples, sample_rate, 2, 16, 320_000, &cancel).unwrap();

        // MP3 files start with either ID3 tag (0x49 0x44 0x33) or sync word (0xFF 0xFB)
        assert!(
            mp3_data.len() > 100,
            "MP3 data too small: {}",
            mp3_data.len()
        );
        assert!(
            (mp3_data[0] == 0xFF && (mp3_data[1] & 0xE0) == 0xE0) || &mp3_data[0..3] == b"ID3",
            "Invalid MP3 header: {:02X} {:02X} {:02X}",
            mp3_data[0],
            mp3_data[1],
            mp3_data[2],
        );

        // Decode it back and verify we get audio
        let decoded = decode_audio(&mp3_data, None, None).unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 2);
        assert!(decoded.samples.len() > 40000, "Too few decoded samples");
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
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let flac_data = encode_to_flac(&original, sample_rate, 1, 16, &cancel).unwrap();
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
        use crate::playback::create_track_stream_pair_with_capacity;
        use crate::playback::sparse_buffer::create_sparse_buffer;
        use std::thread;

        init();

        // Create test FLAC data
        let samples: Vec<i32> = (0..44100)
            .map(|i| ((i as f64 * 0.01).sin() * 10000.0) as i32)
            .collect();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let flac_data = encode_to_flac(&samples, 44100, 1, 16, &cancel).unwrap();

        // Create streaming infrastructure with sparse buffer
        let buffer = create_sparse_buffer(flac_data.len() as u64);
        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(44100, 1, 100000);

        // Spawn decoder thread using new AVIO-based streaming decode
        let decoder_buffer = buffer.clone();
        let decoder_handle = thread::spawn(move || {
            let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
            decode_audio_streaming(decoder_buffer, &mut sink, None, None, None, None, token)
        });

        // Feed data to buffer (simulating download)
        buffer.append_at(0, &flac_data);

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

    #[test]
    fn test_streaming_decode_treats_cancelled_input_as_normal_stop() {
        use crate::playback::create_track_stream_pair_with_capacity;
        use crate::playback::sparse_buffer::create_sparse_buffer;

        init();

        // Size is irrelevant: the buffer is cancelled before any read.
        let buffer = create_sparse_buffer(4096);
        buffer.cancel();
        let (mut sink, source, _ready) = create_track_stream_pair_with_capacity(44100, 1, 4096);
        let token = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let result = decode_audio_streaming(buffer, &mut sink, None, None, None, None, token);

        assert_eq!(result, Err(StreamingDecodeError::InputCancelled));
        assert!(!source.producer_finished());
        assert_eq!(source.samples_decoded(), 0);
    }

    /// Helper: test that seek_to via avformat_seek_file produces correct samples.
    ///
    /// Same ground truth as check_seek_produces_correct_samples, but uses the
    /// seek_to parameter (ffmpeg-level seek within a full SparseBuffer) instead
    /// of the seektable-based restart approach.
    ///
    /// avformat_seek_file lands on the nearest keyframe AT or BEFORE the target,
    /// so decoded output may start slightly before the requested position.
    /// We find the actual alignment by correlating against ground truth.
    fn check_seek_to_produces_correct_samples(
        sample_rate: u32,
        channels: u32,
        bits_per_sample: u32,
    ) {
        use crate::playback::create_track_stream_pair_with_capacity;
        use crate::playback::sparse_buffer::create_sparse_buffer;
        use std::thread;

        let duration_secs = 2;
        let frame_samples = sample_rate as usize * duration_secs;
        let max_val = (1i32 << (bits_per_sample - 1)) - 1;

        let mut original: Vec<i32> = Vec::with_capacity(frame_samples * channels as usize);
        for i in 0..frame_samples {
            let t = i as f64 / sample_rate as f64;
            let freq = 200.0 + (t / duration_secs as f64) * 1800.0;
            let sample =
                ((max_val as f64 * 0.8) * (2.0 * std::f64::consts::PI * freq * t).sin()) as i32;
            original.push(sample);
            if channels == 2 {
                original.push(-sample);
            }
        }

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let flac_data =
            encode_to_flac(&original, sample_rate, channels, bits_per_sample, &cancel).unwrap();
        let ground_truth = decode_audio(&flac_data, None, None).unwrap();

        let seek_sample = sample_rate as u64; // 1 second

        // Fill a SparseBuffer with the complete file, then seek via seek_to
        let buffer = create_sparse_buffer(flac_data.len() as u64);
        buffer.append_at(0, &flac_data);

        let (mut sink, mut source, _ready) =
            create_track_stream_pair_with_capacity(sample_rate, channels, 500000);
        let decoder_handle = thread::spawn(move || {
            let token = Arc::new(std::sync::atomic::AtomicBool::new(false));
            decode_audio_streaming(
                buffer,
                &mut sink,
                Some(seek_sample),
                None,
                None,
                None,
                token,
            )
        });

        let result = decoder_handle.join().unwrap();
        assert!(result.is_ok(), "seek_to decode failed: {:?}", result.err());

        let mut seeked_samples = Vec::new();
        let mut buf = [0.0f32; 1024];
        loop {
            let n = source.pull_samples(&mut buf);
            if n == 0 && source.is_finished() {
                break;
            }
            seeked_samples.extend_from_slice(&buf[..n]);
        }

        assert!(
            seeked_samples.len() > 100,
            "Not enough seeked samples: {}",
            seeked_samples.len()
        );

        let scale = if bits_per_sample <= 16 {
            i16::MAX as f32
        } else {
            i32::MAX as f32
        };

        // avformat_seek_file lands on a keyframe at or before the target.
        // Find where the seeked output actually starts in the ground truth
        // by correlating a window of samples. Search up to 16384 samples
        // before the target (FLAC blocksize varies by sample rate).
        let target_interleaved = (seek_sample as usize) * (channels as usize);
        let max_search = 16384 * channels as usize;
        let search_start = target_interleaved.saturating_sub(max_search);
        // Ensure channel-aligned
        let search_start = search_start - (search_start % channels as usize);

        // Use a large correlation window for reliable matching
        let window = 512.min(seeked_samples.len());
        let mut best_offset = search_start;
        let mut best_error = f64::MAX;

        for offset in (search_start..=target_interleaved).step_by(channels as usize) {
            if offset + window > ground_truth.samples.len() {
                break;
            }
            let mut err = 0.0f64;
            for (&seeked, &truth) in seeked_samples[..window]
                .iter()
                .zip(&ground_truth.samples[offset..offset + window])
            {
                let diff = seeked as f64 - truth as f64 / scale as f64;
                err += diff * diff;
            }
            if err < best_error {
                best_error = err;
                best_offset = offset;
            }
        }

        // Verify the seek landed within a reasonable distance of the target
        let offset_samples = (target_interleaved - best_offset) / channels as usize;
        assert!(
            offset_samples <= 16384,
            "Seek landed too far from target: {} samples before 1s mark",
            offset_samples,
        );

        // Now compare samples from the aligned position
        let expected_samples = &ground_truth.samples[best_offset..];
        let compare_len = 1000.min(seeked_samples.len()).min(expected_samples.len());
        assert!(compare_len > 100, "Not enough samples: {}", compare_len);

        let mut mismatches = 0;
        let mut max_diff = 0.0f32;
        let mut first_mismatch_idx = None;

        for i in 0..compare_len {
            let seeked = seeked_samples[i];
            let expected = expected_samples[i] as f32 / scale;
            let diff = (seeked - expected).abs();

            if diff > 0.01 {
                mismatches += 1;
                if first_mismatch_idx.is_none() {
                    first_mismatch_idx = Some(i);
                }
                max_diff = max_diff.max(diff);
            }
        }

        if mismatches > 0 {
            let first_idx = first_mismatch_idx.unwrap();
            panic!(
                "seek_to mismatch! {}Hz {}ch {}bit (aligned at {} samples before target)\n\
                 First mismatch at {}: seeked={:.4}, expected={:.4}\n\
                 Mismatches: {}/{}, max_diff: {:.4}",
                sample_rate,
                channels,
                bits_per_sample,
                offset_samples,
                first_idx,
                seeked_samples[first_idx],
                expected_samples[first_idx] as f32 / scale,
                mismatches,
                compare_len,
                max_diff,
            );
        }
    }

    #[test]
    fn test_seek_to_mono_44100_16bit() {
        init();
        check_seek_to_produces_correct_samples(44100, 1, 16);
    }

    #[test]
    fn test_seek_to_stereo_44100_24bit() {
        init();
        check_seek_to_produces_correct_samples(44100, 2, 24);
    }

    #[test]
    fn test_seek_to_stereo_96000_16bit() {
        init();
        check_seek_to_produces_correct_samples(96000, 2, 16);
    }
}
