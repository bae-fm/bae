//! Decoding any audio format to PCM: the in-memory `decode_audio` path
//! (returns or streams interleaved i32 samples) and the `SparseBuffer`-backed
//! `decode_audio_streaming` loop (pushes f32 samples to a `TrackSink`), plus
//! the per-thread FFmpeg fatal-error counter that tracks decode failures.

use super::avio::{
    avio_read_callback, avio_seek_callback, streaming_avio_read_callback,
    streaming_avio_seek_callback, AvioContext, StreamingAvioContext,
};
use super::{av_err_str, DecodedAudio, DecodedSink, StreamingDecodeError, AVIO_BUFFER_SIZE};
use crate::playback::{SharedSparseBuffer, TrackSink};
use std::cell::Cell;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

// Thread-local FFmpeg error counter for per-decode error tracking
thread_local! {
    // clippy::missing_const_for_thread_local fires spuriously on the Android target
    // even though `= const { ... }` is already the const form; suppress the false positive.
    #[allow(clippy::missing_const_for_thread_local)]
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

/// Decode any audio format to PCM samples.
///
/// If start_ms/end_ms are provided, only that time range is decoded.
/// Returns interleaved i32 samples.
pub fn decode_audio(
    data: &[u8],
    start_sample: Option<u64>,
    end_sample: Option<u64>,
) -> Result<DecodedAudio, String> {
    let mut sink = CollectingSink {
        samples: Vec::new(),
        format: None,
    };
    decode_audio_to_sink(data, start_sample, end_sample, &mut sink)?;
    // `decode_audio_to_sink` returning Ok means the format was probed, so
    // `on_format` ran before any samples.
    let (sample_rate, channels, bits_per_sample) = sink
        .format
        .expect("decode produced samples without a format");
    Ok(DecodedAudio {
        samples: sink.samples,
        sample_rate,
        channels,
        bits_per_sample,
    })
}

/// The sink behind `decode_audio`: collects the whole decode into a `Vec` and
/// records the probed format.
struct CollectingSink {
    samples: Vec<i32>,
    format: Option<(u32, u32, u32)>,
}

impl DecodedSink for CollectingSink {
    fn on_format(&mut self, sample_rate: u32, channels: u32, bits_per_sample: u32) {
        self.format = Some((sample_rate, channels, bits_per_sample));
    }
    fn on_samples(&mut self, samples: &[i32]) {
        self.samples.extend_from_slice(samples);
    }
}

/// Stream-decode audio, pushing the format then each interleaved-i32 chunk into
/// `sink` instead of returning a buffered `Vec`. Same window semantics as
/// [`decode_audio`]; `start_sample`/`end_sample` trim the decoded range.
pub fn decode_audio_to_sink(
    data: &[u8],
    start_sample: Option<u64>,
    end_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
) -> Result<(), String> {
    // Safety: FFmpeg operations are contained within this function
    unsafe { decode_audio_avio(data, start_sample, end_sample, sink) }
}

/// Internal AVIO-based decode implementation
unsafe fn decode_audio_avio(
    data: &[u8],
    start_sample: Option<u64>,
    end_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    // Count fatal FFmpeg errors for this decode, so the sink can flag a track
    // whose audio bytes fail to decode (import decode-verify rides this pass).
    // The counter is reset after `find_stream_info` below, not here: probing an
    // attached-picture stream (an embedded cover) emits its own fatal-level logs
    // that say nothing about the audio, so only errors from the audio decode loop
    // should count.
    install_ffmpeg_log_callback();

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

    // Only count fatal errors from the audio decode below -- probing an embedded
    // cover during `find_stream_info` above may have logged its own.
    reset_ffmpeg_errors();

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
    let codec_id = (*codecpar).codec_id;
    let codec = find_audio_decoder(codec_id);
    if codec.is_null() {
        if codec_id == AVCodecID::AV_CODEC_ID_PCM_S64LE {
            let result = decode_packed_s64_packets_to_sink(
                fmt_ctx,
                stream_index,
                stream,
                codecpar,
                (*codecpar).sample_rate as u32,
                (*codecpar).ch_layout.nb_channels as u32,
                start_sample,
                end_sample,
                sink,
            );
            avformat_close_input(&mut fmt_ctx);
            drop(avio_ctx);
            sink.set_decode_error_count(get_ffmpeg_errors());
            return result;
        }
        avformat_close_input(&mut fmt_ctx);
        return Err(format!("Decoder not found for {:?}", codec_id));
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

    sink.on_format(sample_rate, channels, 32);

    let in_ch_layout = (*codecpar).ch_layout;
    let mut swr_ctx = match allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_S32,
        sample_rate,
        (*codec_ctx).sample_fmt,
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            avcodec_free_context(&mut (codec_ctx as *mut _));
            avformat_close_input(&mut fmt_ctx);
            return Err(e);
        }
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
        swr_free(&mut swr_ctx);
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        return Err("Failed to allocate frame/packet".to_string());
    }

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

            let frame_samples_vec = match convert_frame_to_i32(swr_ctx, frame, channels as usize) {
                Ok(samples) => samples,
                Err(e) => {
                    av_frame_free(&mut (frame as *mut _));
                    av_packet_free(&mut (packet as *mut _));
                    swr_free(&mut swr_ctx);
                    avcodec_free_context(&mut (codec_ctx as *mut _));
                    avformat_close_input(&mut fmt_ctx);
                    return Err(e);
                }
            };

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
                sink.on_samples(&frame_samples_vec[skip_start..take_end]);
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
            let frame_samples_vec = match convert_frame_to_i32(swr_ctx, frame, channels as usize) {
                Ok(samples) => samples,
                Err(e) => {
                    av_frame_free(&mut (frame as *mut _));
                    av_packet_free(&mut (packet as *mut _));
                    swr_free(&mut swr_ctx);
                    avcodec_free_context(&mut (codec_ctx as *mut _));
                    avformat_close_input(&mut fmt_ctx);
                    return Err(e);
                }
            };
            sink.on_samples(&frame_samples_vec);
        }
    }

    // Cleanup
    av_frame_free(&mut (frame as *mut _));
    av_packet_free(&mut (packet as *mut _));
    swr_free(&mut swr_ctx);
    avcodec_free_context(&mut (codec_ctx as *mut _));
    avformat_close_input(&mut fmt_ctx);
    // Note: avformat_close_input frees the AVIO context and buffer

    // Keep avio_ctx alive until here (prevent drop during FFmpeg operations)
    drop(avio_ctx);

    // Report the fatal-error count so a verifying sink can flag a broken decode.
    sink.set_decode_error_count(get_ffmpeg_errors());

    Ok(())
}

