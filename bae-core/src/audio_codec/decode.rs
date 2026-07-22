//! Decoding any audio format to PCM. Every decode reads its compressed bytes
//! from a `SparseStreamingBuffer` — the only way bytes reach a decoder — through
//! one shared packet/frame/trim loop with two output stages: interleaved i32
//! into a `DecodedSink` (`decode_audio_to_sink`, used by loudness and save) and
//! packed f32 into a `TrackSink` (`decode_audio_streaming`, used by playback).
//! Also home to the per-thread FFmpeg fatal-error counter that tracks decode
//! failures.

use super::avio::{
    close_input_and_free_custom_avio, free_custom_avio_context, free_format_and_custom_avio,
    streaming_avio_read_callback, streaming_avio_seek_callback, StreamingAvioContext,
};
use super::{av_err_str, DecodedAudio, DecodedSink, StreamingDecodeError, AVIO_BUFFER_SIZE};
use crate::playback::{SharedSparseBuffer, TrackSink};
use std::cell::Cell;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

const CHANNEL_COUNT_ERROR: &str = "channel count must be greater than zero";

// One decode runs on one thread, so a thread-local counts its FFmpeg errors.
thread_local! {
    // clippy::missing_const_for_thread_local fires spuriously on the Android target
    // even though `= const { ... }` is already the const form; suppress the false positive.
    #[allow(clippy::missing_const_for_thread_local)]
    static FFMPEG_DECODE_ERRORS: Cell<u32> = const { Cell::new(0) };
}

fn reset_ffmpeg_errors() {
    FFMPEG_DECODE_ERRORS.with(|c| c.set(0));
}

fn get_ffmpeg_errors() -> u32 {
    FFMPEG_DECODE_ERRORS.with(|c| c.get())
}

unsafe fn free_decode_resources(
    frame: &mut *mut ffmpeg_sys_next::AVFrame,
    packet: &mut *mut ffmpeg_sys_next::AVPacket,
    swr_ctx: &mut *mut ffmpeg_sys_next::SwrContext,
    codec_ctx: &mut *mut ffmpeg_sys_next::AVCodecContext,
    fmt_ctx: &mut *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
) {
    ffmpeg_sys_next::av_frame_free(frame);
    ffmpeg_sys_next::av_packet_free(packet);
    ffmpeg_sys_next::swr_free(swr_ctx);
    ffmpeg_sys_next::avcodec_free_context(codec_ctx);
    close_input_and_free_custom_avio(fmt_ctx, avio);
}

struct DecodeResources {
    frame: *mut ffmpeg_sys_next::AVFrame,
    packet: *mut ffmpeg_sys_next::AVPacket,
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    codec_ctx: *mut ffmpeg_sys_next::AVCodecContext,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
}

impl Drop for DecodeResources {
    fn drop(&mut self) {
        unsafe {
            free_decode_resources(
                &mut self.frame,
                &mut self.packet,
                &mut self.swr_ctx,
                &mut self.codec_ctx,
                &mut self.fmt_ctx,
                self.avio,
            );
        }
    }
}

struct BufferDecodeResources {
    core: Option<DecodeResources>,
    avio_ctx_ptr: *mut StreamingAvioContext,
}

impl BufferDecodeResources {
    fn core(&self) -> &DecodeResources {
        self.core
            .as_ref()
            .expect("streaming decode resources missing core resources")
    }
}

impl Drop for BufferDecodeResources {
    fn drop(&mut self) {
        if let Some(core) = self.core.take() {
            drop(core);
        }
        unsafe {
            let _ = Box::from_raw(self.avio_ctx_ptr);
        }
    }
}

struct AudioStreamInfo {
    stream_index: c_int,
    stream: *mut ffmpeg_sys_next::AVStream,
    codecpar: *mut ffmpeg_sys_next::AVCodecParameters,
}

enum InputOpenProbeError {
    Open(i32),
    Probe(i32),
}

enum DecodeCodecOpenError {
    Missing(ffmpeg_sys_next::AVCodecID),
    Allocate,
    CopyParams(i32),
    Open(i32),
}

impl DecodeCodecOpenError {
    fn message(&self) -> String {
        match self {
            Self::Missing(codec_id) => format!("Decoder not found for {:?}", codec_id),
            Self::Allocate => "Failed to allocate codec context".to_string(),
            Self::CopyParams(ret) => format!("Failed to copy codec params: {}", av_err_str(*ret)),
            Self::Open(ret) => format!("Failed to open codec: {}", av_err_str(*ret)),
        }
    }
}

enum ProbedAudioCodecOpenError {
    MissingStream(String),
    Codec {
        audio: AudioStreamInfo,
        error: DecodeCodecOpenError,
    },
}

unsafe fn allocate_input_format_context<F>(
    avio: *mut ffmpeg_sys_next::AVIOContext,
    configure: F,
) -> Result<*mut ffmpeg_sys_next::AVFormatContext, ()>
where
    F: FnOnce(*mut ffmpeg_sys_next::AVFormatContext),
{
    let fmt_ctx = ffmpeg_sys_next::avformat_alloc_context();
    if fmt_ctx.is_null() {
        free_custom_avio_context(avio);
        return Err(());
    }
    (*fmt_ctx).pb = avio;
    configure(fmt_ctx);
    Ok(fmt_ctx)
}

unsafe fn open_and_probe_input(
    fmt_ctx: &mut *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
) -> Result<(), InputOpenProbeError> {
    use ffmpeg_sys_next::*;

    debug_assert_eq!((**fmt_ctx).pb, avio);

    let ret = avformat_open_input(fmt_ctx, ptr::null(), ptr::null_mut(), ptr::null_mut());
    if ret < 0 {
        return Err(InputOpenProbeError::Open(ret));
    }

    let ret = avformat_find_stream_info(*fmt_ctx, ptr::null_mut());
    if ret < 0 {
        return Err(InputOpenProbeError::Probe(ret));
    }

    Ok(())
}

