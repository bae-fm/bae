//! Where each CUE track's audio lies in the image it is cut from: its pregap
//! and main segments, by sample, and the bytes a seek to each boundary lands
//! on.

use super::{
    audio_segment, cue_audio_file, cue_track_by_playable_index, ensure_probe_audio_format,
};
use crate::types::CueFlacAnalysis;
use crate::ImportError;
use bae_audio::ProbeResult;
use bae_records::{DbAudioSegment, DbAudioSegmentRole, SegmentSpan};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(super) struct CueAudioPregap {
    pub(super) file_reference: String,
    pub(super) start_sample: u64,
    pub(super) end_sample: u64,
}

fn probe_duration_samples(probe: &ProbeResult) -> Result<u64, ImportError> {
    u64::try_from(probe.duration.as_nanos() * u128::from(probe.sample_rate.get()) / 1_000_000_000)
        .map_err(|error| ImportError::Internal {
            detail: format!("CUE audio duration exceeds the sample-count range: {error}"),
        })
}

/// Resolve an audio pregap in the coordinate system of the file containing its
/// INDEX 00. When INDEX 01 moves to another file, the pregap is the tail of the
/// file before, so it runs to that file's end rather than to INDEX 01's
/// file-local zero.
pub(super) fn cue_audio_pregap(
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
) -> Result<Option<CueAudioPregap>, ImportError> {
    let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
    let Some(index) = cue_track.audio_pregap() else {
        return Ok(None);
    };
    let audio = cue_audio_file(cue_pair, &index.file_reference)?;
    let probe = &audio.probe;
    ensure_probe_audio_format(&audio.path, probe)?;
    let sample_rate = u64::from(probe.sample_rate.get());
    let start_sample = crate::cue_sheet::cue_frames_to_samples(index.frames, sample_rate);
    let end_sample = if index.file_reference == cue_track.file_reference {
        crate::cue_sheet::cue_frames_to_samples(cue_track.start_cue_frames, sample_rate)
    } else {
        probe_duration_samples(probe)?
    };
    if end_sample < start_sample {
        return Err(ImportError::Internal {
            detail: format!(
                "CUE track {} pregap starts after it ends in {}",
                cue_track.number, index.file_reference
            ),
        });
    }
    Ok(Some(CueAudioPregap {
        file_reference: index.file_reference.clone(),
        start_sample,
        end_sample,
    }))
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
/// The scan opened and probed every image a sheet names, and admitted only
/// audio of a usable format, so these offsets can always be read.
/// An error saying why they couldn't fails the import.
pub(super) fn cue_file_landing_bytes(
    file: &crate::types::CueAnalyzedAudioFile,
    cue_sheet: &crate::cue_sheet::CueSheet,
) -> Result<HashMap<u64, u64>, ImportError> {
    ensure_probe_audio_format(&file.path, &file.probe)?;
    let sample_rate = u64::from(file.probe.sample_rate.get());
    let start_samples: Vec<u64> = cue_sheet
        .playable_tracks()
        .flat_map(|track| track.indexes.iter())
        .filter(|index| index.file_reference == file.file_reference)
        .filter(|index| matches!(index.number, 0 | 1))
        .map(|index| crate::cue_sheet::cue_frames_to_samples(index.frames, sample_rate))
        .collect();
    let landings = bae_audio::seek_landing_bytes(&file.path, &start_samples).map_err(|reason| {
        ImportError::Internal {
            detail: format!(
                "could not read where each track of the CUE image {} starts: {reason}",
                file.path.display()
            ),
        }
    })?;
    Ok(start_samples.into_iter().zip(landings).collect())
}

fn cue_segment_byte(
    byte_landings_by_file: &HashMap<PathBuf, HashMap<u64, u64>>,
    file_path: &Path,
    sample: u64,
) -> Option<u64> {
    if sample == 0 {
        return None;
    }
    byte_landings_by_file.get(file_path).and_then(|map| map.get(&sample)).copied()
}

pub(super) fn cue_segments(
    audio_format_id: &str,
    cue_pair: &CueFlacAnalysis,
    cue_index: usize,
    file_ids: &HashMap<PathBuf, String>,
    byte_landings_by_file: &HashMap<PathBuf, HashMap<u64, u64>>,
    ids: &dyn coven::IdSource,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<Vec<DbAudioSegment>, ImportError> {
    let cue_track = cue_track_by_playable_index(cue_pair, cue_index)?;
    let main_audio = cue_audio_file(cue_pair, &cue_track.file_reference)?;
    let main_path = main_audio.path.as_path();
    let probe = &main_audio.probe;
    ensure_probe_audio_format(main_path, probe)?;
    let sample_rate = u64::from(probe.sample_rate.get());
    let mut segments = Vec::new();

    if let Some(pregap) = cue_audio_pregap(cue_pair, cue_index)? {
        let pregap_path = cue_audio_file(cue_pair, &pregap.file_reference)?.path.as_path();
        let pregap_file_id = file_ids.get(pregap_path).ok_or_else(|| ImportError::Internal {
            detail: format!("No DbFile registered for CUE pregap source {pregap_path:?}"),
        })?;
        let start_sample = pregap.start_sample;
        let end_sample = Some(pregap.end_sample);
        let start_byte = cue_segment_byte(byte_landings_by_file, pregap_path, start_sample);
        let end_byte = end_sample
            .and_then(|sample| cue_segment_byte(byte_landings_by_file, pregap_path, sample));
        segments.push(audio_segment(
            audio_format_id,
            segments.len() as i64,
            DbAudioSegmentRole::AudioPregap,
            pregap_file_id,
            SegmentSpan { start_sample, end_sample, start_byte, end_byte },
            ids,
            now,
        ));
    }

    let main_file_id = file_ids.get(main_path).ok_or_else(|| ImportError::Internal {
        detail: format!("No DbFile registered for CUE source {main_path:?}"),
    })?;
    let start_sample =
        crate::cue_sheet::cue_frames_to_samples(cue_track.start_cue_frames, sample_rate);
    // Where the sheet's slice ends — the same boundary its length and its
    // audition window read.
    let end_sample = cue_track
        .end_cue_frames
        .map(|frames| crate::cue_sheet::cue_frames_to_samples(frames, sample_rate));
    let start_byte = cue_segment_byte(byte_landings_by_file, main_path, start_sample);
    let end_byte =
        end_sample.and_then(|sample| cue_segment_byte(byte_landings_by_file, main_path, sample));
    segments.push(audio_segment(
        audio_format_id,
        segments.len() as i64,
        DbAudioSegmentRole::Main,
        main_file_id,
        SegmentSpan { start_sample, end_sample, start_byte, end_byte },
        ids,
        now,
    ));

    Ok(segments)
}

#[cfg(test)]
#[path = "cue_segments_tests.rs"]
mod tests;
