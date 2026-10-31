//! Where a decode starts reading its stream, and the one seek dispatch that
//! puts the demuxer there.

use super::*;

/// Whether a decode that jumps by byte to a recorded frame knows which
/// samples the frames it then reads hold: their packets state their own first
/// sample -- a FLAC frame's header, a WavPack block's, an Ogg page's granule
/// (Opus, Vorbis). Every other demuxer loses its place on a byte jump: WAV's,
/// AIFF's and MP3's restart their timestamps at 0, so a track would play from
/// twice its start; MP4's can't jump by byte; APE's packets state no byte. A
/// decode of those seeks by sample, through the demuxer's own index or
/// arithmetic, except an MP3's, which starts at a recorded frame whose sample
/// it is told (see [`DecodeStart::Frame`]).
pub fn byte_jump_keeps_place(content_type: &bae_util::content_type::ContentType) -> bool {
    use bae_util::content_type::ContentType;
    matches!(
        content_type,
        ContentType::Flac | ContentType::WavPack | ContentType::Opus | ContentType::Vorbis
    )
}

/// Where a decode starts reading its stream. The decode's `start_at_sample`
/// trim then drops what it puts out before its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStart {
    /// The stream's first frame.
    StreamStart,
    /// The frame that begins at this byte, for a stream whose packets state
    /// their own first sample ([`byte_jump_keeps_place`]).
    Byte(u64),
    /// A recorded frame: the byte it begins at and its first sample. A stream
    /// whose packets state their own sample (a FLAC) jumps there by byte. An
    /// MP3's frames state no position, so its demuxer is given the frame as an
    /// index entry, seeks to the frame's sample through it, and counts the
    /// samples of the frames it reads from there: the timestamps, the encoder
    /// delay it drops only at the stream's first frame, and the padding it
    /// drops at its last are then the stream's own.
    Frame { byte: u64, sample: u64 },
    /// The frame at or before this sample, through the demuxer's own index or
    /// arithmetic (APE, MP4, WAV, a seektable-bearing FLAC), landing a lossy
    /// codec's pre-roll early. An MP3 has no index to seek by and is read from
    /// its start instead.
    Sample(u64),
}

/// Position the demuxer where a decode starts: the one seek dispatch every
/// decode uses (see [`DecodeStart`]). The caller's `start_at_sample` trim then
/// makes the first output sample exact.
///
/// The decoder is not flushed after the seek: it has been sent no packet, so
/// it holds nothing to drop, and a flush does not return every decoder to the
/// state it opened in -- AAC's decodes its next frames differently from a
/// freshly opened one.
pub(super) unsafe fn seek_input_or_warn(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    codec_ctx: *const ffmpeg_sys_next::AVCodecContext,
    audio: &AudioStreamInfo,
    sample_clock: &SampleClock,
    start: DecodeStart,
) {
    use bae_util::content_type::ContentType;

    let content_type = crate::probe::content_type_from_codec_id((*codec_ctx).codec_id);
    match start {
        DecodeStart::StreamStart => {}
        DecodeStart::Byte(byte) => seek_to_byte_or_warn(fmt_ctx, audio.stream_index, byte),
        DecodeStart::Frame { byte, .. } if byte_jump_keeps_place(&content_type) => {
            seek_to_byte_or_warn(fmt_ctx, audio.stream_index, byte)
        }
        DecodeStart::Frame { byte, sample } if content_type == ContentType::Mp3 => {
            seek_to_indexed_frame_or_warn(fmt_ctx, audio, sample_clock, byte, sample)
        }
        // Import records frames only for FLAC and MP3.
        DecodeStart::Frame { byte, sample } => error!(
            "a {content_type:?} stream was given a recorded frame (byte {byte}, sample \
             {sample}); decoding from the start"
        ),
        // Opened without `AVFMT_FLAG_FAST_SEEK`, an MP3 demuxer indexes only
        // the frames it has read, so a sample seek would read forward from its
        // first frames anyway, landing a pre-roll short of the frames its bit
        // reservoir reaches back to. Read from the start, the trim gives the
        // stream's own samples.
        DecodeStart::Sample(sample) if content_type == ContentType::Mp3 => {
            debug!("an MP3 seeks no sample: decoding from the start to sample {sample}")
        }
        DecodeStart::Sample(sample) => seek_to_sample_or_warn(
            fmt_ctx,
            audio.stream_index,
            sample_clock,
            sample as i64 - seek_preroll(codec_ctx) as i64,
        ),
    }
}