unsafe fn allocate_swr_context(
    ch_layout: *const ffmpeg_sys_next::AVChannelLayout,
    out_sample_fmt: ffmpeg_sys_next::AVSampleFormat,
    sample_rate: u32,
    in_sample_fmt: ffmpeg_sys_next::AVSampleFormat,
) -> Result<*mut ffmpeg_sys_next::SwrContext, String> {
    use ffmpeg_sys_next::*;

    let mut swr_ctx: *mut SwrContext = ptr::null_mut();
    let ret = swr_alloc_set_opts2(
        &mut swr_ctx,
        ch_layout,
        out_sample_fmt,
        sample_rate as c_int,
        ch_layout,
        in_sample_fmt,
        sample_rate as c_int,
        0,
        ptr::null_mut(),
    );
    if ret < 0 || swr_ctx.is_null() {
        return Err(format!(
            "Failed to allocate SwrContext: {}",
            av_err_str(ret)
        ));
    }

    let ret = swr_init(swr_ctx);
    if ret < 0 {
        swr_free(&mut swr_ctx);
        return Err(format!("Failed to init SwrContext: {}", av_err_str(ret)));
    }

    Ok(swr_ctx)
}

unsafe fn convert_frame_to_i32(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    frame: *const ffmpeg_sys_next::AVFrame,
    channels: usize,
) -> Result<Vec<i32>, String> {
    use ffmpeg_sys_next::*;

    let input_samples = (*frame).nb_samples;
    let mut output_buf = vec![0i32; input_samples as usize * channels];
    let out_ptr = output_buf.as_mut_ptr() as *mut u8;
    let converted = swr_convert(
        swr_ctx,
        &out_ptr,
        input_samples,
        (*frame).extended_data as *const *const u8,
        input_samples,
    );
    if converted < 0 {
        return Err(format!(
            "Failed to convert decoded frame: {}",
            av_err_str(converted)
        ));
    }
    output_buf.truncate(converted as usize * channels);
    Ok(output_buf)
}

