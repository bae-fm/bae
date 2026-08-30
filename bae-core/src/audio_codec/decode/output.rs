use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum InvalidPacketHandling {
    Reject,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    Discard,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum FrameOutputWindow {
    Skip,
    Stop,
    Emit {
        skip_start: usize,
        take_end: usize,
        reached_end: bool,
    },
}

pub(super) fn pts_to_sample(
    pts: i64,
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: u32,
) -> i64 {
    if time_base.num == 1 && time_base.den == sample_rate as c_int {
        pts
    } else {
        (pts as f64 * time_base.num as f64 / time_base.den as f64 * sample_rate as f64) as i64
    }
}

pub(super) fn frame_sample_bounds(
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

pub(super) fn frame_output_window(
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

/// The output stage of the shared decode loop: converts each decoded frame to
/// its sample type and pushes the trimmed window to its consumer.
pub(super) trait FrameOutput {
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
pub(super) struct SinkOutput<'a> {
    sink: &'a mut dyn DecodedSink,
    cancel: &'a std::sync::atomic::AtomicBool,
    converted: Vec<i32>,
}

impl<'a> SinkOutput<'a> {
    pub(super) fn new(
        sink: &'a mut dyn DecodedSink,
        cancel: &'a std::sync::atomic::AtomicBool,
    ) -> Self {
        Self {
            sink,
            cancel,
            converted: Vec::new(),
        }
    }
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
pub(super) struct TrackOutput<'a> {
    sink: &'a mut TrackSink,
    converted: Vec<f32>,
    samples_output: u64,
    first_sample_logged: bool,
    buffer: &'a SharedSparseBuffer,
    decode_start: Instant,
}

impl<'a> TrackOutput<'a> {
    pub(super) fn new(
        sink: &'a mut TrackSink,
        buffer: &'a SharedSparseBuffer,
        decode_start: Instant,
    ) -> Self {
        Self {
            sink,
            converted: Vec::new(),
            samples_output: 0,
            first_sample_logged: false,
            buffer,
            decode_start,
        }
    }

    pub(super) fn samples_output(&self) -> u64 {
        self.samples_output
    }
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
pub(super) unsafe fn run_decode_loop(
    res: &DecodeResources,
    stream_index: c_int,
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: u32,
    channels: usize,
    start_sample: Option<u64>,
    stop_sample: Option<u64>,
    invalid_packet_handling: InvalidPacketHandling,
    out: &mut dyn FrameOutput,
) -> Result<u32, String> {
    use ffmpeg_sys_next::*;

    let mut tracked_sample_pos: i64 = -1;
    let mut reached_stop = false;
    let mut consumer_stopped = false;
    let mut discarded_invalid_packets = 0u32;

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

        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        if ret == AVERROR_INVALIDDATA && invalid_packet_handling == InvalidPacketHandling::Discard {
            discarded_invalid_packets = discarded_invalid_packets.saturating_add(1);
            continue;
        }
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

    Ok(discarded_invalid_packets)
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

pub(super) unsafe fn convert_samples<T: Clone + Default>(
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

pub(super) unsafe fn decode_packed_s64_packets_to_sink(
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
