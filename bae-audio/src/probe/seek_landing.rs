//! Where a seek to a sample lands: the byte of the frame holding each of a set
//! of samples (`seek_landing_bytes`), found with the demuxer's own seek through
//! the probe's opening of the file. The import records these bytes, so only the
//! desktop builds carry this code.

use super::{content_type_from_codec_id, open_best_audio_stream};
use bae_util::content_type::ContentType;
use coven::FileLease;
use std::os::raw::c_int;
use std::path::Path;

/// The timestamps of `samples` in `stream`, counted as a decode from the
/// stream's start counts them: from its `start_time`, past any encoder delay.
/// An error when the stream states no sample rate or an unusable time base.
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

/// The places of the open-files budget [`seek_landing_bytes`] takes.
pub const SEEK_LANDING_PLACES: usize = 2;

/// The byte of the frame holding each of `samples` -- where a playback seek to
/// that sample starts reading. Recorded at import as each track's start byte.
///
/// Each target is found with the demuxer's own seek: a FLAC's binary search
/// over its real frames (`open_best_audio_stream` leaves `AVFMT_FLAG_FAST_SEEK`
/// unset, so the file's seektable, which may be coarse or full of placeholder
/// points, isn't trusted), APE's mandatory index, WAV's arithmetic, MP4's
/// sample table. The landed frame's byte is the read cursor after the seek
/// where the demuxer moves it there (FLAC, APE, whose packets state no byte),
/// and the next packet's byte otherwise (MP4 moves only its place in the
/// sample table).
///
/// A FLAC's final frames are the exception: the binary search can't place a
/// sample among them, so they are read in order once, and a start in the last
/// frame is recorded as the frame before it, which a decode can date.
///
/// Playback never searches for a track: it byte-seeks straight to the recorded
/// byte (FLAC) or sample-seeks and prefetches it (APE). An error, saying why,
/// only if the file can't be opened or its frames can't be read or seeked.
///
/// The file opens in `lease`'s places. A FLAC is open twice at once (the
/// searching opening and the one that reads its final frames), so `lease`
/// holds at least [`SEEK_LANDING_PLACES`] places; a smaller one fails here
/// instead of waiting forever for a place of its own.
pub fn seek_landing_bytes(
    path: &Path,
    samples: &[u64],
    lease: &FileLease,
) -> Result<Vec<u64>, String> {
    if lease.count() < SEEK_LANDING_PLACES {
        return Err(format!(
            "finding where each track starts needs {SEEK_LANDING_PLACES} places of the \
             open-files budget, and its lease holds {}",
            lease.count()
        ));
    }
    unsafe {
        let (opened, stream_index) =
            open_best_audio_stream(path, lease.place_blocking()).map_err(|e| e.to_string())?;
        let fmt_ctx = opened.fmt_ctx;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let codecpar = (*stream).codecpar;
        let landings =
            stream_timestamps(stream, samples).and_then(
                |targets| match content_type_from_codec_id((*codecpar).codec_id) {
                    ContentType::Flac => {
                        flac_frame_landings(path, lease, fmt_ctx, stream_index, &targets)
                    }
                    content_type => {
                        let landing_byte = if content_type == ContentType::Ape {
                            LandingByte::ReadCursor
                        } else {
                            LandingByte::NextPacket
                        };
                        targets
                            .iter()
                            .map(|&target| {
                                demuxer_seek_landing(fmt_ctx, stream_index, target, landing_byte)
                            })
                            .collect()
                    }
                },
            );

        #[cfg(test)]
        crate::test_reads::record_landing_search(path, (*(*fmt_ctx).pb).bytes_read as u64);
        drop(opened);
        landings
    }
}