unsafe fn open_probed_audio_codec(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
) -> Result<(AudioStreamInfo, *mut ffmpeg_sys_next::AVCodecContext), ProbedAudioCodecOpenError> {
    let audio = find_audio_stream(fmt_ctx).map_err(ProbedAudioCodecOpenError::MissingStream)?;
    match open_audio_codec(audio.codecpar) {
        Ok(codec_ctx) => Ok((audio, codec_ctx)),
        Err(error) => Err(ProbedAudioCodecOpenError::Codec { audio, error }),
    }
}

unsafe fn find_audio_stream(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
) -> Result<AudioStreamInfo, String> {
    use ffmpeg_sys_next::*;

    let stream_index = av_find_best_stream(
        fmt_ctx,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );
    if stream_index < 0 {
        return Err("No audio stream found".to_string());
    }

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let codecpar = (*stream).codecpar;
    Ok(AudioStreamInfo {
        stream_index,
        stream,
        codecpar,
    })
}

unsafe fn open_audio_codec(
    codecpar: *const ffmpeg_sys_next::AVCodecParameters,
) -> Result<*mut ffmpeg_sys_next::AVCodecContext, DecodeCodecOpenError> {
    use ffmpeg_sys_next::*;

    let codec_id = (*codecpar).codec_id;
    let codec = avcodec_find_decoder(codec_id);
    if codec.is_null() {
        return Err(DecodeCodecOpenError::Missing(codec_id));
    }

    let codec_ctx = avcodec_alloc_context3(codec);
    if codec_ctx.is_null() {
        return Err(DecodeCodecOpenError::Allocate);
    }

    let ret = avcodec_parameters_to_context(codec_ctx, codecpar);
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(DecodeCodecOpenError::CopyParams(ret));
    }

    let ret = avcodec_open2(codec_ctx, codec, ptr::null_mut());
    if ret < 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        return Err(DecodeCodecOpenError::Open(ret));
    }

    Ok(codec_ctx)
}

#[derive(Debug, PartialEq, Eq)]
enum FrameOutputWindow {
    Skip,
    Stop,
    Emit {
        skip_start: usize,
        take_end: usize,
        reached_end: bool,
    },
}

fn pts_to_sample(pts: i64, time_base: ffmpeg_sys_next::AVRational, sample_rate: u32) -> i64 {
    if time_base.num == 1 && time_base.den == sample_rate as c_int {
        pts
    } else {
        (pts as f64 * time_base.num as f64 / time_base.den as f64 * sample_rate as f64) as i64
    }
}

fn frame_sample_bounds(
    pts: i64,
    num_samples: usize,
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: u32,
    tracked_sample_pos: &mut i64,
) -> (i64, i64) {
    let frame_start = if pts != ffmpeg_sys_next::AV_NOPTS_VALUE {
        let sample_pos = pts_to_sample(pts, time_base, sample_rate);
        *tracked_sample_pos = sample_pos;
        sample_pos
    } else if *tracked_sample_pos >= 0 {
        *tracked_sample_pos
    } else {
        -1
    };
    let frame_end = frame_start + num_samples as i64;
    if *tracked_sample_pos >= 0 {
        *tracked_sample_pos = frame_end;
    }
    (frame_start, frame_end)
}

fn frame_output_window(
    frame_start: i64,
    frame_end: i64,
    output_len: usize,
    channels: usize,
    start_sample: Option<u64>,
    end_sample: Option<u64>,
) -> FrameOutputWindow {
    let mut skip_start = 0;
    let mut take_end = output_len;

    if let Some(start) = start_sample {
        let start = start as i64;
        if frame_start >= 0 && frame_end <= start {
            return FrameOutputWindow::Skip;
        }
        if frame_start >= 0 && frame_start < start {
            skip_start = ((start - frame_start) as usize * channels).min(output_len);
        }
    }

    let mut reached_end = false;
    if let Some(end) = end_sample {
        let end = end as i64;
        if frame_start >= 0 && frame_start >= end {
            return FrameOutputWindow::Stop;
        }
        if frame_start >= 0 && frame_end > end {
            take_end = ((end - frame_start) as usize * channels).min(output_len);
            reached_end = true;
        }
    }

    if skip_start < take_end {
        FrameOutputWindow::Emit {
            skip_start,
            take_end,
            reached_end,
        }
    } else if reached_end {
        FrameOutputWindow::Stop
    } else {
        FrameOutputWindow::Skip
    }
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

/// Counts FFmpeg's fatal errors on the decoding thread.
unsafe extern "C" fn ffmpeg_log_callback(
    _avcl: *mut c_void,
    level: c_int,
    _fmt: *const std::ffi::c_char,
    _vl: FfmpegVaList,
) {
    // AV_LOG_FATAL (8) and AV_LOG_PANIC (0) only. AV_LOG_ERROR (16) includes
    // recoverable sync errors during seeking, which aren't decode failures.
    if level <= 8 {
        FFMPEG_DECODE_ERRORS.with(|c| c.set(c.get() + 1));
    }
}

fn install_ffmpeg_log_callback() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        ffmpeg_sys_next::av_log_set_callback(Some(ffmpeg_log_callback));
    });
}

/// A probed demuxer over a sparse buffer: the format context wired to a
/// streaming AVIO whose reads block until the buffer's fill delivers the
/// bytes. `avio_ctx_ptr` owns the reader; it is freed by the caller (via
/// `BufferDecodeResources` or explicitly on an error path).
struct BufferInput {
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
    avio_ctx_ptr: *mut StreamingAvioContext,
}

