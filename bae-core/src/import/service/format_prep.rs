//! Probe-and-build the audio-format and content-type records for an import.
//!
//! Resolves each discovered file's `ContentType`, derives per-track
//! `DbAudioFormat` rows (CUE-backed tracks share their image's analysis;
//! standalone files are probed), and computes CUE track byte windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::import::types::{CueFlacAnalysis, TrackFile};
use crate::util::content_type::ContentType;
use crate::util::content_type_hint::ContentTypeHint;

use super::ImportService;

fn admitted_audio_content_type(content_type: &ContentType) -> bool {
    matches!(
        content_type,
        ContentType::Flac
            | ContentType::Mp3
            | ContentType::Ape
            | ContentType::Alac
            | ContentType::Aac
            | ContentType::Pcm
            | ContentType::Opus
            | ContentType::Vorbis
            | ContentType::WavPack
            | ContentType::Dsd
    )
}

/// Resolve the probe-verified `ContentType` for a discovered file.
///
/// Audio files are probed (only the decoder knows which codec the container
/// holds — `.m4a` could be ALAC or AAC, and the extension alone can't tell).
/// Non-audio files get their `ContentType` derived from the extension hint:
/// that's an honest mapping for images (an extension on image bytes predicts
/// the codec), text, and PDF. Anything the hint can't classify becomes
/// `OctetStream`, which flows through the DB as-is.
pub(super) fn resolve_file_content_type(path: &Path) -> Result<ContentType, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| format!("File has no extension: {:?}", path))?;
    let hint = ContentTypeHint::from_extension(ext);

    if hint.is_audio() {
        let path_str = path
            .to_str()
            .ok_or_else(|| format!("Invalid path: {}", path.display()))?;
        let probe = crate::audio_codec::probe_audio_from_path(path_str)
            .ok_or_else(|| format!("Failed to probe audio file: {}", path.display()))?;
        if !admitted_audio_content_type(&probe.content_type) {
            return Err(format!(
                "Unsupported audio codec {} in {}",
                probe.content_type.display_name(),
                path.display()
            ));
        }
        return Ok(probe.content_type);
    }

    Ok(match hint {
        ContentTypeHint::Jpeg => ContentType::Jpeg,
        ContentTypeHint::Png => ContentType::Png,
        ContentTypeHint::Gif => ContentType::Gif,
        ContentTypeHint::Webp => ContentType::Webp,
        ContentTypeHint::Bmp => ContentType::Bmp,
        ContentTypeHint::Svg => ContentType::Svg,
        ContentTypeHint::PlainText => ContentType::PlainText,
        ContentTypeHint::Pdf => ContentType::Pdf,
        // Audio hints were handled above; anything else is unknown binary.
        _ => ContentType::OctetStream,
    })
}

/// Build an audio format for a track inside a CUE-backed file (shared FLAC or APE image).
fn cue_backed_audio_format(
    db_track_id: &str,
    file_path: &Path,
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, String> {
    use crate::db::DbAudioFormat;

    let cue_track = cue_pair.cue_sheet.tracks.get(cue_index).ok_or_else(|| {
        format!(
            "CUE track index {} out of bounds for {}",
            cue_index,
            file_path.display()
        )
    })?;

    let fmt = cue_analysis_format(cue_pair);
    let audio_pregap_ms = cue_track
        .pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);
    let generated_pregap_ms = cue_track
        .generated_pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);
    let audio_pregap_samples = cue_track
        .pregap_cue_frames
        .map(|pregap| cue_track.start_cue_frames.saturating_sub(pregap))
        .filter(|&frames| frames > 0)
        .map(|frames| (frames * fmt.sample_rate as u64 / 75) as i64);
    let generated_pregap_samples = cue_track
        .generated_pregap_frames
        .filter(|&frames| frames > 0)
        .map(|frames| (frames * fmt.sample_rate as u64 / 75) as i64);

    // Every CUE codec decodes its shared file natively and is trimmed to the
    // track's sample window -- one shape across FLAC, APE and ALAC.
    let start_sample = cue_track.audio_start_sample(fmt.sample_rate);
    let end_sample = cue_track.end_sample(fmt.sample_rate);
    Ok(DbAudioFormat::new(
        db_track_id,
        fmt.content_type,
        fmt.sample_rate as i64,
        fmt.bits_per_sample,
        fmt.channels as i64,
        start_sample as i64,
        end_sample.map(|s| s as i64),
        id,
        now,
    )
    .with_pregap(audio_pregap_ms)
    .with_generated_pregap(generated_pregap_ms)
    .with_pregap_samples(audio_pregap_samples)
    .with_generated_pregap_samples(generated_pregap_samples))
}

