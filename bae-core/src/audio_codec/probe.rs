//! Probing and format detection: open a file by path, inspect its best audio
//! stream, and report container/codec properties (`ProbeResult`) or the byte the
//! demuxer lands on when seeking to each of a set of samples (`seek_landing_bytes`).

use crate::util::content_type::ContentType;
use crate::util::session_cache::SessionCache;
use std::os::raw::c_int;
use std::ptr;
use tracing::{debug, warn};

/// How many probed files the cache below holds.
///
/// One entry per audio file, and the folders in play are whole watched roots:
/// identification walks every candidate, and the pane the user opens minutes
/// later must still find its own files here. Sized for a large library rather
/// than for one import, because eviction costs an FFmpeg open of a file that is
/// very often on a slow external volume.
const PROBE_CACHE_CAPACITY: usize = 20_000;

/// Files already probed this session, keyed by absolute path.
///
/// The entry carries the size and modification time the file had when it was
/// read, and is served only while both still match what the filesystem reports
/// — so re-encoding, retagging, or replacing a file re-probes it rather than
/// answering about bytes that are gone. Nothing expires on time, because time
/// says nothing about whether a file changed.
static PROBE_CACHE: SessionCache<CachedProbe> =
    SessionCache::new("audio probe cache", PROBE_CACHE_CAPACITY);

/// A probe, with the file identity it describes.
#[derive(Clone)]
struct CachedProbe {
    stamp: FileStamp,
    /// What the probe answered — `None` included: "FFmpeg cannot read these
    /// bytes" is an answer about this exact file, and re-deriving it costs the
    /// same open as any other.
    result: Option<ProbeResult>,
}

/// What the filesystem says a file is right now: its size and modification
/// time, which is what writing to it moves.
#[derive(Clone, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    modified: std::time::SystemTime,
}

impl FileStamp {
    /// The file's current stamp, or `None` when the filesystem will not state
    /// one — the path is gone, or the volume keeps no modification times.
    /// Either way there is nothing to key an entry on, so the caller probes
    /// without caching rather than trusting an entry it cannot invalidate.
    fn of(path: &str) -> Option<Self> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                debug!("probe cache: {path} cannot be stat'd ({error}); probing uncached");
                return None;
            }
        };
        match metadata.modified() {
            Ok(modified) => Some(Self {
                size: metadata.len(),
                modified,
            }),
            Err(error) => {
                debug!("probe cache: {path} has no modification time ({error}); probing uncached");
                None
            }
        }
    }
}

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

/// The file's audio properties, read once per file and reused while the file is
/// unchanged.
///
/// Opening a container is the expensive part of every surface that needs a
/// playing time — identification's total, the mapping pane's per-row lengths,
/// the commit's per-track durations — and they all ask about the same files.
/// So the answer is remembered against the file's identity (path, size,
/// modification time) and the open happens once, however many of them ask.
pub fn probe_audio_from_path(path: &str) -> Option<ProbeResult> {
    let Some(stamp) = FileStamp::of(path) else {
        return open_and_probe(path);
    };
    if let Some(cached) = PROBE_CACHE.get_cloned(path) {
        if cached.stamp == stamp {
            return cached.result;
        }
    }
    let result = open_and_probe(path);
    PROBE_CACHE.put(
        path,
        CachedProbe {
            stamp,
            result: result.clone(),
        },
    );
    result
}

/// Read audio properties from the bytes at `path` without consulting the
/// metadata-keyed session cache. The folder scanner uses this after hashing
/// the file so the persisted digest and facts always describe the same bytes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn probe_audio_from_path_uncached(path: &str) -> Option<ProbeResult> {
    open_and_probe(path)
}

/// How many times each path has been opened by [`open_and_probe`], so a test
/// can assert that re-asking about a folder reads nothing off disk.
#[cfg(test)]
fn probe_open_counts() -> &'static std::sync::Mutex<std::collections::HashMap<String, u64>> {
    static COUNTS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u64>>> =
        std::sync::OnceLock::new();
    COUNTS.get_or_init(Default::default)
}

/// How many times FFmpeg has been opened for `path` this test binary.
#[cfg(test)]
pub(crate) fn probe_opens_for(path: &std::path::Path) -> u64 {
    let path = path.to_str().expect("test paths are UTF-8");
    probe_open_counts()
        .lock()
        .expect("probe open counts mutex poisoned")
        .get(path)
        .copied()
        .unwrap_or(0)
}

/// Forget the cached answer for one test path so a later call proves whether
/// the production path asked FFmpeg again rather than receiving a cache hit.
#[cfg(test)]
pub(crate) fn forget_probe_for(path: &std::path::Path) {
    PROBE_CACHE.remove(path.to_str().expect("test paths are UTF-8"));
}

