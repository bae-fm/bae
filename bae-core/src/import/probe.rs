//! What the folder's audio plays for, derived from the scan's stored facts.
//!
//! The scan opens each physical audio file and stores its facts on the
//! candidate. The mapping table, slot table, and Ready rule project their
//! per-file and per-sheet-entry durations from that one stored shape.
//!
//! Two kinds of row, matching the two kinds of [`AudioFile`]:
//!
//! * **file** — one per audio file the folder holds, whatever job it does.
//!   A standalone track's own length, and a container's whole length where a
//!   track sheet carves it. Summing these is the candidate's total playing
//!   time, which is why a container counts once and its slices not at all.
//! * **slice** — one per track a bound sheet carves, timed by the sheet, with
//!   the container's total closing the last one.
//!
//! `duration_ms: None` is a sheet entry that supplies no timing. Physical audio
//! with no readable duration never becomes a valid scanned candidate.

use crate::audio_codec::ProbeResult;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::track_slots::{audio_layout, UnitContribution};
use crate::import::types::{AudioFile, CueAnalyzedAudioFile, CueFlacAnalysis};
use crate::import::ImportError;

/// One audio unit and what it plays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDuration {
    pub audio: AudioFile,
    /// `None` when the unit was read and states no length.
    pub duration_ms: Option<u64>,
}

/// Every effective audio unit of one candidate, with what it plays for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceDurations {
    pub units: Vec<SourceDuration>,
}

impl SourceDurations {
    pub fn new(units: Vec<SourceDuration>) -> Self {
        Self { units }
    }

    /// What this unit plays for: no row when it is not part of the effective
    /// layout, and `Some(None)` when a sheet entry states no length.
    pub fn duration_of(&self, audio: &AudioFile) -> Option<Option<u64>> {
        self.units
            .iter()
            .find(|unit| &unit.audio == audio)
            .map(|unit| unit.duration_ms)
    }

    /// The candidate's total playing time, summed over its audio files — each
    /// file once, whether it holds one track or a whole disc a sheet carves.
    ///
    /// `0` when any file states no length. A partial sum understates the total
    /// and would read downstream as "the durations disagree" — a wrong answer
    /// where the honest one is "we don't know", and the Ready rule refuses
    /// both.
    pub fn total_ms(&self) -> u64 {
        let mut total: u64 = 0;
        for unit in &self.units {
            let AudioFile::Standalone { .. } = unit.audio else {
                continue;
            };
            match unit.duration_ms {
                Some(ms) => total = total.saturating_add(ms),
                None => return 0,
            }
        }
        total
    }

    /// One file's worth of duration, totalling `total_ms` — for a test
    /// that cares only about the sum and has no folder behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn totalling(total_ms: u64) -> Self {
        Self {
            units: vec![SourceDuration {
                audio: AudioFile::Standalone {
                    file_id: "audio".to_string(),
                },
                duration_ms: Some(total_ms),
            }],
        }
    }
}
/// Project every audio unit from the scan's authoritative per-file facts.
pub fn source_durations(files: &CategorizedFiles) -> SourceDurations {
    let mut units = Vec::new();
    for file in files.audio() {
        units.push(SourceDuration {
            audio: AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            },
            duration_ms: file.source_audio.as_ref().map(|audio| audio.duration_ms),
        });
    }
    for (_, contribution) in audio_layout(files) {
        let UnitContribution::Runs(sheets) = contribution else {
            continue;
        };
        for sheet in sheets {
            let sheet_id = sheet.file.relative_path.as_str();
            for index in 0..sheet.sheet.playable_track_count() {
                units.push(SourceDuration {
                    audio: AudioFile::SheetSlice {
                        file_id: sheet.audio.relative_path.clone(),
                        sheet_id: sheet_id.to_string(),
                        index: index as u32,
                    },
                    duration_ms: slice_duration_ms(files, &sheet, index),
                });
            }
        }
    }
    SourceDurations { units }
}

/// One slice's own playing time: the sheet's exact timing, or — for the last
/// track, which has no next-track boundary in the sheet — the container's total
/// minus the slice's start. The same reading the commit writes, so what the
/// pane showed is what lands on the track.
fn slice_duration_ms(
    files: &CategorizedFiles,
    sheet: &crate::import::folder_scanner::BoundTrackSheet<'_>,
    index: usize,
) -> Option<u64> {
    let cue_track = sheet.sheet.playable_tracks().nth(index)?;
    let duration = cue_track
        .track_duration_ms()
        .map(|d| d as i64)
        .or_else(|| {
            scanned_audio_for_reference(files, sheet, &cue_track.file_reference)
                .and_then(|file| file.source_audio.as_ref())
                .map(|audio| audio.duration_ms as i64 - cue_track.start_time_ms() as i64)
        })?;
    (duration >= 0).then_some(duration as u64)
}

/// Parse-and-probe one bound track sheet, once, for every slice carved out of
/// it. The audio a sheet naming a single file is analyzed against is the audio
/// it is *bound* to, not what its `FILE` directive spells: a sheet written for
/// a WAV that was later encoded to FLAC still says `WAV`, and re-resolving the
/// directive here would refuse the very pairing the user corrected. A sheet
/// naming one file per track has no single binding to stand in, so its
/// references resolve as written.
pub(crate) fn sheet_analysis(
    files: &CategorizedFiles,
    sheet_id: &str,
) -> Result<CueFlacAnalysis, ImportError> {
    let bound = files.bound_sheets();
    let sheet = bound
        .iter()
        .find(|bound| bound.file.relative_path == sheet_id)
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{sheet_id} no longer describes any of the folder's audio"),
        })?;

    let cue_dir = sheet
        .file
        .path
        .parent()
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{sheet_id} has no parent directory"),
        })?;
    let single_file = sheet.sheet.single_file().is_some();

    let mut audio_files = Vec::new();
    for file_reference in sheet.sheet.audio_file_references() {
        let file = if single_file {
            sheet.audio
        } else {
            files
                .audio()
                .find(|audio| audio.path == cue_dir.join(file_reference))
                .ok_or_else(|| ImportError::UnusableFile {
                    detail: format!("{sheet_id} references missing audio {file_reference}"),
                })?
        };
        let source_audio = file
            .source_audio
            .as_ref()
            .ok_or_else(|| ImportError::UnusableFile {
                detail: format!("{} has no scanned audio facts", file.relative_path),
            })?;
        audio_files.push(CueAnalyzedAudioFile {
            file_reference: file_reference.to_string(),
            path: file.path.clone(),
            probe: ProbeResult {
                content_type: source_audio.content_type.clone(),
                duration: std::time::Duration::from_millis(source_audio.duration_ms),
                sample_rate: source_audio.format.sample_rate_hz as u32,
                bits_per_sample: source_audio.format.bits_per_sample.map(|bits| bits as u32),
                bitrate_kbps: source_audio.format.bitrate_kbps,
                channels: source_audio.format.channels as u32,
            },
        });
    }

    Ok(CueFlacAnalysis {
        cue_sheet: sheet.sheet.clone(),
        audio_files,
    })
}

fn scanned_audio_for_reference<'a>(
    files: &'a CategorizedFiles,
    sheet: &crate::import::folder_scanner::BoundTrackSheet<'_>,
    file_reference: &str,
) -> Option<&'a crate::import::folder_scanner::ScannedFile> {
    if sheet.sheet.single_file().is_some() {
        return files
            .audio()
            .find(|audio| audio.relative_path == sheet.audio.relative_path);
    }
    let cue_dir = sheet.file.path.parent()?;
    let path = cue_dir.join(file_reference);
    files.audio().find(|audio| audio.path == path)
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