/// Build an audio format for a per-track file (FLAC, MP3, APE, etc.) via FFmpeg probe.
fn standalone_probed_audio_format(
    db_track_id: &str,
    file_path: &Path,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, String> {
    use crate::db::DbAudioFormat;

    let path_str = file_path
        .to_str()
        .ok_or_else(|| format!("Invalid path: {}", file_path.display()))?;
    let probe = crate::audio_codec::probe_audio_from_path(path_str)
        .ok_or_else(|| format!("Failed to probe audio file: {}", file_path.display()))?;
    if !admitted_audio_content_type(&probe.content_type) {
        return Err(format!(
            "Unsupported audio codec {} in {}",
            probe.content_type.display_name(),
            file_path.display()
        ));
    }

    // A per-track file is its own whole-file window: (0, None) samples and the
    // default (0, None) byte span -- the whole file.
    Ok(DbAudioFormat::new(
        db_track_id,
        probe.content_type,
        probe.sample_rate as i64,
        probe.bits_per_sample.map(|b| b as i64),
        probe.channels as i64,
        0,
        None,
        id,
        now,
    ))
}

/// The audio format descriptor of a CUE-backed file, read once from its analysis
/// and shared by every caller that needs any of these fields, so the per-codec
/// extraction lives in one match instead of being repeated per field.
struct CueAnalysisFormat {
    content_type: ContentType,
    sample_rate: u32,
    bits_per_sample: Option<i64>,
    channels: u32,
}

fn cue_analysis_format(cue_pair: &CueFlacAnalysis) -> CueAnalysisFormat {
    let probe = &cue_pair.analysis.probe;
    CueAnalysisFormat {
        content_type: probe.content_type.clone(),
        sample_rate: probe.sample_rate,
        bits_per_sample: probe.bits_per_sample.map(|b| b as i64),
        channels: probe.channels,
    }
}

/// Byte each CUE track's audio *starts* on within the shared file: the byte the
/// demuxer lands on when seeking to that track's `start_sample`, found via
/// `seek_landing_bytes` (one open per file, no decode). `starts[i]` is track
/// `i`'s start byte, index-aligned to the CUE tracks.
///
/// This one list gives both boundaries of every track, because a CUE track's
/// byte range is `[start[i], start[i + 1])`: track `i` ends exactly where track
/// `i + 1`'s audio begins, so its read-ahead ceiling is the next track's start
/// byte. The last track has no next start and runs to EOF. The first track starts
/// at byte 0 (nothing to prefetch), so the caller stores `None` for it. `None`
/// here means the offsets couldn't be read (non-UTF-8 path or a failed seek),
/// distinct from a real result; the caller then stores no start or end byte for
/// that file's tracks and they keep the whole-file read-ahead span.
fn cue_track_start_bytes(file_path: &Path, cue_pair: &CueFlacAnalysis) -> Option<Vec<u64>> {
    let sample_rate = cue_analysis_format(cue_pair).sample_rate;
    let start_samples: Vec<u64> = cue_pair
        .cue_sheet
        .tracks
        .iter()
        .map(|t| t.audio_start_sample(sample_rate))
        .collect();
    let Some(path) = file_path.to_str() else {
        warn!("cue_track_start_bytes: non-UTF-8 path, cannot seek byte offsets: {file_path:?}");
        return None;
    };
    crate::audio_codec::seek_landing_bytes(path, &start_samples)
}

impl ImportService {
    /// Build audio format records for all tracks. CUE-backed tracks already hold
    /// their shared analysis + index; standalone tracks are probed here.
    pub(super) fn build_audio_formats(
        tracks_to_files: &[TrackFile],
        file_ids: &HashMap<PathBuf, String>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<Vec<crate::db::DbAudioFormat>, String> {
        let now = clock.now();
        let mut audio_formats = Vec::with_capacity(tracks_to_files.len());
        // Seektable landing byte for each track's `start_sample` per shared CUE
        // file, computed once per file (one ffmpeg open, not per track) and reused
        // for both boundaries: a track starts at its own landing and ends at the
        // next track's. `None` means the offsets couldn't be read for that file.
        let mut cue_starts_by_file: HashMap<PathBuf, Option<Vec<u64>>> = HashMap::new();

        for track_file in tracks_to_files {
            // Each track carries the absolute path to its source file; that path
            // is the `file_ids` key — no bare-filename lookup that could collide
            // across disc subfolders.
            let file_id = file_ids.get(track_file.file_path()).ok_or_else(|| {
                format!(
                    "No DbFile registered for track source {:?}",
                    track_file.file_path()
                )
            })?;

            let format = match track_file {
                TrackFile::CueBacked {
                    db_track,
                    file_path,
                    cue_pair,
                    cue_index,
                } => {
                    let af = cue_backed_audio_format(
                        &db_track.id,
                        file_path,
                        cue_pair,
                        *cue_index,
                        ids.new_id(),
                        now,
                    )?;
                    // One seek pass per file gives every track's start byte; the
                    // range of track N is [start[N], start[N+1]). When the offsets
                    // can't be read, all this file's tracks keep the default
                    // whole-file span and lose the parallel prefetch.
                    let starts = cue_starts_by_file.entry(file_path.clone()).or_insert_with(|| {
                        let computed = cue_track_start_bytes(file_path, cue_pair);
                        if computed.is_none() {
                            warn!(
                                "track start offsets unavailable for {:?}; its tracks keep the whole-file read-ahead span and lose the parallel prefetch",
                                file_path
                            );
                        }
                        computed
                    });
                    // The first track (start_sample 0) begins at byte 0 — nothing
                    // to prefetch, so it keeps the default `None` start byte. A
                    // deep track records the seektable landing for its start.
                    let start_byte = if af.start_sample > 0 {
                        starts
                            .as_ref()
                            .and_then(|s| s.get(*cue_index))
                            .map(|&b| b as i64)
                    } else {
                        None
                    };
                    // The read-ahead ceiling is where the next track's audio
                    // starts. The last track has no next start, so it stays `None`
                    // and runs to EOF.
                    let end_byte = starts
                        .as_ref()
                        .and_then(|s| s.get(*cue_index + 1))
                        .map(|&b| b as i64);
                    af.with_end_byte(end_byte).with_start_byte(start_byte)
                }
                TrackFile::Standalone {
                    db_track,
                    file_path,
                } => standalone_probed_audio_format(&db_track.id, file_path, ids.new_id(), now)?,
            };

            audio_formats.push(format.with_file_id(file_id));
        }

        Ok(audio_formats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_codec_allowlist_is_the_intentional_import_surface() {
        for content_type in [
            ContentType::Flac,
            ContentType::Mp3,
            ContentType::Ape,
            ContentType::Alac,
            ContentType::Aac,
            ContentType::Pcm,
            ContentType::Opus,
            ContentType::Vorbis,
            ContentType::WavPack,
            ContentType::Dsd,
        ] {
            assert!(admitted_audio_content_type(&content_type));
        }

        assert!(!admitted_audio_content_type(&ContentType::Other(
            "codec:AV_CODEC_ID_SPEEX".to_string()
        )));
        assert!(!admitted_audio_content_type(&ContentType::Other(
            "audio/x-ms-wma".to_string()
        )));
    }
}