unsafe fn decode_packed_s64_packets_to_sink(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    stream: *mut ffmpeg_sys_next::AVStream,
    codecpar: *const ffmpeg_sys_next::AVCodecParameters,
    sample_rate: u32,
    channels: u32,
    start_sample: Option<u64>,
    end_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    sink.on_format(sample_rate, channels, 32);
    let in_ch_layout = (*codecpar).ch_layout;
    let mut swr_ctx = allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_S32,
        sample_rate,
        AVSampleFormat::AV_SAMPLE_FMT_S64,
    )?;

    if let Some(sample_pos) = start_sample {
        av_seek_frame(
            fmt_ctx,
            stream_index,
            sample_pos as i64,
            AVSEEK_FLAG_BACKWARD as c_int,
        );
    }

    let packet = av_packet_alloc();
    if packet.is_null() {
        swr_free(&mut swr_ctx);
        return Err("Failed to allocate packet".to_string());
    }

    let time_base = (*stream).time_base;
    let channels_usize = channels as usize;
    let bytes_per_frame = channels_usize * std::mem::size_of::<i64>();
    let mut tracked_sample_pos: i64 = 0;
    let mut reached_end = false;

    while av_read_frame(fmt_ctx, packet) >= 0 {
        if (*packet).stream_index != stream_index {
            av_packet_unref(packet);
            continue;
        }

        let num_samples = (*packet).size as usize / bytes_per_frame;
        let pts = (*packet).pts;
        let frame_start = if pts != AV_NOPTS_VALUE {
            if time_base.num == 1 && time_base.den == sample_rate as c_int {
                pts
            } else {
                (pts as f64 * time_base.num as f64 / time_base.den as f64 * sample_rate as f64)
                    as i64
            }
        } else {
            tracked_sample_pos
        };
        let frame_end = frame_start + num_samples as i64;
        tracked_sample_pos = frame_end;

        if let Some(start) = start_sample {
            let start = start as i64;
            if frame_end <= start {
                av_packet_unref(packet);
                continue;
            }
        }

        if let Some(end) = end_sample {
            let end = end as i64;
            if frame_start >= end {
                av_packet_unref(packet);
                break;
            }
        }

        let packet_samples =
            match convert_packed_s64_to_i32(swr_ctx, (*packet).data, num_samples, channels_usize) {
                Ok(samples) => samples,
                Err(e) => {
                    av_packet_unref(packet);
                    av_packet_free(&mut (packet as *mut _));
                    swr_free(&mut swr_ctx);
                    return Err(e);
                }
            };

        let skip_start = if let Some(start) = start_sample {
            let start = start as i64;
            if frame_start < start {
                ((start - frame_start) as usize * channels_usize).min(packet_samples.len())
            } else {
                0
            }
        } else {
            0
        };

        let take_end = if let Some(end) = end_sample {
            let end = end as i64;
            if frame_end > end {
                reached_end = true;
                ((end - frame_start) as usize * channels_usize).min(packet_samples.len())
            } else {
                packet_samples.len()
            }
        } else {
            packet_samples.len()
        };

        if skip_start < take_end {
            sink.on_samples(&packet_samples[skip_start..take_end]);
        }

        av_packet_unref(packet);
        if reached_end {
            break;
        }
    }

    av_packet_free(&mut (packet as *mut _));
    swr_free(&mut swr_ctx);
    Ok(())
}

unsafe fn convert_packed_s64_to_i32(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    input: *const u8,
    input_samples: usize,
    channels: usize,
) -> Result<Vec<i32>, String> {
    use ffmpeg_sys_next::*;

    let mut output_buf = vec![0i32; input_samples * channels];
    let out_ptr = output_buf.as_mut_ptr() as *mut u8;
    let in_ptr = input;
    let converted = swr_convert(
        swr_ctx,
        &out_ptr,
        input_samples as c_int,
        &in_ptr,
        input_samples as c_int,
    );
    if converted < 0 {
        return Err(format!(
            "Failed to convert decoded frame: {}",
            av_err_str(converted)
        ));
    }
    output_buf.truncate(converted as usize * channels);
    Ok(output_buf)
}

unsafe fn find_audio_decoder(
    codec_id: ffmpeg_sys_next::AVCodecID,
) -> *const ffmpeg_sys_next::AVCodec {
    use ffmpeg_sys_next::*;

    let codec = avcodec_find_decoder(codec_id);
    if !codec.is_null() {
        return codec;
    }

    if codec_id == AVCodecID::AV_CODEC_ID_PCM_S64LE {
        avcodec_find_decoder_by_name(c"pcm_s64le".as_ptr())
    } else {
        ptr::null()
    }
}

