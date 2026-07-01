//! Probe-and-build the audio-format and content-type records for an import.
//!
//! Resolves each discovered file's `ContentType`, derives per-track
//! `DbAudioFormat` rows (CUE-backed tracks share their image's analysis;
//! standalone files are probed), and computes CUE track byte windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::import::types::{CueAudioAnalysis, CueFlacAnalysis, TrackFile};
use crate::util::content_type::ContentType;
use crate::util::content_type_hint::ContentTypeHint;

use super::ImportService;

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

    let pregap_ms = cue_track
        .pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| ms as i64);

    // Every CUE codec decodes its shared file natively and is trimmed to the
    // track's sample window -- one shape across FLAC, APE and ALAC.
    let fmt = cue_analysis_format(cue_pair);
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
    .with_pregap(pregap_ms))
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
    match &cue_pair.analysis {
        CueAudioAnalysis::Flac { flac_info } => CueAnalysisFormat {
            content_type: ContentType::Flac,
            sample_rate: flac_info.sample_rate,
            bits_per_sample: Some(flac_info.bits_per_sample as i64),
            channels: flac_info.channels,
        },
        CueAudioAnalysis::Ape { ape_info } => CueAnalysisFormat {
            content_type: ContentType::Ape,
            sample_rate: ape_info.sample_rate,
            bits_per_sample: Some(ape_info.bits_per_sample as i64),
            channels: ape_info.channels as u32,
        },
        CueAudioAnalysis::Alac {
            sample_rate,
            channels,
            bits_per_sample,
            ..
        } => CueAnalysisFormat {
            content_type: ContentType::Alac,
            sample_rate: *sample_rate,
            bits_per_sample: bits_per_sample.map(|b| b as i64),
            channels: *channels,
        },
    }
}

/// Byte offset of each CUE track's end within the shared file, found by seeking
/// (no decode) -- computed once per file. `ends[i]` is track `i`'s end byte; the
/// last track has no entry and runs to EOF. The boundary is each track's
/// `end_sample` -- the same sample `cue_backed_audio_format` trims to -- so the
/// read-ahead ceiling and the decode window can't drift. `None` when the offsets
/// can't be read (non-UTF-8 path, a non-last track missing its end sample, or a
/// failed seek), distinct from a real empty result; the caller then spans the
/// whole file.
fn cue_track_byte_ends(file_path: &Path, cue_pair: &CueFlacAnalysis) -> Option<Vec<u64>> {
    let sample_rate = cue_analysis_format(cue_pair).sample_rate;
    let tracks = &cue_pair.cue_sheet.tracks;
    // Seek to each non-last track's end sample (the last track runs to EOF, so it
    // has no end sample); every other track must have one.
    let end_samples = match tracks
        .iter()
        .take(tracks.len().saturating_sub(1))
        .map(|t| t.end_sample(sample_rate))
        .collect::<Option<Vec<u64>>>()
    {
        Some(samples) => samples,
        None => {
            warn!("cue_track_byte_ends: a non-last track has no end sample in {file_path:?}");
            return None;
        }
    };
    let Some(path) = file_path.to_str() else {
        warn!("cue_track_byte_ends: non-UTF-8 path, cannot seek byte offsets: {file_path:?}");
        return None;
    };
    crate::audio_codec::frame_byte_offsets(path, &end_samples)
}

/// Byte each CUE track's audio *starts* on within the shared file: the seektable
/// checkpoint the playback seek lands on when seeking to that track's
/// `start_sample`, found via `seek_landing_bytes` (one open per file, no decode).
/// `starts[i]` is track `i`'s start byte, index-aligned to the CUE tracks. The
/// first track starts at byte 0 (nothing to prefetch), so the caller stores
/// `None` for it. `None` here means the offsets couldn't be read (non-UTF-8 path
/// or a failed seek), distinct from a real result; the caller then stores no
/// start byte for that file's tracks.
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
        // Track end byte offsets per shared CUE file, computed once and reused
        // across that file's tracks (one ffmpeg open per file, not per track).
        // `None` means the offsets couldn't be read for that file.
        let mut cue_ends_by_file: HashMap<PathBuf, Option<Vec<u64>>> = HashMap::new();
        // Track start byte offsets per shared CUE file, same caching as the ends:
        // the seektable landing for each track's `start_sample`, computed once per
        // file. `None` means the offsets couldn't be read for that file.
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
                    // The last track (no entry) runs to EOF, as does any track
                    // whose offsets couldn't be read -- both keep the default
                    // whole-file span.
                    let ends = cue_ends_by_file.entry(file_path.clone()).or_insert_with(|| {
                        let computed = cue_track_byte_ends(file_path, cue_pair);
                        if computed.is_none() {
                            warn!(
                                "track byte offsets unavailable for {:?}; its tracks keep the whole-file read-ahead span",
                                file_path
                            );
                        }
                        computed
                    });
                    let end_byte = ends
                        .as_ref()
                        .and_then(|e| e.get(*cue_index))
                        .map(|&b| b as i64);
                    // The first track (start_sample 0) begins at byte 0 — nothing
                    // to prefetch, so it keeps the default `None` start byte. A
                    // deep track records the seektable landing for its start.
                    let start_byte = if af.start_sample > 0 {
                        let starts =
                            cue_starts_by_file.entry(file_path.clone()).or_insert_with(|| {
                                let computed = cue_track_start_bytes(file_path, cue_pair);
                                if computed.is_none() {
                                    warn!(
                                        "track start offsets unavailable for {:?}; its tracks lose the parallel prefetch",
                                        file_path
                                    );
                                }
                                computed
                            });
                        starts
                            .as_ref()
                            .and_then(|s| s.get(*cue_index))
                            .map(|&b| b as i64)
                    } else {
                        None
                    };
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
