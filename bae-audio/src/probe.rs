//! Probing and format detection: open a file by path, inspect its best audio
//! stream, and report container/codec properties (`ProbeResult`) or the byte of
//! the frame holding each of a set of samples (`seek_landing_bytes`).

use bae_util::content_type::ContentType;
use std::os::raw::c_int;
use std::ptr;
use tracing::{debug, warn};

/// Probe-discovered audio properties. Populated from `AVCodecID` and
/// `AVCodecParameters`, so every field reflects what FFmpeg actually sees
/// in the bytes rather than what the file extension promises.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub content_type: ContentType,
    pub duration: std::time::Duration,
    pub sample_rate: u32,
    /// Source bit depth for lossless audio. Perceptual codecs are `None` even
    /// when FFmpeg exposes a coded-word width.
    pub bits_per_sample: Option<u32>,
    /// Average coded-stream bitrate for perceptual codecs. Container metadata,
    /// embedded artwork, and other non-audio bytes are excluded.
    pub bitrate_kbps: Option<i64>,
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
        AVCodecID::AV_CODEC_ID_OPUS => ContentType::Opus,
        AVCodecID::AV_CODEC_ID_VORBIS => ContentType::Vorbis,
        AVCodecID::AV_CODEC_ID_WAVPACK => ContentType::WavPack,
        AVCodecID::AV_CODEC_ID_DSD_LSBF
        | AVCodecID::AV_CODEC_ID_DSD_MSBF
        | AVCodecID::AV_CODEC_ID_DSD_LSBF_PLANAR
        | AVCodecID::AV_CODEC_ID_DSD_MSBF_PLANAR => ContentType::Dsd,
        AVCodecID::AV_CODEC_ID_PCM_S16LE
        | AVCodecID::AV_CODEC_ID_PCM_S16BE
        | AVCodecID::AV_CODEC_ID_PCM_U16LE
        | AVCodecID::AV_CODEC_ID_PCM_U16BE
        | AVCodecID::AV_CODEC_ID_PCM_S24LE
        | AVCodecID::AV_CODEC_ID_PCM_S24BE
        | AVCodecID::AV_CODEC_ID_PCM_U24LE
        | AVCodecID::AV_CODEC_ID_PCM_U24BE
        | AVCodecID::AV_CODEC_ID_PCM_S32LE
        | AVCodecID::AV_CODEC_ID_PCM_S32BE
        | AVCodecID::AV_CODEC_ID_PCM_U32LE
        | AVCodecID::AV_CODEC_ID_PCM_U32BE
        | AVCodecID::AV_CODEC_ID_PCM_F32LE
        | AVCodecID::AV_CODEC_ID_PCM_F32BE
        | AVCodecID::AV_CODEC_ID_PCM_F64LE
        | AVCodecID::AV_CODEC_ID_PCM_F64BE
        | AVCodecID::AV_CODEC_ID_PCM_U8
        | AVCodecID::AV_CODEC_ID_PCM_S8
        | AVCodecID::AV_CODEC_ID_PCM_ALAW
        | AVCodecID::AV_CODEC_ID_PCM_MULAW
        | AVCodecID::AV_CODEC_ID_PCM_S64LE
        | AVCodecID::AV_CODEC_ID_PCM_S64BE => ContentType::Pcm,
        other => ContentType::Other(format!("codec:{:?}", other)),
    }
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

fn duration_from_time_base(
    units: i64,
    time_base: ffmpeg_sys_next::AVRational,
) -> Option<std::time::Duration> {
    if units <= 0 || time_base.num <= 0 || time_base.den <= 0 {
        return None;
    }

    let micros = units as i128 * time_base.num as i128 * 1_000_000i128 / time_base.den as i128;
    (micros > 0).then(|| std::time::Duration::from_micros(micros as u64))
}

unsafe fn probed_duration(
    path: &str,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream: *mut ffmpeg_sys_next::AVStream,
) -> Option<std::time::Duration> {
    if let Some(duration) = duration_from_time_base((*stream).duration, (*stream).time_base) {
        return Some(duration);
    }

    let duration_us = (*fmt_ctx).duration;
    if duration_us > 0 {
        debug!(
            "probe_audio_from_path: using container duration for {path}; stream duration was unavailable"
        );
        Some(std::time::Duration::from_micros(duration_us as u64))
    } else {
        None
    }
}

