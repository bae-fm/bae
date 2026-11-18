//! Probing and format detection: open a file at a path, inspect its best audio
//! stream, and report container/codec properties (`ProbeResult`). The desktop
//! builds also find where a seek to a sample lands (`seek_landing`).

use crate::avio::{
    close_input_and_free_custom_avio, file_avio_read_callback, file_avio_seek_callback,
    free_custom_avio_context, FileAvioContext,
};
use bae_util::audio_container::AudioContainer;
use bae_util::content_type::ContentType;
use coven::LeasePlace;
use std::os::raw::{c_int, c_void};
use std::path::Path;
use std::ptr;
use tracing::{debug, warn};

#[cfg(desktop)]
pub(crate) mod seek_landing;

/// Probe-discovered audio properties. Populated from `AVCodecID` and
/// `AVCodecParameters`, so every field reflects what FFmpeg actually sees
/// in the bytes rather than what the file extension promises.
#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub content_type: ContentType,
    /// The container the stream is in, from the demuxer FFmpeg read the
    /// bytes with.
    pub container: AudioContainer,
    pub duration: std::time::Duration,
    /// The stream's sample rate in Hz.
    pub sample_rate: std::num::NonZeroU32,
    /// Source bit depth for lossless audio. Perceptual codecs are `None` even
    /// when FFmpeg exposes a coded-word width.
    pub bits_per_sample: Option<u32>,
    /// Average coded-stream bitrate for perceptual codecs. Container metadata,
    /// embedded artwork, and other non-audio bytes are excluded.
    pub bitrate_kbps: Option<i64>,
    /// The stream's channel count.
    pub channels: std::num::NonZeroU32,
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

/// Map the demuxer FFmpeg opened a file with to the container it reads. A
/// demuxer bae names no container for lands in `Other("demuxer:{name}")`, so
/// it survives storage and is visibly not one of ours.
pub(super) fn container_from_demuxer(name: &str) -> AudioContainer {
    match name {
        "flac" => AudioContainer::Flac,
        "mp3" => AudioContainer::Mp3,
        "wav" => AudioContainer::Wav,
        "aiff" => AudioContainer::Aiff,
        "ogg" => AudioContainer::Ogg,
        "mov,mp4,m4a,3gp,3g2,mj2" => AudioContainer::Mp4,
        "aac" => AudioContainer::Adts,
        "ape" => AudioContainer::Ape,
        "wv" => AudioContainer::WavPack,
        "dsf" => AudioContainer::Dsf,
        // FFmpeg reads DSDIFF with its IFF demuxer.
        "iff" => AudioContainer::Dsdiff,
        other => AudioContainer::Other(format!("demuxer:{other}")),
    }
}

/// Why a file gave no probe.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Reading the file failed (it would not open, or a read of it errored),
    /// which says nothing of what the file holds.
    #[error("the file could not be read: {0}")]
    Io(#[source] std::io::Error),
    /// The file was read and holds no audio stream bae can use, for this
    /// reason.
    #[error("{0}")]
    NotAudio(String),
}

/// A file FFmpeg has opened through the handle Rust opened it with (see
/// [`FileAvioContext`]): the format context reading it, the AVIO context under
/// that, and the file. Dropping it closes all three.
struct OpenedFile {
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    avio: *mut ffmpeg_sys_next::AVIOContext,
    file: *mut FileAvioContext,
}

impl OpenedFile {
    /// The first failed read of the file, taken.
    fn take_read_error(&mut self) -> Option<std::io::Error> {
        // SAFETY: `file` is the box this opening owns until it drops.
        unsafe { (*self.file).take_read_error() }
    }
}

impl Drop for OpenedFile {
    fn drop(&mut self) {
        unsafe {
            close_input_and_free_custom_avio(&mut self.fmt_ctx, self.avio);
            drop(Box::from_raw(self.file));
        }
    }
}