/// How far before its target a sample seek lands, so a lossy decoder has
/// rebuilt the state its frame at the target depends on by the time it gets
/// there: the codec's own seek pre-roll where it states one (Opus's 80 ms), and
/// at least one frame, which an MDCT frame overlaps (AAC's). The trim drops
/// what the pre-roll decodes. The pre-roll may reach before sample 0: an AAC
/// stream's priming frame sits there, which the decoder must be fed to put out
/// its first frames as a decode from the start does. A lossless codec's frames
/// stand alone.
unsafe fn seek_preroll(codec_ctx: *const ffmpeg_sys_next::AVCodecContext) -> u64 {
    if crate::probe::content_type_from_codec_id((*codec_ctx).codec_id).is_lossless_audio() {
        return 0;
    }
    let declared = u64::try_from((*codec_ctx).seek_preroll).unwrap_or(0);
    let frame = u64::try_from((*codec_ctx).frame_size).unwrap_or(0);
    declared.max(frame)
}

/// Jump straight to a recorded frame's byte offset: no seektable consulted, no
/// binary search reading the file's end. The landed frame states its own first
/// sample, so the caller's `start_at_sample` trim still reaches the exact
/// sample -- except a FLAC's last frame, which FFmpeg's FLAC parser gives no
/// timestamp, though its header carries its frame number. Import never
/// records a track start or a scrub point there. A failure is not fatal — the
/// caller decodes from the start instead, which is correct but reads more of
/// the file than it needs.
unsafe fn seek_to_byte_or_warn(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    byte_pos: u64,
) {
    use ffmpeg_sys_next::*;

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
    }
}

/// Start an MP3 decode at a recorded frame: the frame at `byte`, whose first
/// sample is `sample`. The frame goes into the stream's index at its
/// timestamp, replacing any entry there, and the demuxer seeks to that
/// timestamp through its index (opened without `AVFMT_FLAG_FAST_SEEK`, it has
/// no estimate to seek by instead): it reads from the frame's byte and dates
/// the frames from its timestamp on, as a decode from the stream's start
/// dates them, so it drops the encoder delay only at the stream's first frame
/// and the end padding at its last. A failure is logged: an entry that can't
/// be added leaves the decode reading from the start, and a failed seek
/// leaves it reading on from wherever the demuxer left its reader.
unsafe fn seek_to_indexed_frame_or_warn(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    audio: &AudioStreamInfo,
    sample_clock: &SampleClock,
    byte: u64,
    sample: u64,
) {
    use ffmpeg_sys_next::*;

    let timestamp = sample_clock.timestamp_of(sample as i64);
    let ret = av_add_index_entry(
        audio.stream,
        byte as i64,
        timestamp,
        0,
        0,
        AVINDEX_KEYFRAME as c_int,
    );
    if ret < 0 {
        warn!(
            "indexing the frame at byte {byte} (sample {sample}) failed ({}); decoding from the start",
            av_err_str(ret)
        );
        return;
    }
    let ret = avformat_seek_file(
        fmt_ctx,
        audio.stream_index,
        i64::MIN,
        timestamp,
        timestamp,
        0,
    );
    if ret < 0 {
        warn!(
            "seek to the frame at byte {byte} (sample {sample}) failed ({})",
            av_err_str(ret)
        );
    }
}

/// Seek to the frame at or before `sample_pos`. With `AVFMT_FLAG_FAST_SEEK`
/// set, a seektable-bearing FLAC (and APE/MP4) costs one jump. A FLAC with no
/// seektable and no stored scrub points binary-searches the file: its read of
/// the file's end waits for the fill like any other read, one fetch per step,
/// though the search can't place a sample in the stream's final frames. A seek
/// that fails is logged, and the decode reads on from wherever the failed
/// search left the reader: the start when the demuxer didn't move it, the
/// file's end after a FLAC search that read to it, where the decode finds no
/// packets.
pub(super) unsafe fn seek_to_sample_or_warn(
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    sample_clock: &SampleClock,
    sample_pos: i64,
) {
    use ffmpeg_sys_next::*;

    let target_ts = sample_clock.timestamp_of(sample_pos);
    let ret = avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target_ts, target_ts, 0);
    if ret < 0 {
        warn!(
            "seek to sample {sample_pos} failed ({}); decoding from the start \
             (file has no usable seektable?)",
            av_err_str(ret)
        );
    }
}