// =============================================================================
// AVIO-based Streaming Decode
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
    seek_to_byte: Option<u64>,
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
            seek_to_byte,
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
    seek_to_byte: Option<u64>,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    end_byte: Option<u64>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), StreamingDecodeError> {
    use ffmpeg_sys_next::*;

    // Wall-clock origin for the first-sample latency log below: spans the probe
    // (open_input + find_stream_info), the seek, and the decode up to the first
    // audio sample -- the whole "how long before playback starts" window.
    let decode_start = Instant::now();

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
    // Make the demuxer load its seek index from the file's own seektable. FLAC
    // (and MP3) only populate that index when this flag is set; without it,
    // avformat_seek_file ignores the seektable and binary-searches the file --
    // reading the end, then bisecting frame by frame. Over a cloud home each of
    // those reads is a separate ranged fetch (~15 of them, ~1s each), so a single
    // track start or in-track seek stalls for ~10s. With the flag, a
    // seektable-bearing FLAC seeks in one fetch. No-op for APE/MP4/WAV, which are
    // already indexed (see notes/ffmpeg-seek-behavior.md).
    (*fmt_ctx).flags |= AVFMT_FLAG_FAST_SEEK as c_int;

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
    let codec_id = (*codecpar).codec_id;
    let codec = find_audio_decoder(codec_id);
    if codec.is_null() {
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(format!(
            "Decoder not found for {:?}",
            codec_id
        )));
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
    let mut swr_ctx = allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_FLT,
        sample_rate,
        (*codec_ctx).sample_fmt,
    )
    .map_err(|e| {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        avformat_close_input(&mut fmt_ctx);
        let _ = Box::from_raw(avio_ctx_ptr);
        StreamingDecodeError::decode(e)
    })?;

    // Seek to the target sample if requested. A seektable-bearing FLAC (and APE,
    // MP4) seeks in one jump now that AVFMT_FLAG_FAST_SEEK is set. A FLAC with no
    // seektable can't: over a streaming buffer the binary-search fallback would
    // have to read the file's end before it's fetched, so the seek fails. Decoding
    // from the start (then trimming to the target) is the only way to play such a
    // file -- correct here, but wasteful over a cloud home, which is why every
    // CUE/FLAC should carry a seektable (issue #226). Keep the bail-out logged.
    //
    // A by-byte seek (AVSEEK_FLAG_BYTE) jumps straight to a known frame offset --
    // no seektable consulted, no binary search reading the file's end -- and the
    // landed frame's sample is read from its header, so the `start_at_sample`
    // trim below still reaches the exact sample. Used for FLAC, where the frame
    // byte is recorded at import; APE has no per-frame byte positions, so it
    // sample-seeks via its mandatory index instead.
    if let Some(byte_pos) = seek_to_byte {
        let ret = av_seek_frame(
            fmt_ctx,
            stream_index,
            byte_pos as i64,
            AVSEEK_FLAG_BYTE as c_int,
        );
        if ret < 0 {
            warn!(
                "byte seek to {byte_pos} failed ({}); decoding from the start",
                av_err_str(ret)
            );
        } else {
            avcodec_flush_buffers(codec_ctx);
        }
    } else if let Some(sample_pos) = seek_to_sample {
        let target_ts = sample_pos as i64;
        let ret = avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target_ts, target_ts, 0);
        if ret < 0 {
            warn!(
                "avformat_seek_file to sample {sample_pos} failed ({}); \
                 decoding from the start (file has no usable seektable?)",
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
    let mut first_sample_logged = false;

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
            let converted = swr_convert(
                swr_ctx,
                &out_ptr,
                num_samples as c_int,
                (*frame).extended_data as *const *const u8,
                (*frame).nb_samples,
            );
            if converted < 0 {
                av_frame_free(&mut (frame as *mut _));
                av_packet_free(&mut (packet as *mut _));
                swr_free(&mut swr_ctx);
                avcodec_free_context(&mut (codec_ctx as *mut _));
                avformat_close_input(&mut fmt_ctx);
                let _ = Box::from_raw(avio_ctx_ptr);
                return Err(StreamingDecodeError::decode(format!(
                    "Failed to convert decoded frame: {}",
                    av_err_str(converted)
                )));
            }
            output_buf.truncate(converted as usize * channels as usize);

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

            if !samples_to_output.is_empty() {
                if !first_sample_logged {
                    first_sample_logged = true;
                    info!(
                        "first audio sample: buffer={} fetched {}B from coven in {}ms",
                        buffer.id(),
                        buffer.bytes_fetched(),
                        decode_start.elapsed().as_millis(),
                    );
                }
                if !push_samples_to_sink(sink, samples_to_output) {
                    break;
                }
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
            let converted = swr_convert(
                swr_ctx,
                &out_ptr,
                num_samples as c_int,
                (*frame).extended_data as *const *const u8,
                (*frame).nb_samples,
            );
            if converted < 0 {
                av_frame_free(&mut (frame as *mut _));
                av_packet_free(&mut (packet as *mut _));
                swr_free(&mut swr_ctx);
                avcodec_free_context(&mut (codec_ctx as *mut _));
                avformat_close_input(&mut fmt_ctx);
                let _ = Box::from_raw(avio_ctx_ptr);
                return Err(StreamingDecodeError::decode(format!(
                    "Failed to convert decoded frame: {}",
                    av_err_str(converted)
                )));
            }
            output_buf.truncate(converted as usize * channels as usize);

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