enum BufferInputError {
    Alloc(&'static str),
    Open(i32),
    Probe(i32),
}

impl BufferInputError {
    fn message(&self) -> String {
        match self {
            Self::Alloc(what) => (*what).to_string(),
            Self::Open(ret) => format!("Failed to open input: {}", av_err_str(*ret)),
            Self::Probe(ret) => format!("Failed to find stream info: {}", av_err_str(*ret)),
        }
    }
}

/// Build the streaming AVIO over `buffer` and open + probe the demuxer on it.
/// On error, everything allocated so far — including the AVIO context box — is
/// freed before returning.
unsafe fn open_buffer_input(
    buffer: &SharedSparseBuffer,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
) -> Result<BufferInput, BufferInputError> {
    use ffmpeg_sys_next::*;

    let reader = buffer.new_reader_with_cancel(cancel_token.clone());
    let avio_ctx = Box::new(StreamingAvioContext {
        reader: std::sync::Mutex::new(reader),
        buffer: buffer.clone(),
        cancel_token,
    });
    let avio_ctx_ptr = Box::into_raw(avio_ctx);

    // FFmpeg takes ownership of this buffer.
    let avio_buffer = av_malloc(AVIO_BUFFER_SIZE) as *mut u8;
    if avio_buffer.is_null() {
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(BufferInputError::Alloc("Failed to allocate AVIO buffer"));
    }

    let avio = avio_alloc_context(
        avio_buffer,
        AVIO_BUFFER_SIZE as c_int,
        0,
        avio_ctx_ptr as *mut c_void,
        Some(streaming_avio_read_callback),
        None,
        Some(streaming_avio_seek_callback),
    );
    if avio.is_null() {
        av_free(avio_buffer as *mut c_void);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(BufferInputError::Alloc("Failed to create AVIO context"));
    }

    // Without this, `avformat_seek_file` refuses to seek the stream at all.
    (*avio).seekable = AVIO_SEEKABLE_NORMAL as c_int;

    let Ok(mut fmt_ctx) = allocate_input_format_context(avio, |fmt_ctx| {
        // Make the demuxer load its seek index from the file's own seektable.
        // FLAC (and MP3) only populate that index when this flag is set; without
        // it, avformat_seek_file ignores the seektable and binary-searches the
        // file -- reading the end, then bisecting frame by frame. Over a cloud
        // home each of those reads is a separate ranged fetch (~15 of them, ~1s
        // each), so a single track start or in-track seek stalls for ~10s. With
        // the flag, a seektable-bearing FLAC seeks in one fetch. No-op for
        // APE/MP4/WAV, which are already indexed (see
        // notes/ffmpeg-seek-behavior.md).
        (*fmt_ctx).flags |= AVFMT_FLAG_FAST_SEEK as c_int;
    }) else {
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(BufferInputError::Alloc("Failed to allocate format context"));
    };

    if let Err(e) = open_and_probe_input(&mut fmt_ctx, avio) {
        let error = match e {
            InputOpenProbeError::Open(ret) => {
                free_format_and_custom_avio(fmt_ctx, avio);
                BufferInputError::Open(ret)
            }
            InputOpenProbeError::Probe(ret) => {
                close_input_and_free_custom_avio(&mut fmt_ctx, avio);
                BufferInputError::Probe(ret)
            }
        };
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(error);
    }

    Ok(BufferInput {
        fmt_ctx,
        avio,
        avio_ctx_ptr,
    })
}

/// The output stage of the shared decode loop: converts each decoded frame to
/// its sample type and pushes the trimmed window to its consumer.
trait FrameOutput {
    /// Whether the consumer is cancelled; checked between packets and frames.
    fn cancelled(&self) -> bool;

    /// Convert `frame` into the output's sample type, holding the result
    /// internally, and return its interleaved length (the window trim needs the
    /// length before the push).
    ///
    /// # Safety
    /// `swr_ctx` and `frame` must be valid, matching FFmpeg pointers.
    unsafe fn convert(
        &mut self,
        swr_ctx: *mut ffmpeg_sys_next::SwrContext,
        frame: *const ffmpeg_sys_next::AVFrame,
        channels: usize,
    ) -> Result<usize, String>;

    /// Push `[skip_start, take_end)` of the converted frame. `false` means the
    /// consumer stopped (a cancelled sink); the loop then exits without
    /// flushing.
    fn push(&mut self, skip_start: usize, take_end: usize) -> bool;
}

/// i32 → `DecodedSink` output stage (loudness measurement, save re-encode,
/// tests). Cancellation comes from the caller's token.
struct SinkOutput<'a> {
    sink: &'a mut dyn DecodedSink,
    cancel: &'a std::sync::atomic::AtomicBool,
    converted: Vec<i32>,
}

impl FrameOutput for SinkOutput<'_> {
    fn cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }

    unsafe fn convert(
        &mut self,
        swr_ctx: *mut ffmpeg_sys_next::SwrContext,
        frame: *const ffmpeg_sys_next::AVFrame,
        channels: usize,
    ) -> Result<usize, String> {
        self.converted = convert_frame_to_i32(swr_ctx, frame, channels)?;
        Ok(self.converted.len())
    }

    fn push(&mut self, skip_start: usize, take_end: usize) -> bool {
        self.sink.on_samples(&self.converted[skip_start..take_end]);
        true
    }
}

/// f32 → `TrackSink` output stage (playback): chunked pushes that re-check
/// cancellation, plus the first-sample latency log and the output tally.
struct TrackOutput<'a> {
    sink: &'a mut TrackSink,
    converted: Vec<f32>,
    samples_output: u64,
    first_sample_logged: bool,
    buffer: &'a SharedSparseBuffer,
    decode_start: Instant,
}