fn kbps_from_bps(bps: i64) -> Option<i64> {
    if bps <= 0 {
        return None;
    }
    let rounded = bps / 1_000 + i64::from(bps % 1_000 >= 500);
    Some(rounded.max(1))
}

/// FFmpeg exposes a declared/estimated bitrate for formats such as MP3, AAC,
/// and Vorbis. Opus commonly leaves that field unset, so derive its average
/// from audio-packet bytes only. Both paths describe the coded audio stream;
/// neither includes container tags or embedded artwork.
unsafe fn probed_lossy_bitrate_kbps(
    path: &str,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    duration: std::time::Duration,
) -> Option<i64> {
    use ffmpeg_sys_next::*;

    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    if let Some(kbps) = kbps_from_bps((*(*stream).codecpar).bit_rate) {
        return Some(kbps);
    }

    let start = if (*stream).start_time == AV_NOPTS_VALUE {
        0
    } else {
        (*stream).start_time
    };
    if av_seek_frame(fmt_ctx, stream_index, start, AVSEEK_FLAG_BACKWARD) < 0 {
        warn!("probe_audio_from_path: cannot seek {path} to measure audio bitrate");
        return None;
    }

    let mut packet = av_packet_alloc();
    if packet.is_null() {
        warn!("probe_audio_from_path: cannot allocate packet to measure {path} bitrate");
        return None;
    }
    let mut audio_bytes = 0u64;
    let read_result = loop {
        let result = av_read_frame(fmt_ctx, packet);
        if result < 0 {
            break result;
        }
        if (*packet).stream_index == stream_index {
            let packet_size = (*packet).size;
            if packet_size < 0 {
                av_packet_unref(packet);
                warn!("probe_audio_from_path: negative audio packet size in {path}");
                av_packet_free(&mut packet);
                return None;
            }
            let Some(total) = audio_bytes.checked_add(packet_size as u64) else {
                av_packet_unref(packet);
                warn!("probe_audio_from_path: audio byte count overflow for {path}");
                av_packet_free(&mut packet);
                return None;
            };
            audio_bytes = total;
        }
        av_packet_unref(packet);
    };
    av_packet_free(&mut packet);

    if read_result != AVERROR_EOF {
        warn!("probe_audio_from_path: failed while reading audio packets from {path}");
        return None;
    }
    if audio_bytes == 0 {
        warn!("probe_audio_from_path: no audio packets found while measuring {path}");
        return None;
    }

    let bps = u128::from(audio_bytes) * 8 * 1_000_000_000 / duration.as_nanos();
    let Ok(bps) = i64::try_from(bps) else {
        warn!("probe_audio_from_path: measured bitrate is out of range for {path}");
        return None;
    };
    kbps_from_bps(bps)
}

/// The file's audio properties, read from its current bytes.
pub fn probe_audio_from_path(path: &str) -> Option<ProbeResult> {
    open_and_probe(path)
}

/// Open the file with FFmpeg and read its best audio stream. `None` (already
/// logged) when it cannot be opened, has no audio stream, or states no usable
/// duration.
fn open_and_probe(path: &str) -> Option<ProbeResult> {
    #[cfg(feature = "test-instrumentation")]
    crate::test_reads::record_probe(std::path::Path::new(path));
    unsafe {
        use ffmpeg_sys_next::*;

        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "probe_audio_from_path")?;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let duration = probed_duration(path, fmt_ctx, stream);
        let codecpar = (*stream).codecpar;
        let codec_id = (*codecpar).codec_id;
        let content_type = content_type_from_codec_id(codec_id);
        let sample_rate = (*codecpar).sample_rate as u32;
        let bits_per_sample = if !content_type.is_lossless_audio() {
            None
        } else if (*codecpar).bits_per_raw_sample > 0 {
            Some((*codecpar).bits_per_raw_sample as u32)
        } else if (*codecpar).bits_per_coded_sample > 0 {
            Some((*codecpar).bits_per_coded_sample as u32)
        } else {
            None
        };
        let channels = (*codecpar).ch_layout.nb_channels as u32;

        let bitrate_kbps = if content_type.is_lossless_audio() {
            None
        } else {
            duration.and_then(|duration| {
                probed_lossy_bitrate_kbps(path, fmt_ctx, stream_index, duration)
            })
        };

        avformat_close_input(&mut fmt_ctx);

        if let Some(duration) = duration {
            Some(ProbeResult {
                content_type,
                duration,
                sample_rate,
                bits_per_sample,
                bitrate_kbps,
                channels,
            })
        } else {
            debug!("probe_audio_from_path: no usable container duration for {path}");
            None
        }
    }
}

