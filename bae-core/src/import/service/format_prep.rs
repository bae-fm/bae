//! Build the audio-format and content-type records for an import from scan facts.
//!
//! Resolves each discovered file's `ContentType`, derives per-track
//! `DbAudioFormat` rows (CUE-backed tracks share their image's analysis;
//! standalone files reuse their scan facts), and computes CUE track byte windows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::audio_codec::ProbeResult;
use crate::db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, SegmentSpan};
use crate::import::types::{CueFlacAnalysis, TrackFile};
use crate::import::ImportError;
use crate::util::content_type::ContentType;
use crate::util::content_type_hint::ContentTypeHint;

use super::ImportService;

fn ensure_probe_audio_format(
    path: &Path,
    probe: &crate::audio_codec::ProbeResult,
) -> Result<(), ImportError> {
    if probe.sample_rate == 0 || probe.channels == 0 {
        return Err(ImportError::UnusableFile {
            detail: format!(
                "unusable audio format in {}: sample_rate={}, channels={}",
                path.display(),
                probe.sample_rate,
                probe.channels
            ),
        });
    }
    if !probe.content_type.is_supported_audio() {
        return Err(ImportError::UnusableFile {
            detail: format!(
                "Unsupported audio codec {} in {}",
                probe.content_type.display_name(),
                path.display()
            ),
        });
    }
    Ok(())
}

/// The probe-verified `ContentType` for a discovered file.
///
/// Audio files reuse the scan's probe facts: only the decoder knows which codec
/// the container holds — a `.m4a` could be ALAC or AAC, and the extension can't say. Non-audio
/// files take their `ContentType` from the extension hint, an honest mapping for
/// images (an extension on image bytes does predict the codec), text, and PDF.
/// Anything the hint can't classify becomes `OctetStream` and flows through the
/// DB as-is — including a file with no extension at all, which a rip folder can
/// legitimately hold (a `README`, a `.`-less checksum file) and which the
/// release carries like any other file.
pub(super) fn resolve_file_content_type(
    file: &crate::import::folder_scanner::ScannedFile,
) -> Result<ContentType, ImportError> {
    if let Some(audio) = &file.source_audio {
        return Ok(audio.content_type.clone());
    }
    let path = &file.path;
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return Ok(ContentType::OctetStream);
    };
    let hint = ContentTypeHint::from_extension(ext);

    if hint.is_audio() {
        return Err(ImportError::UnusableFile {
            detail: format!("{} has no scanned audio facts", path.display()),
        });
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

struct BuiltAudioFormat {
    format: DbAudioFormat,
    segments: Vec<DbAudioSegment>,
}

fn cue_track_by_playable_index(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
) -> Result<&crate::cue_flac::CueTrack, ImportError> {
    cue_pair
        .cue_sheet
        .playable_tracks()
        .nth(cue_index)
        .ok_or_else(|| ImportError::Internal {
            detail: format!("CUE playable track index {cue_index} out of bounds"),
        })
}

fn cue_file_probe<'a>(
    cue_pair: &'a CueFlacAnalysis,
    file_reference: &str,
) -> Result<&'a ProbeResult, ImportError> {
    cue_pair
        .audio_files
        .iter()
        .find(|file| file.file_reference == file_reference)
        .map(|file| &file.probe)
        .ok_or_else(|| ImportError::Internal {
            detail: format!("CUE references audio without scan facts: {file_reference}"),
        })
}

fn cue_file_path<'a>(
    cue_pair: &'a CueFlacAnalysis,
    file_reference: &str,
) -> Result<&'a Path, ImportError> {
    cue_pair
        .audio_files
        .iter()
        .find(|file| file.file_reference == file_reference)
        .map(|file| file.path.as_path())
        .ok_or_else(|| ImportError::Internal {
            detail: format!("CUE references unmapped audio file: {file_reference}"),
        })
}

struct CueAudioPregap {
    file_reference: String,
    start_sample: u64,
    end_sample: u64,
    duration_ms: u64,
}

fn probe_duration_samples(probe: &ProbeResult) -> Result<u64, ImportError> {
    u64::try_from(probe.duration.as_nanos() * u128::from(probe.sample_rate) / 1_000_000_000)
        .map_err(|_| ImportError::Internal {
            detail: "CUE audio duration exceeds the sample-count range".to_string(),
        })
}

fn cue_audio_measure(value: u64, quantity: &str) -> Result<i64, ImportError> {
    i64::try_from(value).map_err(|_| ImportError::Internal {
        detail: format!("CUE {quantity} exceeds the database integer range"),
    })
}