impl FrameOutput for TrackOutput<'_> {
    fn cancelled(&self) -> bool {
        self.sink.is_cancelled()
    }

    unsafe fn convert(
        &mut self,
        swr_ctx: *mut ffmpeg_sys_next::SwrContext,
        frame: *const ffmpeg_sys_next::AVFrame,
        channels: usize,
    ) -> Result<usize, String> {
        self.converted = convert_frame_to_f32(swr_ctx, frame, channels)?;
        Ok(self.converted.len())
    }

    fn push(&mut self, skip_start: usize, take_end: usize) -> bool {
        let samples = &self.converted[skip_start..take_end];
        self.samples_output += samples.len() as u64;
        if samples.is_empty() {
            return true;
        }
        if !self.first_sample_logged {
            self.first_sample_logged = true;
            info!(
                "first audio sample: buffer={} fetched {}B from coven in {}ms",
                self.buffer.id(),
                self.buffer.bytes_fetched(),
                self.decode_start.elapsed().as_millis(),
            );
        }
        push_samples_to_sink(self.sink, samples)
    }
}

/// The shared packet/frame/trim loop both decode paths run: read packets for
/// the audio stream, decode frames, trim each to `[start_sample, stop_sample)`
/// via `frame_output_window`, and push the window into `out`. Flushes the
/// decoder afterwards unless the window's end was reached (flushing past it
/// would emit samples past the window) or the consumer stopped.
unsafe fn run_decode_loop(
    res: &DecodeResources,
    stream_index: c_int,
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: u32,
    channels: usize,
    start_sample: Option<u64>,
    stop_sample: Option<u64>,
    out: &mut dyn FrameOutput,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    let mut tracked_sample_pos: i64 = -1;
    let mut reached_stop = false;
    let mut consumer_stopped = false;

    'packets: while av_read_frame(res.fmt_ctx, res.packet) >= 0 {
        if out.cancelled() || reached_stop {
            av_packet_unref(res.packet);
            break;
        }

        if (*res.packet).stream_index != stream_index {
            av_packet_unref(res.packet);
            continue;
        }

        let ret = avcodec_send_packet(res.codec_ctx, res.packet);
        av_packet_unref(res.packet);

        if ret < 0 {
            return Err(format!(
                "Failed to send packet to decoder: {}",
                av_err_str(ret)
            ));
        }

        while avcodec_receive_frame(res.codec_ctx, res.frame) >= 0 {
            if out.cancelled() {
                break;
            }

            let num_samples = (*res.frame).nb_samples as usize;
            let pts = (*res.frame).pts;
            let (frame_start, frame_end) = frame_sample_bounds(
                pts,
                num_samples,
                time_base,
                sample_rate,
                &mut tracked_sample_pos,
            );

            let converted_len = out.convert(res.swr_ctx, res.frame, channels)?;
            match frame_output_window(
                frame_start,
                frame_end,
                converted_len,
                channels,
                start_sample,
                stop_sample,
            ) {
                FrameOutputWindow::Skip => {}
                FrameOutputWindow::Stop => {
                    reached_stop = true;
                    break;
                }
                FrameOutputWindow::Emit {
                    skip_start,
                    take_end,
                    reached_end,
                } => {
                    reached_stop = reached_end;
                    if !out.push(skip_start, take_end) {
                        consumer_stopped = true;
                        break 'packets;
                    }
                }
            }
        }

        if reached_stop {
            break;
        }
    }

    // Flushing after the window's end would emit samples past it; a stopped
    // consumer has nowhere to put them.
    if !reached_stop && !consumer_stopped {
        let ret = avcodec_send_packet(res.codec_ctx, ptr::null());
        if ret < 0 {
            return Err(format!("Failed to flush decoder: {}", av_err_str(ret)));
        }
        while avcodec_receive_frame(res.codec_ctx, res.frame) >= 0 {
            if out.cancelled() {
                break;
            }
            let converted_len = out.convert(res.swr_ctx, res.frame, channels)?;
            if converted_len > 0 && !out.push(0, converted_len) {
                break;
            }
        }
    }

    Ok(())
}

/// Decode a `[start_sample, end_sample)` window from the buffer to interleaved
/// i32 PCM, collected whole. `None`/`None` decodes the whole stream. Blocking —
/// run it off the async runtime; reads block until the buffer's fill delivers
/// each window.
pub fn decode_audio(
    buffer: SharedSparseBuffer,
    start_sample: Option<u64>,
    end_sample: Option<u64>,
) -> Result<DecodedAudio, String> {
    let mut sink = CollectingSink {
        samples: Vec::new(),
        format: None,
    };
    let never_cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    decode_audio_to_sink(buffer, start_sample, end_sample, &mut sink, never_cancelled)?;
    // `decode_audio_to_sink` returning Ok means the format was probed, so
    // `on_format` ran before any samples.
    let (sample_rate, channels) = sink
        .format
        .expect("decode produced samples without a format");
    Ok(DecodedAudio {
        samples: sink.samples,
        sample_rate,
        channels,
    })
}

/// The sink behind `decode_audio`: collects the whole decode into a `Vec` and
/// records the probed format.
struct CollectingSink {
    samples: Vec<i32>,
    format: Option<(u32, u32)>,
}

impl DecodedSink for CollectingSink {
    fn on_format(&mut self, sample_rate: u32, channels: u32) {
        self.format = Some((sample_rate, channels));
    }
    fn on_samples(&mut self, samples: &[i32]) {
        self.samples.extend_from_slice(samples);
    }
}