/// The timestamps of `samples` in `stream`, counted as a decode from the
/// stream's start counts them: from its `start_time`, past any encoder delay.
/// An error when the stream states no sample rate or an unusable time base.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn stream_timestamps(
    stream: *const ffmpeg_sys_next::AVStream,
    samples: &[u64],
) -> Result<Vec<i64>, String> {
    let time_base = (*stream).time_base;
    let sample_rate = (*(*stream).codecpar).sample_rate;
    if sample_rate <= 0 || time_base.num <= 0 || time_base.den <= 0 {
        return Err(format!(
            "unusable time base {}/{} at {sample_rate} Hz",
            time_base.num, time_base.den
        ));
    }
    let clock = crate::sample_clock::SampleClock::of_stream(stream, sample_rate as u32);
    samples
        .iter()
        .map(|&sample| {
            i64::try_from(sample)
                .map(|sample| clock.timestamp_of(sample))
                .map_err(|error| format!("sample {sample} is past any timestamp: {error}"))
        })
        .collect()
}

/// The byte of the frame holding each of `samples` -- where a playback seek to
/// that sample starts reading. Recorded at import as each track's start byte.
///
/// Each target is found with the demuxer's own seek, reading the AVIO position
/// (`avio_tell`) right after it, the way the demuxer itself positions: a FLAC's
/// binary search over its real frames (`open_best_audio_stream` leaves
/// `AVFMT_FLAG_FAST_SEEK` unset, so the file's seektable, which may be coarse or
/// full of placeholder points, isn't trusted), APE's mandatory index (its packets
/// carry no byte position), WAV's arithmetic.
///
/// A FLAC's final frames are the exception: the binary search can't place a
/// sample among them, so they are read in order once, and a start in the last
/// frame is recorded as the frame before it, which a decode can date.
///
/// Playback never searches for a track: it byte-seeks straight to the recorded
/// byte (FLAC) or sample-seeks and prefetches it (APE). `None` only if the file
/// can't be opened or its frames can't be read or seeked.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn seek_landing_bytes(path: &str, samples: &[u64]) -> Option<Vec<u64>> {
    unsafe {
        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "seek_landing_bytes")?;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let codecpar = (*stream).codecpar;
        let landings =
            stream_timestamps(stream, samples).and_then(
                |targets| match content_type_from_codec_id((*codecpar).codec_id) {
                    ContentType::Flac => flac_frame_landings(path, fmt_ctx, stream_index, &targets),
                    _ => targets
                        .iter()
                        .map(|&target| demuxer_seek_landing(fmt_ctx, stream_index, target))
                        .collect(),
                },
            );

        #[cfg(test)]
        crate::test_reads::record_landing_search(
            std::path::Path::new(path),
            (*(*fmt_ctx).pb).bytes_read as u64,
        );
        ffmpeg_sys_next::avformat_close_input(&mut fmt_ctx);
        match landings {
            Ok(landings) => Some(landings),
            Err(reason) => {
                warn!("seek_landing_bytes: {reason} in {path}");
                None
            }
        }
    }
}

/// How many of a FLAC's final frames [`flac_frame_landings`] reads in order
/// rather than searches: one more than the frame headers FFmpeg's FLAC parser
/// holds before it emits a frame (`FLAC_MIN_HEADERS`, 10, in
/// `libavcodec/flac_parser.c`). An offset the search reads a timestamp from
/// before these frames therefore always has a full run of headers after it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
const FLAC_TAIL_FRAMES: usize = 11;