/// Resolve an audio pregap in the coordinate system of the file containing its
/// INDEX 00. When INDEX 01 moves to another file, the pregap ends at the prior
/// file's next boundary or EOF rather than at INDEX 01's file-local zero.
fn cue_audio_pregap(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
) -> Result<Option<CueAudioPregap>, ImportError> {
    let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
    let crate::cue_flac::CuePregap::Audio(index) = &cue_track.pregap else {
        return Ok(None);
    };
    let probe = cue_file_probe(cue_pair, &index.file_reference)?;
    let path = cue_file_path(cue_pair, &index.file_reference)?;
    ensure_probe_audio_format(path, probe)?;
    let sample_rate = u64::from(probe.sample_rate);
    let start_sample = crate::cue_flac::cue_frames_to_samples(index.frames, sample_rate);
    let end_sample = if index.file_reference == cue_track.file_reference {
        crate::cue_flac::cue_frames_to_samples(cue_track.start_cue_frames, sample_rate)
    } else if let Some(end_frames) =
        cue_segment_end_frames(cue_pair, cue_index, &index.file_reference)
    {
        crate::cue_flac::cue_frames_to_samples(end_frames, sample_rate)
    } else {
        probe_duration_samples(probe)?
    };
    let sample_count =
        end_sample
            .checked_sub(start_sample)
            .ok_or_else(|| ImportError::Internal {
                detail: format!(
                    "CUE track {} pregap starts after it ends in {}",
                    cue_track.number, index.file_reference
                ),
            })?;
    let duration_ms = u64::try_from(u128::from(sample_count) * 1_000 / u128::from(sample_rate))
        .map_err(|_| ImportError::Internal {
            detail: "CUE pregap duration exceeds the millisecond range".to_string(),
        })?;
    Ok(Some(CueAudioPregap {
        file_reference: index.file_reference.clone(),
        start_sample,
        end_sample,
        duration_ms,
    }))
}