/// Open the file with FFmpeg and read its best audio stream. `None` (already
/// logged) when it cannot be opened, has no audio stream, or states no usable
/// duration.
fn open_and_probe(path: &str) -> Option<ProbeResult> {
    #[cfg(test)]
    {
        *probe_open_counts()
            .lock()
            .expect("probe open counts mutex poisoned")
            .entry(path.to_string())
            .or_default() += 1;
    }
    unsafe {
        use ffmpeg_sys_next::*;

        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "probe_audio_from_path")?;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let duration = probed_duration(path, fmt_ctx, stream);
        let codecpar = (*stream).codecpar;
        let codec_id = (*codecpar).codec_id;
        let content_type = content_type_from_codec_id(codec_id);
        let sample_rate = (*codecpar).sample_rate as u32;
        let mut bits_per_sample = if !content_type.is_lossless_audio() {
            None
        } else if (*codecpar).bits_per_raw_sample > 0 {
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

        if let Some(duration) = duration {
            Some(ProbeResult {
                content_type,
                duration,
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

/// Convert a sample number to a timestamp in the stream's time base. For
/// FLAC/APE the time base is `1/sample_rate`, so the timestamp is the sample
/// number; otherwise scale by the time base. `None` when the time base is
/// unusable (non-positive numerator, or a zero sample rate) -- the caller then
/// bails out of the seek loop.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn sample_to_timestamp(
    sample: u64,
    time_base: ffmpeg_sys_next::AVRational,
    sample_rate: i64,
) -> Option<i64> {
    if time_base.num == 1 && time_base.den as i64 == sample_rate {
        Some(sample as i64)
    } else if sample_rate > 0 && time_base.num > 0 {
        Some(
            (sample as i128 * time_base.den as i128 / (time_base.num as i128 * sample_rate as i128))
                as i64,
        )
    } else {
        None
    }
}

/// The byte the demuxer reads from after seeking to each of `samples` -- read
/// from the AVIO position (`avio_tell`) right after the seek, the way the demuxer
/// itself positions. Recorded at import as each track's start byte.
///
/// Two reasons this reads a byte where reading the demuxer packet's `pos` returns
/// `None` or a wrong value:
/// - The AVIO position is defined for every format. APE packets carry no byte
///   position; a FLAC with placeholder seektable points returns none either.
/// - It does *not* set `AVFMT_FLAG_FAST_SEEK` (`open_best_audio_stream` leaves it
///   unset), so a FLAC seek binary-searches the real frames instead of trusting
///   the file's seektable -- landing accurately even when that seektable is coarse
///   or full of placeholder points. (APE ignores the flag and uses its mandatory
///   index either way.)
///
/// Binary-searching a FLAC reads the file's end here, once at import. Playback
/// never does: it byte-seeks straight to the recorded byte (FLAC) or sample-seeks
/// and prefetches it (APE). `None` only if the file can't be opened or a seek
/// fails outright.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn seek_landing_bytes(path: &str, samples: &[u64]) -> Option<Vec<u64>> {
    unsafe {
        use ffmpeg_sys_next::*;

        // Opens without AVFMT_FLAG_FAST_SEEK, so a FLAC seek binary-searches the
        // real frames (see the doc comment) rather than trusting the seektable.
        let (mut fmt_ctx, stream_index) = open_best_audio_stream(path, "seek_landing_bytes")?;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let time_base = (*stream).time_base;
        let sample_rate = (*(*stream).codecpar).sample_rate as i64;

        let mut offsets = Vec::with_capacity(samples.len());
        let mut failed: Option<String> = None;
        for &sample in samples {
            let Some(target_ts) = sample_to_timestamp(sample, time_base, sample_rate) else {
                failed = Some(format!("invalid time base for sample {sample}"));
                break;
            };
            if avformat_seek_file(fmt_ctx, stream_index, i64::MIN, target_ts, target_ts, 0) < 0 {
                failed = Some(format!("seek to sample {sample} failed"));
                break;
            }
            // Where the demuxer left the read cursor = the frame it will read next.
            // `avio_seek(.., 0, SEEK_CUR)` is `avio_tell`: returns the position
            // without moving it.
            let pos = avio_seek((*fmt_ctx).pb, 0, 1 /* SEEK_CUR */);
            if pos < 0 {
                failed = Some(format!("avio_tell after seek to sample {sample} failed"));
                break;
            }
            offsets.push(pos as u64);
        }

        avformat_close_input(&mut fmt_ctx);
        match failed {
            Some(reason) => {
                warn!("seek_landing_bytes: {reason} in {path}");
                None
            }
            None => Some(offsets),
        }
    }
}
