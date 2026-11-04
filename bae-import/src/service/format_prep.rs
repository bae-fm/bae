//! Build the audio-format records for an import from scan facts.
//!
//! Derives per-track `DbAudioFormat` rows (CUE-backed tracks share their
//! image's analysis; standalone files reuse their scan facts), each with the
//! segments of the file it plays from.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use crate::types::{CueFlacAnalysis, TrackAudio, TrackFile};
use crate::ImportError;
use bae_db::{DbAudioFormat, DbAudioSegment, DbAudioSegmentRole, SegmentSpan};

use super::ImportService;

mod cue_segments;

use cue_segments::{cue_audio_pregap, cue_file_landing_bytes};

fn ensure_probe_audio_format(
    path: &Path,
    probe: &bae_audio::ProbeResult,
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

/// A track's sample rate or channel count as its audio format row stores it:
/// above zero and within a `u32`. `source` names the file it was read from.
fn track_audio_count(source: &Path, quantity: &str, raw: i64) -> Result<NonZeroU32, ImportError> {
    u32::try_from(raw).ok().and_then(NonZeroU32::new).ok_or_else(|| ImportError::UnusableFile {
        detail: format!("{} states {quantity} {raw}", source.display()),
    })
}

/// A track's bit depth as its audio format row stores it: none for lossy
/// audio, else 1 to 32.
fn track_bit_depth(source: &Path, raw: Option<i64>) -> Result<Option<NonZeroU32>, ImportError> {
    raw.map(|raw| {
        u32::try_from(raw)
            .ok()
            .filter(|bits| (1..=32).contains(bits))
            .and_then(NonZeroU32::new)
            .ok_or_else(|| ImportError::UnusableFile {
                detail: format!(
                    "{} states a {raw}-bit depth; bae stores 1 to 32 bits",
                    source.display()
                ),
            })
    })
    .transpose()
}

struct BuiltAudioFormat {
    format: DbAudioFormat,
    segments: Vec<DbAudioSegment>,
}

fn cue_track_by_playable_index(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
) -> Result<&crate::cue_sheet::CueTrack, ImportError> {
    cue_pair.cue_sheet.playable_tracks().nth(cue_index).ok_or_else(|| ImportError::Internal {
        detail: format!("CUE playable track index {cue_index} out of bounds"),
    })
}

/// The scanned audio file a `FILE` directive of the sheet names. Every file
/// a bound sheet names was resolved and probed by the scan.
fn cue_audio_file<'a>(
    cue_pair: &'a CueFlacAnalysis,
    file_reference: &str,
) -> Result<&'a crate::types::CueAnalyzedAudioFile, ImportError> {
    cue_pair.audio_file(file_reference).ok_or_else(|| ImportError::Internal {
        detail: format!("CUE references audio without scan facts: {file_reference}"),
    })
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
    let audio = cue_audio_file(cue_pair, &cue_track.file_reference)?;
    let probe = &audio.probe;
    ensure_probe_audio_format(&audio.path, probe)?;
    let audio_pregap = cue_audio_pregap(cue_pair, cue_index)?;
    let audio_pregap_samples = audio_pregap
        .as_ref()
        .map(|pregap| pregap.end_sample - pregap.start_sample)
        .filter(|&samples| samples > 0);
    let generated_pregap_samples =
        cue_track.generated_pregap_frames.filter(|&frames| frames > 0).map(|frames| {
            crate::cue_sheet::cue_frames_to_samples(frames, u64::from(probe.sample_rate))
        });

    Ok(DbAudioFormat::new(
        db_track_id,
        probe.content_type.clone(),
        track_audio_count(&audio.path, "sample rate", i64::from(probe.sample_rate))?,
        track_bit_depth(&audio.path, probe.bits_per_sample.map(i64::from))?,
        track_audio_count(&audio.path, "channel count", i64::from(probe.channels))?,
        id,
        now,
    )
    .with_pregaps(audio_pregap_samples, generated_pregap_samples))
}