/// The byte of the FLAC frame holding each of `targets` (stream timestamps).
///
/// FFmpeg's binary search can't place a target in a stream's final frames. It
/// reads each timestamp by parsing from an arbitrary offset, and near the end of
/// the file the parser has too few headers left to date a frame -- it dates a
/// fixed-blocksize frame only once it has seen the frame after it, so the last
/// frame never gets one. A target there fails the seek, or lands a frame short.
/// How many final frames are affected depends on their byte sizes, not on a
/// count of samples, so no sample margin before a target avoids them.
///
/// So the final frames are read in order once, from a byte offset before them
/// (where every frame is dated: each has its successor, and the last follows a
/// dated one), and the first of them is added to the demuxer's index. The index
/// entry bounds every search below it, keeping its reads off the final frames
/// and sparing it the read of the file's end it would otherwise make per target
/// (`ff_find_last_ts`). A target among the final frames takes its frame's byte
/// from that read; every other target is searched.
///
/// A target in the last frame takes the byte of the frame before it. A decode
/// that jumps to the last frame can't place its samples -- the parser gives it
/// no timestamp, and nothing before it was read -- so it couldn't trim to the
/// target. From the frame before, which the parser dates, the last frame's
/// timestamp follows.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn flac_frame_landings(
    path: &str,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    targets: &[i64],
) -> Result<Vec<u64>, String> {
    use ffmpeg_sys_next::*;

    let tail = flac_final_frames(path)?;
    let (tail_pts, tail_pos) = tail[0];
    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    if av_add_index_entry(
        stream,
        tail_pos as i64,
        tail_pts,
        0,
        0,
        AVINDEX_KEYFRAME as c_int,
    ) < 0
    {
        return Err("cannot index the final frames".to_string());
    }
    targets
        .iter()
        .map(|&target| {
            if target >= tail_pts {
                // The frame holding the target -- or, for the last frame, the one
                // before it.
                let holding = tail.partition_point(|&(pts, _)| pts <= target) - 1;
                let landing = if holding == tail.len() - 1 {
                    holding.checked_sub(1).ok_or_else(|| {
                        format!("timestamp {target} is in the stream's only frame")
                    })?
                } else {
                    holding
                };
                Ok(tail[landing].1)
            } else {
                demuxer_seek_landing(fmt_ctx, stream_index, target)
            }
        })
        .collect()
}

/// The (timestamp, byte) of at least the last [`FLAC_TAIL_FRAMES`] frames of a
/// FLAC (all of them in a shorter stream), in order: read from a byte offset
/// before the file's end, moved back until the read holds that many frames.
///
/// Read through its own opening of the file: the demuxer adds every frame it
/// reads to its index, and the first frame after a start inside a frame gets a
/// wrong byte (the parser counts the leftover it passes over from where it
/// stopped reading, not where that frame begins). The searching opening must
/// not inherit that entry.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn flac_final_frames(path: &str) -> Result<Vec<(i64, u64)>, String> {
    use ffmpeg_sys_next::*;

    let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "seek_landing_bytes")
        .ok_or_else(|| "cannot open it to read its final frames".to_string())?;
    let frames = flac_final_frames_in(fmt_ctx, stream_index);
    #[cfg(test)]
    crate::test_reads::record_landing_search(
        std::path::Path::new(path),
        (*(*fmt_ctx).pb).bytes_read as u64,
    );
    avformat_close_input(&mut fmt_ctx);
    frames
}

