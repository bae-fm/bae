//! Probing and format detection: open a file by path, inspect its best audio
//! stream, and report container/codec properties (`ProbeResult`) or per-sample
//! frame byte offsets (`frame_byte_offsets`).

use crate::util::content_type::ContentType;
use std::os::raw::c_int;
use std::ptr;
use tracing::{debug, warn};

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
pub(super) fn content_type_from_codec_id(id: ffmpeg_sys_next::AVCodecID) -> ContentType {
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