/// Stream-decode a `[start_sample, end_sample)` window from the buffer, pushing
/// the format then each interleaved-i32 chunk into `sink`. `cancel` aborts
/// between frames. Blocking — run it off the async runtime.
///
/// Sets no read-ahead ceiling on its reader: the fill keeps one window
/// (`MIN_READAHEAD`) ahead of the decode and evicts behind it, so a whole-file
/// decode holds about one window of compressed bytes. (Playback's ceiling
/// exists to buffer the rest of a track against network stalls; a decode that
/// consumes as fast as it reads needs only the cushion.)
pub fn decode_audio_to_sink(
    buffer: SharedSparseBuffer,
    start_sample: Option<u64>,
    end_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    decode_audio_to_sink_with_seek(
        buffer,
        None,
        start_sample,
        start_sample,
        end_sample,
        sink,
        cancel,
    )
}

/// The i32 decode with playback's full seek dispatch: jump by byte to a
/// recorded landing (`seek_to_byte`), or seek by sample; trim the lead-in at
/// `start_at_sample`; stop at `stop_at_sample`. [`decode_audio_to_sink`] is the
/// sample-window special case. The save re-encoder drives this with the same
/// per-segment derivation playback uses.
#[allow(clippy::too_many_arguments)]
pub(crate) fn decode_audio_to_sink_with_seek(
    buffer: SharedSparseBuffer,
    seek_to_byte: Option<u64>,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    // SAFETY: every FFmpeg pointer the decode allocates lives and dies inside it.
    unsafe {
        decode_buffer_to_sink_impl(
            buffer,
            seek_to_byte,
            seek_to_sample,
            start_at_sample,
            stop_at_sample,
            sink,
            cancel,
        )
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn decode_buffer_to_sink_impl(
    buffer: SharedSparseBuffer,
    seek_to_byte: Option<u64>,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    sink: &mut dyn DecodedSink,
    cancel: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    use ffmpeg_sys_next::*;

    // Count this decode's fatal FFmpeg errors, so the sink can flag a track whose
    // audio bytes fail to decode (import's decode-verify rides this pass). The
    // counter is reset after the probe below, not here: probing an
    // attached-picture stream (an embedded cover) emits its own fatal-level logs
    // that say nothing about the audio.
    install_ffmpeg_log_callback();

    let BufferInput {
        mut fmt_ctx,
        avio,
        avio_ctx_ptr,
    } = open_buffer_input(&buffer, cancel.clone()).map_err(|e| e.message())?;

    // Count only the audio decode's errors: probing an embedded cover during
    // `find_stream_info` above may have logged its own.
    reset_ffmpeg_errors();

    let (audio, codec_ctx) = match open_probed_audio_codec(fmt_ctx) {
        Ok(opened) => opened,
        Err(ProbedAudioCodecOpenError::MissingStream(e)) => {
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            return Err(e);
        }
        Err(ProbedAudioCodecOpenError::Codec {
            audio,
            error: DecodeCodecOpenError::Missing(AVCodecID::AV_CODEC_ID_PCM_S64LE),
        }) => {
            let result = decode_packed_s64_packets_to_sink(
                fmt_ctx,
                audio.stream_index,
                audio.stream,
                audio.codecpar,
                (*audio.codecpar).sample_rate as u32,
                (*audio.codecpar).ch_layout.nb_channels as u32,
                start_at_sample,
                stop_at_sample,
                sink,
            );
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            sink.set_decode_error_count(get_ffmpeg_errors());
            return result;
        }
        Err(ProbedAudioCodecOpenError::Codec { error, .. }) => {
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            return Err(error.message());
        }
    };

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*audio.codecpar).ch_layout.nb_channels as u32;
    if channels == 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        close_input_and_free_custom_avio(&mut fmt_ctx, avio);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(CHANNEL_COUNT_ERROR.to_string());
    }

    sink.on_format(sample_rate, channels);

    let in_ch_layout = (*audio.codecpar).ch_layout;
    let mut swr_ctx = match allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_S32,
        sample_rate,
        (*codec_ctx).sample_fmt,
    ) {
        Ok(ctx) => ctx,
        Err(e) => {
            avcodec_free_context(&mut (codec_ctx as *mut _));
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            return Err(e);
        }
    };

    let time_base = (*audio.stream).time_base;

    // The same seek dispatch as the playback path: a by-byte jump lands on the
    // recorded frame boundary (no seektable consulted); otherwise seek by
    // sample. The `start_at_sample` trim below makes the first output sample
    // exact either way.
    if let Some(byte_pos) = seek_to_byte {
        let ret = av_seek_frame(
            fmt_ctx,
            audio.stream_index,
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
        seek_to_sample_or_warn(fmt_ctx, audio.stream_index, sample_pos);
    }

    let mut frame = av_frame_alloc();
    let mut packet = av_packet_alloc();
    if frame.is_null() || packet.is_null() {
        let mut codec_ctx = codec_ctx;
        free_decode_resources(
            &mut frame,
            &mut packet,
            &mut swr_ctx,
            &mut codec_ctx,
            &mut fmt_ctx,
            avio,
        );
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err("Failed to allocate frame/packet".to_string());
    }

    let resources = BufferDecodeResources {
        core: Some(DecodeResources {
            frame,
            packet,
            swr_ctx,
            codec_ctx,
            fmt_ctx,
            avio,
        }),
        avio_ctx_ptr,
    };

    let mut out = SinkOutput {
        sink,
        cancel: &cancel,
        converted: Vec::new(),
    };
    let result = run_decode_loop(
        resources.core(),
        audio.stream_index,
        time_base,
        sample_rate,
        channels as usize,
        start_at_sample,
        stop_at_sample,
        &mut out,
    );

    drop(resources);

    // A verifying sink flags a broken decode off this count.
    sink.set_decode_error_count(get_ffmpeg_errors());

    result?;

    // A cancelled input (a failed fill cancelling the buffer, or the caller
    // aborting) leaves the output truncated; reporting success would let a
    // consumer treat a partial decode as the whole window. The AVIO read
    // callback sets this token when the buffer is cancelled under the decode,
    // so both abort paths land here.
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("decode input cancelled".to_string());
    }

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