/// Build an audio format for a per-track file from the scan's authoritative facts.
fn standalone_audio_format(
    db_track_id: &str,
    file_path: &Path,
    source_audio: &crate::folder_scanner::ScannedAudio,
    id: String,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<bae_db::DbAudioFormat, ImportError> {
    // A per-track file is its own whole-file source; the sample window lives in
    // its `audio_format_segments` row.
    Ok(DbAudioFormat::new(
        db_track_id,
        source_audio.content_type.clone(),
        track_audio_count(file_path, "sample rate", source_audio.format.sample_rate_hz)?,
        track_bit_depth(file_path, source_audio.format.bits_per_sample)?,
        track_audio_count(file_path, "channel count", source_audio.format.channels)?,
        id,
        now,
    ))
}

impl ImportService {
    fn audio_segment(
        audio_format_id: &str,
        segment_index: i64,
        role: DbAudioSegmentRole,
        file_id: &str,
        span: SegmentSpan,
        ids: &dyn coven::IdSource,
        now: chrono::DateTime<chrono::Utc>,
    ) -> DbAudioSegment {
        DbAudioSegment {
            id: ids.new_id().to_string(),
            audio_format_id: audio_format_id.to_string(),
            segment_index,
            role,
            file_id: file_id.to_string(),
            start_sample: span.start_sample,
            end_sample: span.end_sample,
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            created_at: now,
        }
    }

    /// Build audio format records for all tracks from the scan's stored facts,
    /// reporting to `bugs` a CUE image whose track starts can't be read.
    pub fn build_audio_formats(
        tracks_to_files: &[TrackFile],
        file_ids: &HashMap<PathBuf, String>,
        clock: &dyn coven::Clock,
        ids: &dyn coven::IdSource,
        bugs: &bae_diagnostics::Diagnostics,
    ) -> Result<BuiltAudioFormats, ImportError> {
        let now = chrono::DateTime::<chrono::Utc>::from(clock.now());
        let mut audio_formats = Vec::with_capacity(tracks_to_files.len());
        let mut audio_segments = Vec::new();
        let mut cue_landings_by_file: HashMap<PathBuf, Option<HashMap<u64, u64>>> = HashMap::new();

        for track_file in tracks_to_files {
            let db_track = &track_file.db_track;
            let built = match &track_file.audio {
                TrackAudio::CueBacked { cue_pair, cue_index, .. } => {
                    let af = cue_backed_audio_format(
                        &db_track.id,
                        cue_pair,
                        *cue_index,
                        ids.new_id().to_string(),
                        now,
                    )?;
                    for audio_file in &cue_pair.audio_files {
                        cue_landings_by_file.entry(audio_file.path.clone()).or_insert_with(|| {
                            match cue_file_landing_bytes(audio_file, &cue_pair.cue_sheet) {
                                Ok(landings) => Some(landings),
                                // The image's tracks hold no byte window and
                                // keep the whole-file read-ahead span.
                                Err(reason) => {
                                    bugs.report_bug(
                                        "read where each track of a CUE image starts",
                                        &format!("{}: {reason}", audio_file.path.display()),
                                    );
                                    None
                                }
                            }
                        });
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
                    BuiltAudioFormat { format: af, segments }
                }
                TrackAudio::Standalone { file_path, source_audio } => {
                    let file_id = file_ids.get(file_path).ok_or_else(|| ImportError::Internal {
                        detail: format!("No DbFile registered for track source {file_path:?}"),
                    })?;
                    let af = standalone_audio_format(
                        &db_track.id,
                        file_path,
                        source_audio,
                        ids.new_id().to_string(),
                        now,
                    )?;
                    let segment = Self::audio_segment(
                        &af.id,
                        0,
                        DbAudioSegmentRole::Main,
                        file_id,
                        SegmentSpan::whole_file(),
                        ids,
                        now,
                    );
                    BuiltAudioFormat { format: af, segments: vec![segment] }
                }
            };

            audio_segments.extend(built.segments);
            audio_formats.push(built.format);
        }

        Ok(BuiltAudioFormats { audio_formats, audio_segments })
    }
}

pub struct BuiltAudioFormats {
    pub audio_formats: Vec<DbAudioFormat>,
    pub audio_segments: Vec<DbAudioSegment>,
}

#[cfg(test)]
#[path = "format_prep_tests.rs"]
mod tests;
