use super::*;

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
///
/// Decodes one segment of a track. Returns its fatal FFmpeg errors plus invalid
/// packets discarded while decoding continued. It does not mark the sink
/// finished: a track is a sequence of segments (pregap, main) and only the
/// caller knows when the last one ends.
pub fn decode_audio_streaming(
    buffer: SharedSparseBuffer,
    sink: &mut TrackSink,
    seek_to_byte: Option<u64>,
    seek_to_sample: Option<u64>,
    start_at_sample: Option<u64>,
    stop_at_sample: Option<u64>,
    end_byte: Option<u64>,
    cancel_token: Arc<std::sync::atomic::AtomicBool>,
) -> Result<u32, DecodeError> {
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
) -> Result<u32, DecodeError> {
    use ffmpeg_sys_next::*;

    // Wall-clock origin for the first-sample latency log below: it spans the probe
    // (open_input + find_stream_info), the seek, and the decode up to the first
    // audio sample -- the whole "how long before playback starts" window.
    let decode_start = Instant::now();

    let BufferInput {
        mut fmt_ctx,
        avio,
        avio_ctx_ptr,
    } = open_buffer_input(&buffer, cancel_token.clone())
        .map_err(|e| e.into_decode_error(&cancel_token))?;

    let (audio, codec_ctx) = match open_probed_audio_codec(fmt_ctx) {
        Ok(opened) => opened,
        Err(ProbedAudioCodecOpenError::MissingStream(e)) => {
            let read_failure = release_buffer_input(&mut fmt_ctx, avio, avio_ctx_ptr);
            return Err(DecodeError::after_read(read_failure, e));
        }
        Err(ProbedAudioCodecOpenError::Codec { error, .. }) => {
            let read_failure = release_buffer_input(&mut fmt_ctx, avio, avio_ctx_ptr);
            return Err(DecodeError::after_read(read_failure, error.message()));
        }
    };

    let sample_rate = (*codec_ctx).sample_rate as u32;
    let channels = (*audio.codecpar).ch_layout.nb_channels as u32;
    if channels == 0 {
        avcodec_free_context(&mut (codec_ctx as *mut _));
        let read_failure = release_buffer_input(&mut fmt_ctx, avio, avio_ctx_ptr);
        return Err(DecodeError::after_read(read_failure, CHANNEL_COUNT_ERROR));
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
        let read_failure = release_buffer_input(&mut fmt_ctx, avio, avio_ctx_ptr);
        DecodeError::after_read(read_failure, e)
    })?;

    // The by-byte seek is for FLAC, whose frame byte is recorded at import; APE
    // has no per-frame byte positions, so it sample-seeks via its mandatory
    // index instead.
    //
    // A sample seek costs one jump for a seektable-bearing FLAC (and for APE/MP4)
    // now that AVFMT_FLAG_FAST_SEEK is set. A FLAC with no seektable can't seek at
    // all here: the binary-search fallback would have to read the file's end before
    // it is fetched. Decoding from the start and trimming to the target is then the
    // only way to play it -- correct, but wasteful over a cloud home, which is why
    // every CUE/FLAC should carry a seektable (issue #226). Keep the bail-out
    // logged.
    if let Some(byte_pos) = seek_to_byte {
        seek_to_byte_or_warn(fmt_ctx, codec_ctx, audio.stream_index, byte_pos);
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
    (*avio_ctx_ptr).set_readahead_ceiling(ceiling);

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
        return Err(DecodeError::decode("Failed to allocate frame/packet"));
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
    let mut out = TrackOutput::new(sink, &buffer, decode_start);
    let loop_result = run_decode_loop(
        resources.core(),
        audio.stream_index,
        (*audio.stream).time_base,
        sample_rate,
        channels as usize,
        start_at_sample,
        stop_at_sample,
        InvalidPacketHandling::Discard,
        &mut out,
    );
    let samples_output = out.samples_output();

    let read_failure = (*resources.avio_ctx_ptr).read_failure();
    drop(resources);
    // A failed source read ends FFmpeg's packet loop the way EOF does; the
    // read's error is the outcome, not the truncated segment.
    if let Some(error) = read_failure {
        return Err(DecodeError::SourceRead(error));
    }
    let discarded_packet_count = loop_result.map_err(DecodeError::decode)?;

    let fatal_error_count = get_ffmpeg_errors();
    let error_count = fatal_error_count.saturating_add(discarded_packet_count);
    if error_count > 0 {
        warn!(
            "Streaming AVIO decode had {fatal_error_count} fatal FFmpeg errors and {discarded_packet_count} discarded invalid packets"
        );
    }

    info!(
        "Streaming AVIO segment decode complete: {sample_rate}Hz, {channels}ch, {samples_output} samples, {fatal_error_count} fatal errors, {discarded_packet_count} discarded invalid packets"
    );

    Ok(error_count)
}