/// [`flac_final_frames`] over an opened file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn flac_final_frames_in(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
) -> Result<Vec<(i64, u64)>, String> {
    use ffmpeg_sys_next::*;

    let size = avio_size((*fmt_ctx).pb);
    if size < 0 {
        return Err(format!(
            "file size unknown ({})",
            super::av_err_str(size as i32)
        ));
    }
    // STREAMINFO's largest frame size, when the encoder recorded it, sizes the
    // first read to hold the frames needed (plus the one a mid-frame start
    // misplaces); otherwise, or if the frames are fewer than it says, the read
    // moves back until they fit.
    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let mut back = flac_max_frame_bytes((*stream).codecpar)
        .map_or(0, |bytes| (FLAC_TAIL_FRAMES as i64 + 2) * bytes)
        .max(super::AVIO_BUFFER_SIZE as i64);
    loop {
        // A byte seek below the first frame lands on the first frame.
        let from = (size - back).max(0);
        let result = av_seek_frame(fmt_ctx, stream_index, from, AVSEEK_FLAG_BYTE as c_int);
        if result < 0 {
            return Err(format!(
                "byte seek to {from} failed ({})",
                super::av_err_str(result)
            ));
        }
        let mut frames = flac_frames_to_end(fmt_ctx, stream_index)?;
        // A start past the first frame may fall inside a frame, and the frame
        // after one gets a wrong byte; a start on the first frame can't.
        if from > 0 && !frames.is_empty() {
            frames.remove(0);
        }
        if frames.len() >= FLAC_TAIL_FRAMES || (from == 0 && !frames.is_empty()) {
            return Ok(frames);
        }
        if from == 0 {
            return Err("the stream holds no frames".to_string());
        }
        back *= 2;
    }
}

/// The largest frame, in bytes, the FLAC's STREAMINFO block (FFmpeg's
/// extradata for the stream) records; `None` when the encoder left it unknown.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn flac_max_frame_bytes(codecpar: *const ffmpeg_sys_next::AVCodecParameters) -> Option<i64> {
    // STREAMINFO: min/max block size (16 bits each), then min/max frame size
    // (24 bits each).
    const STREAMINFO_MAX_FRAME_SIZE: std::ops::Range<usize> = 7..10;
    let extradata = (*codecpar).extradata;
    let size = (*codecpar).extradata_size;
    if extradata.is_null() || size < STREAMINFO_MAX_FRAME_SIZE.end as c_int {
        return None;
    }
    let bytes = std::slice::from_raw_parts(extradata, size as usize);
    let [a, b, c] = bytes[STREAMINFO_MAX_FRAME_SIZE] else {
        unreachable!("the range is three bytes");
    };
    let max = i64::from(u32::from_be_bytes([0, a, b, c]));
    (max > 0).then_some(max)
}

/// The (timestamp, byte) of each frame from the read cursor to the end of the
/// file. A read that starts inside a frame first passes over the rest of it,
/// which the parser emits undated; every frame after the first dated one must be
/// dated.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn flac_frames_to_end(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
) -> Result<Vec<(i64, u64)>, String> {
    use ffmpeg_sys_next::*;

    let mut packet = av_packet_alloc();
    if packet.is_null() {
        return Err("cannot allocate a packet".to_string());
    }
    let mut frames: Vec<(i64, u64)> = Vec::new();
    let read = loop {
        let result = av_read_frame(fmt_ctx, packet);
        if result < 0 {
            break if result == AVERROR_EOF {
                Ok(())
            } else {
                Err(format!(
                    "reading frames failed ({})",
                    super::av_err_str(result)
                ))
            };
        }
        let frame =
            ((*packet).stream_index == stream_index).then(|| ((*packet).pts, (*packet).pos));
        av_packet_unref(packet);
        let Some((pts, pos)) = frame else {
            continue;
        };
        if pts == AV_NOPTS_VALUE && frames.is_empty() {
            continue;
        }
        if pts == AV_NOPTS_VALUE || pos < 0 {
            break Err(format!(
                "a frame carries no timestamp or byte (pts {pts}, pos {pos})"
            ));
        }
        frames.push((pts, pos as u64));
    };
    av_packet_free(&mut packet);
    read.map(|()| frames)
}

/// Where the demuxer's own seek to `target` (a stream timestamp) leaves the read
/// cursor.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
unsafe fn demuxer_seek_landing(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    target: i64,
) -> Result<u64, String> {
    use ffmpeg_sys_next::*;

    if avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target, target, 0) < 0 {
        return Err(format!("seek to timestamp {target} failed"));
    }
    // Where the demuxer left the read cursor = the frame it will read next.
    // `avio_seek(.., 0, SEEK_CUR)` is `avio_tell`: returns the position without
    // moving it.
    let pos = avio_seek((*fmt_ctx).pb, 0, 1 /* SEEK_CUR */);
    if pos < 0 {
        return Err(format!("avio_tell after seek to timestamp {target} failed"));
    }
    Ok(pos as u64)
}