/// Build track-level audio format metadata for a CUE-backed track.
fn cue_backed_audio_format(
    db_track_id: &str,
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<DbAudioFormat, ImportError> {
    let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
    let cue_path = cue_file_path(cue_pair, &cue_track.file_reference)?;

    let probe = cue_file_probe(cue_pair, &cue_track.file_reference)?;
    ensure_probe_audio_format(cue_path, probe)?;
    let audio_pregap = cue_audio_pregap(cue_pair, cue_index)?;
    let audio_pregap_ms = audio_pregap
        .as_ref()
        .filter(|pregap| pregap.duration_ms > 0)
        .map(|pregap| cue_audio_measure(pregap.duration_ms, "pregap milliseconds"))
        .transpose()?;
    let generated_pregap_ms = cue_track
        .generated_pregap_duration_ms()
        .filter(|&ms| ms > 0)
        .map(|ms| cue_audio_measure(ms, "generated pregap milliseconds"))
        .transpose()?;
    let audio_pregap_samples = audio_pregap
        .as_ref()
        .map(|pregap| pregap.end_sample - pregap.start_sample)
        .filter(|&samples| samples > 0)
        .map(|samples| cue_audio_measure(samples, "pregap samples"))
        .transpose()?;
    let generated_pregap_samples = cue_track
        .generated_pregap_frames()
        .filter(|&frames| frames > 0)
        .map(|frames| {
            cue_audio_measure(
                crate::cue_flac::cue_frames_to_samples(frames, u64::from(probe.sample_rate)),
                "generated pregap samples",
            )
        })
        .transpose()?;

    Ok(DbAudioFormat::new(
        db_track_id,
        probe.content_type.clone(),
        probe.sample_rate as i64,
        probe.bits_per_sample.map(|b| b as i64),
        probe.channels as i64,
        id,
        now,
    )
    .with_pregap(audio_pregap_ms)
    .with_generated_pregap(generated_pregap_ms)
    .with_pregap_samples(audio_pregap_samples)
    .with_generated_pregap_samples(generated_pregap_samples))
}

/// Build an audio format for a per-track file from the scan's authoritative facts.
fn standalone_audio_format(
    db_track_id: &str,
    source_audio: &crate::import::folder_scanner::ScannedAudio,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<crate::db::DbAudioFormat, ImportError> {
    // A per-track file is its own whole-file source; the sample window lives in
    // its `audio_format_segments` row.
    Ok(DbAudioFormat::new(
        db_track_id,
        source_audio.content_type.clone(),
        source_audio.format.sample_rate_hz,
        source_audio.format.bits_per_sample,
        source_audio.format.channels,
        id,
        now,
    ))
}

/// The byte each CUE track's audio *starts* on within the shared file: where the
/// demuxer lands when seeking to that track's `start_sample`, from
/// `seek_landing_bytes` (one open per file, no decode).
///
/// This one list gives both boundaries of every track, because a CUE track's
/// byte range is `[start[i], start[i + 1])` — track `i` ends exactly where track
/// `i + 1`'s audio begins, so its read-ahead ceiling is the next track's start
/// byte. The last track has no next start and runs to EOF, and the first starts
/// at byte 0 with nothing to prefetch, so the caller stores `None` for both.
///
/// A `None` return means the offsets couldn't be read at all (non-UTF-8 path, a
/// failed seek); the caller then stores no byte window for that file's tracks and
/// they keep the whole-file read-ahead span.
fn cue_file_landing_bytes(
    file_path: &Path,
    cue_pair: &CueFlacAnalysis,
) -> Option<HashMap<u64, u64>> {
    let file = cue_pair
        .audio_files
        .iter()
        .find(|file| file.path == file_path)?;
    if let Err(error) = ensure_probe_audio_format(file_path, &file.probe) {
        warn!("Skipping CUE byte landing for unusable audio format: {error}");
        return None;
    }
    let sample_rate = file.probe.sample_rate;
    let start_samples: Vec<u64> = cue_pair
        .cue_sheet
        .playable_tracks()
        .flat_map(|track| track.indexes.iter())
        .filter(|index| index.file_reference == file.file_reference)
        .filter(|index| matches!(index.number, 0 | 1))
        .map(|index| crate::cue_flac::cue_frames_to_samples(index.frames, sample_rate as u64))
        .collect();
    let Some(path) = file_path.to_str() else {
        warn!("cue_track_start_bytes: non-UTF-8 path, cannot seek byte offsets: {file_path:?}");
        return None;
    };
    let landings = crate::audio_codec::seek_landing_bytes(path, &start_samples)?;
    Some(start_samples.into_iter().zip(landings).collect())
}

fn cue_segment_end_frames(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    file_reference: &str,
) -> Option<u64> {
    cue_pair
        .cue_sheet
        .playable_tracks()
        .skip(cue_index + 1)
        .flat_map(|track| track.indexes.iter())
        .find(|index| index.file_reference == file_reference && matches!(index.number, 0 | 1))
        .map(|index| index.frames)
}

impl ImportService {
    fn audio_segment(
        audio_format_id: &str,
        segment_index: i64,
        role: DbAudioSegmentRole,
        file_id: &str,
        span: SegmentSpan,
        ids: &dyn coven::IdProvider,
        now: chrono::DateTime<chrono::Utc>,
    ) -> DbAudioSegment {
        DbAudioSegment {
            id: ids.new_id(),
            audio_format_id: audio_format_id.to_string(),
            segment_index,
            role,
            file_id: file_id.to_string(),
            start_sample: span.start_sample as i64,
            end_sample: span.end_sample.map(|sample| sample as i64),
            start_byte: span.start_byte.map(|byte| byte as i64),
            end_byte: span.end_byte.map(|byte| byte as i64),
            created_at: now,
        }
    }

    fn cue_segment_byte(
        byte_landings_by_file: &HashMap<PathBuf, Option<HashMap<u64, u64>>>,
        file_path: &Path,
        sample: u64,
    ) -> Option<u64> {
        if sample == 0 {
            return None;
        }
        byte_landings_by_file
            .get(file_path)
            .and_then(|map| map.as_ref())
            .and_then(|map| map.get(&sample))
            .copied()
    }

    fn cue_segments(
        audio_format_id: &str,
        cue_pair: &CueFlacAnalysis,
        cue_index: usize,
        file_ids: &HashMap<PathBuf, String>,
        byte_landings_by_file: &HashMap<PathBuf, Option<HashMap<u64, u64>>>,
        ids: &dyn coven::IdProvider,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<DbAudioSegment>, ImportError> {
        let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
        let main_path = cue_file_path(cue_pair, &cue_track.file_reference)?;
        let probe = cue_file_probe(cue_pair, &cue_track.file_reference)?;
        ensure_probe_audio_format(main_path, probe)?;
        let sample_rate = probe.sample_rate as u64;
        let mut segments = Vec::new();

        if let Some(pregap) = cue_audio_pregap(cue_pair, cue_index)? {
            let pregap_path = cue_file_path(cue_pair, &pregap.file_reference)?;
            let pregap_file_id =
                file_ids
                    .get(pregap_path)
                    .ok_or_else(|| ImportError::Internal {
                        detail: format!(
                            "No DbFile registered for CUE pregap source {pregap_path:?}"
                        ),
                    })?;
            let start_sample = pregap.start_sample;
            let end_sample = Some(pregap.end_sample);
            let start_byte =
                Self::cue_segment_byte(byte_landings_by_file, pregap_path, start_sample);
            let end_byte = end_sample.and_then(|sample| {
                Self::cue_segment_byte(byte_landings_by_file, pregap_path, sample)
            });
            segments.push(Self::audio_segment(
                audio_format_id,
                segments.len() as i64,
                DbAudioSegmentRole::AudioPregap,
                pregap_file_id,
                SegmentSpan {
                    start_sample,
                    end_sample,
                    start_byte,
                    end_byte,
                },
                ids,
                now,
            ));
        }

        let main_file_id = file_ids
            .get(main_path)
            .ok_or_else(|| ImportError::Internal {
                detail: format!("No DbFile registered for CUE source {main_path:?}"),
            })?;
        let start_sample =
            crate::cue_flac::cue_frames_to_samples(cue_track.start_cue_frames, sample_rate);
        let end_sample = cue_segment_end_frames(cue_pair, cue_index, &cue_track.file_reference)
            .map(|frames| crate::cue_flac::cue_frames_to_samples(frames, sample_rate));
        let start_byte = Self::cue_segment_byte(byte_landings_by_file, main_path, start_sample);
        let end_byte = end_sample
            .and_then(|sample| Self::cue_segment_byte(byte_landings_by_file, main_path, sample));
        segments.push(Self::audio_segment(
            audio_format_id,
            segments.len() as i64,
            DbAudioSegmentRole::Main,
            main_file_id,
            SegmentSpan {
                start_sample,
                end_sample,
                start_byte,
                end_byte,
            },
            ids,
            now,
        ));

        Ok(segments)
    }

    /// Build audio format records for all tracks from the scan's stored facts.
    pub(super) fn build_audio_formats(
        tracks_to_files: &[TrackFile],
        file_ids: &HashMap<PathBuf, String>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdProvider,
    ) -> Result<BuiltAudioFormats, ImportError> {
        let now = clock.now();
        let mut audio_formats = Vec::with_capacity(tracks_to_files.len());
        let mut audio_segments = Vec::new();
        let mut cue_landings_by_file: HashMap<PathBuf, Option<HashMap<u64, u64>>> = HashMap::new();

        for track_file in tracks_to_files {
            let built = match track_file {
                TrackFile::CueBacked {
                    db_track,
                    cue_pair,
                    cue_index,
                    ..
                } => {
                    let af = cue_backed_audio_format(
                        &db_track.id,
                        cue_pair,
                        *cue_index,
                        ids.new_id(),
                        now,
                    )?;
                    for audio_file in &cue_pair.audio_files {
                        cue_landings_by_file
                            .entry(audio_file.path.clone())
                            .or_insert_with(|| cue_file_landing_bytes(&audio_file.path, cue_pair));
                    }
                    let segments = Self::cue_segments(
                        &af.id,
                        cue_pair,
                        *cue_index,
                        file_ids,
                        &cue_landings_by_file,
                        ids,
                        now,
                    )?;
                    BuiltAudioFormat {
                        format: af,
                        segments,
                    }
                }
                TrackFile::Standalone {
                    db_track,
                    file_path,
                    source_audio,
                } => {
                    let file_id = file_ids
                        .get(file_path)
                        .ok_or_else(|| ImportError::Internal {
                            detail: format!("No DbFile registered for track source {file_path:?}"),
                        })?;
                    let af =
                        standalone_audio_format(&db_track.id, source_audio, ids.new_id(), now)?;
                    let segment = Self::audio_segment(
                        &af.id,
                        0,
                        DbAudioSegmentRole::Main,
                        file_id,
                        SegmentSpan::whole_file(),
                        ids,
                        now,
                    );
                    BuiltAudioFormat {
                        format: af,
                        segments: vec![segment],
                    }
                }
            };

            audio_segments.extend(built.segments);
            audio_formats.push(built.format);
        }

        Ok(BuiltAudioFormats {
            audio_formats,
            audio_segments,
        })
    }
}

pub(super) struct BuiltAudioFormats {
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
}

#[cfg(test)]
#[path = "format_prep_tests.rs"]
mod tests;