/// Open the file at `path` in `place` for FFmpeg to read, and find its best
/// audio stream. Returns the opened file and the stream index, or why not: a
/// read of the file failed (`Io`), or its stream info can't be read or it has
/// no audio stream (`NotAudio`). Closing the file returns its place to the
/// lease.
unsafe fn open_best_audio_stream(
    path: &Path,
    place: LeasePlace,
) -> Result<(OpenedFile, c_int), ProbeError> {
    use ffmpeg_sys_next::*;

    let file = place.open(path).map_err(ProbeError::Io)?;
    let file = Box::into_raw(Box::new(FileAvioContext::new(file)));
    // FFmpeg takes ownership of this buffer.
    let buffer = av_malloc(super::AVIO_BUFFER_SIZE) as *mut u8;
    if buffer.is_null() {
        drop(Box::from_raw(file));
        return Err(ProbeError::NotAudio("failed to allocate the AVIO buffer".to_string()));
    }
    let avio = avio_alloc_context(
        buffer,
        super::AVIO_BUFFER_SIZE as c_int,
        0,
        file as *mut c_void,
        Some(file_avio_read_callback),
        None,
        Some(file_avio_seek_callback),
    );
    if avio.is_null() {
        av_free(buffer as *mut c_void);
        drop(Box::from_raw(file));
        return Err(ProbeError::NotAudio("failed to create the AVIO context".to_string()));
    }
    let mut fmt_ctx = avformat_alloc_context();
    if fmt_ctx.is_null() {
        free_custom_avio_context(avio);
        drop(Box::from_raw(file));
        return Err(ProbeError::NotAudio("failed to allocate the format context".to_string()));
    }
    (*fmt_ctx).pb = avio;
    // No name: FFmpeg reads only through `avio`. A failed open frees the
    // format context, never the AVIO context it was given.
    let opened = avformat_open_input(&mut fmt_ctx, ptr::null(), ptr::null(), ptr::null_mut());
    if opened < 0 {
        free_custom_avio_context(avio);
        let mut file = Box::from_raw(file);
        return Err(match file.take_read_error() {
            Some(error) => ProbeError::Io(error),
            None => ProbeError::NotAudio(format!(
                "failed to open input ({})",
                super::av_err_str(opened)
            )),
        });
    }
    let mut opened = OpenedFile { fmt_ctx, avio, file };
    let found = avformat_find_stream_info(opened.fmt_ctx, ptr::null_mut());
    if let Some(error) = opened.take_read_error() {
        return Err(ProbeError::Io(error));
    }
    if found < 0 {
        return Err(ProbeError::NotAudio(format!(
            "failed to read stream info ({})",
            super::av_err_str(found)
        )));
    }
    let stream_index = av_find_best_stream(
        opened.fmt_ctx,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );
    if stream_index < 0 {
        return Err(ProbeError::NotAudio("no audio stream".to_string()));
    }
    Ok((opened, stream_index))
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
    path: &Path,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream: *mut ffmpeg_sys_next::AVStream,
) -> Option<std::time::Duration> {
    if let Some(duration) = duration_from_time_base((*stream).duration, (*stream).time_base) {
        return Some(duration);
    }

    let duration_us = (*fmt_ctx).duration;
    if duration_us > 0 {
        debug!(
            "probe_audio_from_path: using container duration for {}; stream duration was unavailable",
            path.display()
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
    path: &Path,
    fmt_ctx: *mut ffmpeg_sys_next::AVFormatContext,
    stream_index: c_int,
    duration: std::time::Duration,
) -> Option<i64> {
    use ffmpeg_sys_next::*;

    let path = path.display();
    let stream = *(*fmt_ctx).streams.add(stream_index as usize);
    if let Some(kbps) = kbps_from_bps((*(*stream).codecpar).bit_rate) {
        return Some(kbps);
    }

    let start = crate::sample_clock::stream_origin(stream);
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

/// A count FFmpeg states as a signed int — a stream's sample rate or channel
/// count — or `None` when it states none: zero, or less.
fn stated_count(value: c_int) -> Option<std::num::NonZeroU32> {
    u32::try_from(value).ok().and_then(std::num::NonZeroU32::new)
}

/// The audio properties of the file at `path`, read from its current bytes
/// with the file open in `place`: `NotAudio` when it holds no audio stream
/// bae can use, `Io` when reading the file failed, which says nothing of what
/// it holds.
pub fn probe_audio_from_path(path: &Path, place: LeasePlace) -> Result<ProbeResult, ProbeError> {
    open_and_probe(path, place)
}

/// Open the file and read its best audio stream through FFmpeg. `NotAudio`
/// when it has no audio stream, or states no sample rate, channels or usable
/// duration; `Io` when a read of it failed, wherever FFmpeg met the failure.
fn open_and_probe(path: &Path, place: LeasePlace) -> Result<ProbeResult, ProbeError> {
    #[cfg(feature = "test-instrumentation")]
    crate::test_reads::record_probe(path);
    unsafe {
        let (mut opened, stream_index) = open_best_audio_stream(path, place)?;
        let fmt_ctx = opened.fmt_ctx;
        let stream = *(*fmt_ctx).streams.add(stream_index as usize);
        let duration = probed_duration(path, fmt_ctx, stream);
        let codecpar = (*stream).codecpar;
        let codec_id = (*codecpar).codec_id;
        let content_type = content_type_from_codec_id(codec_id);
        let container = container_from_demuxer(
            &std::ffi::CStr::from_ptr((*(*fmt_ctx).iformat).name).to_string_lossy(),
        );
        let Some(sample_rate) = stated_count((*codecpar).sample_rate) else {
            return Err(ProbeError::NotAudio(format!(
                "the audio stream states sample rate {}",
                (*codecpar).sample_rate
            )));
        };
        let bits_per_sample = if !content_type.is_lossless_audio() {
            None
        } else if (*codecpar).bits_per_raw_sample > 0 {
            Some((*codecpar).bits_per_raw_sample as u32)
        } else if (*codecpar).bits_per_coded_sample > 0 {
            Some((*codecpar).bits_per_coded_sample as u32)
        } else {
            None
        };
        let Some(channels) = stated_count((*codecpar).ch_layout.nb_channels) else {
            return Err(ProbeError::NotAudio(format!(
                "the audio stream states {} channels",
                (*codecpar).ch_layout.nb_channels
            )));
        };

        let bitrate_kbps = if content_type.is_lossless_audio() {
            None
        } else {
            duration.and_then(|duration| {
                probed_lossy_bitrate_kbps(path, fmt_ctx, stream_index, duration)
            })
        };
        // A read that failed while measuring the bitrate leaves it unmeasured,
        // which is no fact about the file.
        if let Some(error) = opened.take_read_error() {
            return Err(ProbeError::Io(error));
        }

        drop(opened);

        match duration {
            Some(duration) => Ok(ProbeResult {
                content_type,
                container,
                duration,
                sample_rate,
                bits_per_sample,
                bitrate_kbps,
                channels,
            }),
            None => Err(ProbeError::NotAudio("no usable container duration".to_string())),
        }
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