/// How many of a FLAC's final frames [`flac_frame_landings`] reads in order
/// rather than searches: one more than the frame headers FFmpeg's FLAC parser
/// holds before it emits a frame (`FLAC_MIN_HEADERS`, 10, in
/// `libavcodec/flac_parser.c`). An offset the search reads a timestamp from
/// before these frames therefore always has a full run of headers after it.
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
unsafe fn flac_frame_landings(
    path: &Path,
    lease: &FileLease,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    targets: &[i64],
) -> Result<Vec<u64>, String> {
    use ffmpeg_sys_next::*;

    let tail = flac_final_frames(path, lease)?;
    let (tail_pts, tail_pos) = tail[0];
    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    if av_add_index_entry(stream, tail_pos as i64, tail_pts, 0, 0, AVINDEX_KEYFRAME as c_int) < 0 {
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
                demuxer_seek_landing(fmt_ctx, stream_index, target, LandingByte::ReadCursor)
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
unsafe fn flac_final_frames(path: &Path, lease: &FileLease) -> Result<Vec<(i64, u64)>, String> {
    let (opened, stream_index) = open_best_audio_stream(path, lease.place_blocking())
        .map_err(|reason| format!("cannot open it to read its final frames: {reason}"))?;
    let frames = flac_final_frames_in(opened.fmt_ctx, stream_index);
    #[cfg(test)]
    crate::test_reads::record_landing_search(path, (*(*opened.fmt_ctx).pb).bytes_read as u64);
    drop(opened);
    frames
}

/// [`flac_final_frames`] over an opened file.
unsafe fn flac_final_frames_in(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
) -> Result<Vec<(i64, u64)>, String> {
    use ffmpeg_sys_next::*;

    let size = avio_size((*fmt_ctx).pb);
    if size < 0 {
        return Err(format!("file size unknown ({})", crate::av_err_str(size as i32)));
    }
    // STREAMINFO's largest frame size, when the encoder recorded it, sizes the
    // first read to hold the frames needed (plus the one a mid-frame start
    // misplaces); otherwise, or if the frames are fewer than it says, the read
    // moves back until they fit.
    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    let mut back = flac_max_frame_bytes((*stream).codecpar)
        .map_or(0, |bytes| (FLAC_TAIL_FRAMES as i64 + 2) * bytes)
        .max(crate::AVIO_BUFFER_SIZE as i64);
    loop {
        // A byte seek below the first frame lands on the first frame.
        let from = (size - back).max(0);
        let result = av_seek_frame(fmt_ctx, stream_index, from, AVSEEK_FLAG_BYTE as c_int);
        if result < 0 {
            return Err(format!("byte seek to {from} failed ({})", crate::av_err_str(result)));
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
                Err(format!("reading frames failed ({})", crate::av_err_str(result)))
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
            break Err(format!("a frame carries no timestamp or byte (pts {pts}, pos {pos})"));
        }
        frames.push((pts, pos as u64));
    };
    av_packet_free(&mut packet);
    read.map(|()| frames)
}

/// How to read where a demuxer's seek landed.
#[derive(Debug, Clone, Copy)]
enum LandingByte {
    /// The read cursor (`avio_tell`), for a demuxer whose seek moves it to the
    /// frame it reads next: FLAC's search, and APE's index, whose packets carry
    /// no byte.
    ReadCursor,
    /// The byte the next packet states, for a demuxer whose seek moves only
    /// its place in an index and leaves the cursor where it was (MP4's sample
    /// table), and every other one whose packets state their bytes.
    NextPacket,
}

/// The byte the demuxer's own seek to `target` (a stream timestamp) lands on.
unsafe fn demuxer_seek_landing(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    target: i64,
    landing_byte: LandingByte,
) -> Result<u64, String> {
    use ffmpeg_sys_next::*;

    if avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target, target, 0) < 0 {
        return Err(format!("seek to timestamp {target} failed"));
    }
    match landing_byte {
        LandingByte::ReadCursor => {
            // `avio_seek(.., 0, SEEK_CUR)` is `avio_tell`: returns the position
            // without moving it.
            let pos = avio_seek((*fmt_ctx).pb, 0, 1 /* SEEK_CUR */);
            if pos < 0 {
                return Err(format!("avio_tell after seek to timestamp {target} failed"));
            }
            Ok(pos as u64)
        }
        LandingByte::NextPacket => next_packet_byte(fmt_ctx, stream_index)
            .map_err(|reason| format!("after seek to timestamp {target}: {reason}")),
    }
}

/// The byte the next packet of `stream_index` begins at.
unsafe fn next_packet_byte(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
) -> Result<u64, String> {
    use ffmpeg_sys_next::*;

    let mut packet = av_packet_alloc();
    if packet.is_null() {
        return Err("cannot allocate a packet".to_string());
    }
    let read = loop {
        let result = av_read_frame(fmt_ctx, packet);
        if result < 0 {
            break Err(format!("reading the next frame failed ({})", crate::av_err_str(result)));
        }
        let ours = (*packet).stream_index == stream_index;
        let pos = (*packet).pos;
        av_packet_unref(packet);
        if ours {
            break u64::try_from(pos).map_err(|error| format!("the frame states no byte: {error}"));
        }
    };
    av_packet_free(&mut packet);
    read
}

#[cfg(test)]
#[path = "seek_landing_tests.rs"]
mod tests;
