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
//! Every row carries a duration. Physical audio with no readable duration never
//! becomes a valid scanned candidate; CUE slices are closed by the next INDEX
//! 01 boundary or by the referenced audio file's probed end.

use crate::audio_codec::ProbeResult;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::track_slots::{audio_layout, UnitContribution};
use crate::import::types::{AudioFile, CueAnalyzedAudioFile, CueFlacAnalysis};
use crate::import::ImportError;

/// One audio unit and what it plays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDuration {
    pub audio: AudioFile,
    pub duration_ms: u64,
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

    /// What this unit plays for; no row when it is not part of this duration
    /// set's effective layout.
    pub fn duration_of(&self, audio: &AudioFile) -> Option<u64> {
        self.units
            .iter()
            .find(|unit| &unit.audio == audio)
            .map(|unit| unit.duration_ms)
    }

    /// The candidate's total playing time, summed over its audio files — each
    /// file once, whether it holds one track or a whole disc a sheet carves.
    pub fn total_ms(&self) -> u64 {
        self.units
            .iter()
            .filter(|unit| matches!(unit.audio, AudioFile::Standalone { .. }))
            .fold(0u64, |total, unit| {
                total
                    .checked_add(unit.duration_ms)
                    .expect("a candidate's total audio duration fits u64")
            })
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
                duration_ms: total_ms,
            }],
        }
    }
}
/// Project every audio unit from the scan's authoritative per-file facts.
pub fn source_durations(files: &CategorizedFiles) -> Result<SourceDurations, ImportError> {
    let mut units = Vec::new();
    for file in files.audio() {
        let source_audio = file
            .source_audio
            .as_ref()
            .ok_or_else(|| ImportError::UnusableFile {
                detail: format!("{} has no scanned audio facts", file.relative_path),
            })?;
        units.push(SourceDuration {
            audio: AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            },
            duration_ms: source_audio.duration_ms,
        });
    }
    for (_, contribution) in audio_layout(files) {
        let UnitContribution::Runs(sheets) = contribution else {
            continue;
        };
        for sheet in sheets {
            let sheet_id = sheet.file.relative_path.as_str();
            let analysis = sheet_analysis(files, sheet_id)?;
            for (index, track) in sheet.sheet.playable_tracks().enumerate() {
                let audio = sheet
                    .audio_files
                    .iter()
                    .find(|(file_reference, _)| *file_reference == track.file_reference)
                    .expect("a bound sheet resolved every playable track's audio");
                units.push(SourceDuration {
                    audio: AudioFile::SheetSlice {
                        file_id: audio.1.relative_path.clone(),
                        sheet_id: sheet_id.to_string(),
                        index: index as u32,
                    },
                    duration_ms: sheet_track_duration_ms(&analysis, index, sheet_id)?,
                });
            }
        }
    }
    Ok(SourceDurations { units })
}

/// One CUE slice's playing time: the next INDEX 01 boundary closes every track
/// except the last one in a referenced file, whose probed end closes it.
pub(crate) fn sheet_track_duration_ms(
    analysis: &CueFlacAnalysis,
    index: usize,
    sheet_id: &str,
) -> Result<u64, ImportError> {
    let cue_track = analysis
        .cue_sheet
        .playable_tracks()
        .nth(index)
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{sheet_id} no longer describes a track {}", index + 1),
        })?;
    let audio = analysis
        .audio_files
        .iter()
        .find(|file| file.file_reference == cue_track.file_reference)
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!(
                "{sheet_id} track {} references missing audio {}",
                index + 1,
                cue_track.file_reference
            ),
        })?;
    let total_ms =
        u64::try_from(audio.probe.duration.as_millis()).map_err(|_| ImportError::UnusableFile {
            detail: format!(
                "{} is too long to represent in milliseconds",
                audio.path.display()
            ),
        })?;
    cue_track
        .duration_ms_with_file_duration(total_ms)
        .map_err(|error| ImportError::UnusableFile {
            detail: format!("{sheet_id}: {error}"),
        })
}

/// Parse-and-probe one bound track sheet, once, for every slice carved out of
/// it. The scan has already resolved every CUE `FILE` reference to the
/// candidate's physical audio, including extension changes.
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

    let mut audio_files = Vec::new();
    for (file_reference, file) in &sheet.audio_files {
        let source_audio = file
            .source_audio
            .as_ref()
            .ok_or_else(|| ImportError::UnusableFile {
                detail: format!("{} has no scanned audio facts", file.relative_path),
            })?;
        audio_files.push(CueAnalyzedAudioFile {
            file_reference: (*file_reference).to_string(),
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

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