unsafe fn seek_to_sample_or_warn(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    sample_pos: u64,
) {
    use ffmpeg_sys_next::*;

    let ret = av_seek_frame(
        fmt_ctx,
        stream_index,
        sample_pos as i64,
        AVSEEK_FLAG_BACKWARD as c_int,
    );
    if ret < 0 {
        warn!(
            "seek to sample {sample_pos} failed ({}); decoding from the start",
            av_err_str(ret)
        );
    }
}

unsafe fn convert_frame_to_i32(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    frame: *const ffmpeg_sys_next::AVFrame,
    channels: usize,
) -> Result<Vec<i32>, String> {
    convert_frame_samples(swr_ctx, frame, channels)
}

unsafe fn convert_frame_to_f32(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    frame: *const ffmpeg_sys_next::AVFrame,
    channels: usize,
) -> Result<Vec<f32>, String> {
    convert_frame_samples(swr_ctx, frame, channels)
}

unsafe fn convert_frame_samples<T: Clone + Default>(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    frame: *const ffmpeg_sys_next::AVFrame,
    channels: usize,
) -> Result<Vec<T>, String> {
    convert_samples(
        swr_ctx,
        (*frame).extended_data as *const *const u8,
        (*frame).nb_samples,
        channels,
    )
}

unsafe fn convert_samples<T: Clone + Default>(
    swr_ctx: *mut ffmpeg_sys_next::SwrContext,
    input_data: *const *const u8,
    input_samples: c_int,
    channels: usize,
) -> Result<Vec<T>, String> {
    use ffmpeg_sys_next::*;

    let mut output_buf = vec![T::default(); input_samples as usize * channels];
    let out_ptr = output_buf.as_mut_ptr() as *mut u8;
    let converted = swr_convert(swr_ctx, &out_ptr, input_samples, input_data, input_samples);
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

    if channels == 0 {
        return Err(CHANNEL_COUNT_ERROR.to_string());
    }

    sink.on_format(sample_rate, channels);
    let in_ch_layout = (*codecpar).ch_layout;
    let mut swr_ctx = allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_S32,
        sample_rate,
        AVSampleFormat::AV_SAMPLE_FMT_S64,
    )?;

    if let Some(sample_pos) = start_sample {
        seek_to_sample_or_warn(fmt_ctx, stream_index, sample_pos);
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
        let (frame_start, frame_end) = frame_sample_bounds(
            pts,
            num_samples,
            time_base,
            sample_rate,
            &mut tracked_sample_pos,
        );

        let input = (*packet).data as *const u8;
        let packet_samples =
            match convert_samples(swr_ctx, &input, num_samples as c_int, channels_usize) {
                Ok(samples) => samples,
                Err(e) => {
                    av_packet_unref(packet);
                    av_packet_free(&mut (packet as *mut _));
                    swr_free(&mut swr_ctx);
                    return Err(e);
                }
            };

        match frame_output_window(
            frame_start,
            frame_end,
            packet_samples.len(),
            channels_usize,
            start_sample,
            end_sample,
        ) {
            FrameOutputWindow::Skip => {}
            FrameOutputWindow::Stop => {
                av_packet_unref(packet);
                break;
            }
            FrameOutputWindow::Emit {
                skip_start,
                take_end,
                reached_end: window_reached_end,
            } => {
                sink.on_samples(&packet_samples[skip_start..take_end]);
                reached_end = window_reached_end;
            }
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

// =============================================================================
// AVIO-based Streaming Decode
// =============================================================================

/// Decode from a `SparseStreamingBuffer` through FFmpeg's AVIO, pushing f32
/// samples to a `TrackSink`. FFmpeg finds the frame boundaries itself; the buffer
/// just serves the bytes it asks for, blocking until they land.
///
/// The buffer holds the whole backing file. `seek_to_byte` jumps FFmpeg straight
/// to a known frame offset (FLAC), or `seek_to_sample` seeks by sample through the
/// demuxer's index (APE); `start_at_sample` trims the lead-in FFmpeg emits before
/// the exact start (a frame may begin before it); `stop_at_sample` ends output at
/// the track's end. `end_byte` is the track's end byte offset — the read-ahead
/// ceiling handed to the reader so the fill buffers the rest of this track;
/// `None` keeps the whole file (a per-track file, or an album's last track).
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

    // Wall-clock origin for the first-sample latency log below: it spans the probe
    // (open_input + find_stream_info), the seek, and the decode up to the first
    // audio sample -- the whole "how long before playback starts" window.
    let decode_start = Instant::now();

    let cancel_status = cancel_token.clone();
    let BufferInput {
        mut fmt_ctx,
        avio,
        avio_ctx_ptr,
    } = open_buffer_input(&buffer, cancel_token).map_err(|e| {
        let message = e.message();
        match e {
            BufferInputError::Alloc(_) => StreamingDecodeError::decode(message),
            BufferInputError::Open(_) | BufferInputError::Probe(_) => {
                StreamingDecodeError::input_error(&cancel_status, message)
            }
        }
    })?;

    let (audio, codec_ctx) = match open_probed_audio_codec(fmt_ctx) {
        Ok(opened) => opened,
        Err(ProbedAudioCodecOpenError::MissingStream(e)) => {
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            return Err(StreamingDecodeError::decode(e));
        }
        Err(ProbedAudioCodecOpenError::Codec { error, .. }) => {
            close_input_and_free_custom_avio(&mut fmt_ctx, avio);
            let _ = Box::from_raw(avio_ctx_ptr);
            return Err(StreamingDecodeError::decode(error.message()));
        }
    };

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*audio.codecpar).ch_layout.nb_channels as u32;
    if channels == 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        close_input_and_free_custom_avio(&mut fmt_ctx, avio);
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(CHANNEL_COUNT_ERROR));
    }

    debug!("Streaming AVIO decoder: {}Hz, {}ch", sample_rate, channels);

    // Converts whatever the codec produces to packed f32, which is what the ring
    // and the audio callback speak.
    let in_ch_layout = (*audio.codecpar).ch_layout;
    let mut swr_ctx = allocate_swr_context(
        &in_ch_layout,
        AVSampleFormat::AV_SAMPLE_FMT_FLT,
        sample_rate,
        (*codec_ctx).sample_fmt,
    )
    .map_err(|e| {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        close_input_and_free_custom_avio(&mut fmt_ctx, avio);
        let _ = Box::from_raw(avio_ctx_ptr);
        StreamingDecodeError::decode(e)
    })?;

    // A by-byte seek (AVSEEK_FLAG_BYTE) jumps straight to a known frame offset --
    // no seektable consulted, no binary search reading the file's end -- and the
    // landed frame's sample comes from its own header, so the `start_at_sample`
    // trim below still reaches the exact sample. Used for FLAC, whose frame byte
    // is recorded at import; APE has no per-frame byte positions, so it
    // sample-seeks via its mandatory index instead.
    //
    // A sample seek costs one jump for a seektable-bearing FLAC (and for APE/MP4)
    // now that AVFMT_FLAG_FAST_SEEK is set. A FLAC with no seektable can't seek at
    // all here: the binary-search fallback would have to read the file's end before
    // it is fetched. Decoding from the start and trimming to the target is then the
    // only way to play it -- correct, but wasteful over a cloud home, which is why
    // every CUE/FLAC should carry a seektable (issue #226). Keep the bail-out
    // logged.
    if let Some(byte_pos) = seek_to_byte {
        let ret = av_seek_frame(
            fmt_ctx,
            audio.stream_index,
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
        let ret = avformat_seek_file(
            fmt_ctx,
            audio.stream_index,
            i64::MIN,
            target_ts,
            target_ts,
            0,
        );
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

    // The reader now sits at the track's start. Set its read-ahead ceiling -- the
    // track's end byte, or the whole file when the track runs to EOF (a per-track
    // file, or an album's last track) -- so the fill buffers the rest of this track
    // ahead of the playhead.
    let ceiling = match end_byte {
        Some(end) => end,
        None => buffer.get_total_size(),
    };
    (*avio_ctx_ptr)
        .reader
        .lock()
        .unwrap()
        .set_readahead_ceiling(ceiling);

    let mut frame = av_frame_alloc();
    let mut packet = av_packet_alloc();
    if frame.is_null() || packet.is_null() {
        let mut codec_ctx = codec_ctx;
        free_decode_resources(
            &mut frame,
            &mut packet,
            &mut swr_ctx,
            &mut codec_ctx,
            &mut fmt_ctx,
            avio,
        );
        let _ = Box::from_raw(avio_ctx_ptr);
        return Err(StreamingDecodeError::decode(
            "Failed to allocate frame/packet",
        ));
    }

    let resources = BufferDecodeResources {
        core: Some(DecodeResources {
            frame,
            packet,
            swr_ctx,
            codec_ctx,
            fmt_ctx,
            avio,
        }),
        avio_ctx_ptr,
    };
    let mut out = TrackOutput {
        sink,
        converted: Vec::new(),
        samples_output: 0,
        first_sample_logged: false,
        buffer: &buffer,
        decode_start,
    };
    let loop_result = run_decode_loop(
        resources.core(),
        audio.stream_index,
        (*audio.stream).time_base,
        sample_rate,
        channels as usize,
        start_at_sample,
        stop_at_sample,
        &mut out,
    );
    let samples_output = out.samples_output;

    drop(resources);
    loop_result.map_err(StreamingDecodeError::decode)?;

    let error_count = get_ffmpeg_errors();
    if error_count > 0 {
        warn!(
            "Streaming AVIO decode had {} fatal FFmpeg errors",
            error_count
        );
    }
    sink.set_decode_error_count(error_count);

    if !sink.is_cancelled() {
        sink.mark_finished();
    }

    info!(
        "Streaming AVIO decode complete: {}Hz, {}ch, {} samples, {} fatal errors",
        sample_rate, channels, samples_output, error_count
    );

    Ok(())
}

/// Push to the sink in chunks, re-checking cancellation between them so a stop
/// doesn't wait out a whole frame. `false` means cancelled: the decode loop exits.
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

    #[derive(Default)]
    struct TestDecodedSink {
        format: Option<(u32, u32)>,
    }

    impl DecodedSink for TestDecodedSink {
        fn on_format(&mut self, sample_rate: u32, channels: u32) {
            self.format = Some((sample_rate, channels));
        }

        fn on_samples(&mut self, _samples: &[i32]) {}
    }

    fn rational(num: c_int, den: c_int) -> ffmpeg_sys_next::AVRational {
        ffmpeg_sys_next::AVRational { num, den }
    }

    #[test]
    fn pts_to_sample_fast_paths_when_time_base_is_sample_rate() {
        // time_base == 1/sample_rate: the pts already counts samples, returned as-is.
        assert_eq!(pts_to_sample(1000, rational(1, 44_100), 44_100), 1000);
        // Millisecond time base scales up: 500ms * 44100/1000 = 22050 samples.
        assert_eq!(pts_to_sample(500, rational(1, 1000), 44_100), 22_050);
        // CUE-frame time base (1/75) at 75 frames -> exactly one second of samples.
        assert_eq!(pts_to_sample(75, rational(1, 75), 44_100), 44_100);
    }

    #[test]
    fn frame_sample_bounds_tracks_position_across_pts_gaps() {
        let tb = rational(1, 44_100);

        // A valid pts seeds the tracked position and advances it to the frame end.
        let mut tracked = -1;
        assert_eq!(
            frame_sample_bounds(1000, 100, tb, 44_100, &mut tracked),
            (1000, 1100)
        );
        assert_eq!(tracked, 1100);

        // A frame with no pts continues from the tracked position.
        assert_eq!(
            frame_sample_bounds(
                ffmpeg_sys_next::AV_NOPTS_VALUE,
                200,
                tb,
                44_100,
                &mut tracked
            ),
            (1100, 1300)
        );
        assert_eq!(tracked, 1300);

        // No pts and no tracked position yet: start is unknown (-1), and the
        // tracked position stays unset rather than advancing from -1.
        let mut untracked = -1;
        assert_eq!(
            frame_sample_bounds(
                ffmpeg_sys_next::AV_NOPTS_VALUE,
                50,
                tb,
                44_100,
                &mut untracked
            ),
            (-1, 49)
        );
        assert_eq!(untracked, -1);
    }

    #[test]
    fn frame_output_window_trims_at_boundaries() {
        use FrameOutputWindow::*;

        struct Row {
            name: &'static str,
            frame_start: i64,
            frame_end: i64,
            output_len: usize,
            channels: usize,
            start: Option<u64>,
            end: Option<u64>,
            expected: FrameOutputWindow,
        }
        let rows = [
            Row {
                name: "no window emits whole frame",
                frame_start: 0,
                frame_end: 100,
                output_len: 200,
                channels: 2,
                start: None,
                end: None,
                expected: Emit {
                    skip_start: 0,
                    take_end: 200,
                    reached_end: false,
                },
            },
            Row {
                name: "empty output with no window skips",
                frame_start: 0,
                frame_end: 0,
                output_len: 0,
                channels: 2,
                start: None,
                end: None,
                expected: Skip,
            },
            Row {
                name: "frame entirely before start skips",
                frame_start: 0,
                frame_end: 100,
                output_len: 200,
                channels: 2,
                start: Some(200),
                end: None,
                expected: Skip,
            },
            Row {
                name: "start at frame_end is exclusive, skips",
                frame_start: 0,
                frame_end: 100,
                output_len: 200,
                channels: 2,
                start: Some(100),
                end: None,
                expected: Skip,
            },
            Row {
                name: "frame straddling start trims the lead-in",
                frame_start: 0,
                frame_end: 100,
                output_len: 200,
                channels: 2,
                start: Some(40),
                end: None,
                expected: Emit {
                    skip_start: 80,
                    take_end: 200,
                    reached_end: false,
                },
            },
            Row {
                name: "frame at or past end stops",
                frame_start: 200,
                frame_end: 300,
                output_len: 200,
                channels: 2,
                start: None,
                end: Some(200),
                expected: Stop,
            },
            Row {
                name: "frame straddling end trims the tail and reaches end",
                frame_start: 0,
                frame_end: 100,
                output_len: 200,
                channels: 2,
                start: None,
                end: Some(60),
                expected: Emit {
                    skip_start: 0,
                    take_end: 120,
                    reached_end: true,
                },
            },
            Row {
                name: "take_end clamps to a short output buffer",
                frame_start: 0,
                frame_end: 100,
                output_len: 50,
                channels: 1,
                start: None,
                end: Some(80),
                expected: Emit {
                    skip_start: 0,
                    take_end: 50,
                    reached_end: true,
                },
            },
            Row {
                name: "start-skip past a reached end stops",
                frame_start: 0,
                frame_end: 100,
                output_len: 100,
                channels: 1,
                start: Some(70),
                end: Some(60),
                expected: Stop,
            },
            Row {
                name: "start-skip clamped to output with no end skips",
                frame_start: 0,
                frame_end: 100,
                output_len: 50,
                channels: 1,
                start: Some(80),
                end: None,
                expected: Skip,
            },
            Row {
                name: "unknown frame_start disables trimming",
                frame_start: -1,
                frame_end: 99,
                output_len: 200,
                channels: 2,
                start: Some(50),
                end: Some(60),
                expected: Emit {
                    skip_start: 0,
                    take_end: 200,
                    reached_end: false,
                },
            },
        ];

        for row in rows {
            let got = frame_output_window(
                row.frame_start,
                row.frame_end,
                row.output_len,
                row.channels,
                row.start,
                row.end,
            );
            assert_eq!(got, row.expected, "{}", row.name);
        }
    }

    #[test]
    fn packed_s64_decoder_rejects_zero_channels_before_format_signal() {
        crate::audio_codec::init();

        unsafe {
            let fmt_ctx = ffmpeg_sys_next::avformat_alloc_context();
            assert!(!fmt_ctx.is_null());
            let stream = ffmpeg_sys_next::avformat_new_stream(fmt_ctx, ptr::null());
            assert!(!stream.is_null());
            (*stream).time_base = ffmpeg_sys_next::AVRational {
                num: 1,
                den: 44_100,
            };
            let mut codecpar = ffmpeg_sys_next::avcodec_parameters_alloc();
            assert!(!codecpar.is_null());
            (*codecpar).ch_layout.nb_channels = 0;

            let mut sink = TestDecodedSink::default();
            let result = decode_packed_s64_packets_to_sink(
                fmt_ctx, 0, stream, codecpar, 44_100, 0, None, None, &mut sink,
            );

            ffmpeg_sys_next::avcodec_parameters_free(&mut codecpar);
            ffmpeg_sys_next::avformat_free_context(fmt_ctx);

            let err = result.expect_err("zero-channel PCM should fail");
            assert!(
                err.contains("channel count must be greater than zero"),
                "unexpected decode error: {err}"
            );
            assert_eq!(sink.format, None);
        }
    }
}
