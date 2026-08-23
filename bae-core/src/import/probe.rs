//! What the folder's audio plays for, read off the disk once.
//!
//! Every playing time the import shows comes from opening a file, and opening
//! files is the one part of building the pane that costs anything. So it is
//! done in one place, by identification, and the numbers are stored: the
//! mapping table, the slot table and the Ready rule all read a
//! [`ProbedDurations`] value rather than an audio decoder.
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
//! `duration_ms: None` is "opened, and it would not say" — a file that will
//! not probe, or a sheet that gives its track no timing. The absence of a row
//! is the different fact that nothing has looked yet.

use crate::audio_codec::ProbeResult;
use crate::import::folder_scanner::CategorizedFiles;
use crate::import::track_slots::{audio_layout, UnitContribution};
use crate::import::types::{AudioFile, CueAnalyzedAudioFile, CueFlacAnalysis};
use crate::import::ImportError;
use std::collections::HashMap;
use tracing::warn;

/// One audio unit and what it plays for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedUnit {
    pub audio: AudioFile,
    /// `None` when the unit was read and states no length.
    pub duration_ms: Option<u64>,
}

/// Every audio unit of one candidate that has been read, with what it plays
/// for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbedDurations {
    pub units: Vec<ProbedUnit>,
}

impl ProbedDurations {
    pub fn new(units: Vec<ProbedUnit>) -> Self {
        Self { units }
    }

    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    /// What this unit plays for: `None` when nothing has read it, and
    /// `Some(None)` when it was read and states no length.
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

    /// One file's worth of measurement, totalling `total_ms` — for a test
    /// that cares only about the sum and has no folder behind it.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn totalling(total_ms: u64) -> Self {
        Self {
            units: vec![ProbedUnit {
                audio: AudioFile::Standalone {
                    file_id: "audio".to_string(),
                },
                duration_ms: Some(total_ms),
            }],
        }
    }

    /// Fold `other`'s rows in, replacing any row for the same unit.
    pub fn merge(&mut self, other: ProbedDurations) {
        for unit in other.units {
            match self
                .units
                .iter_mut()
                .find(|existing| existing.audio == unit.audio)
            {
                Some(existing) => existing.duration_ms = unit.duration_ms,
                None => self.units.push(unit),
            }
        }
    }
}

/// Open every audio unit of `files` once and record what it plays for.
///
/// One FFmpeg open per audio file, plus one parse-and-probe per bound sheet
/// shared by every slice carved out of it — so a twelve-track disc image costs
/// two opens, not twelve.
pub fn probe_durations(files: &CategorizedFiles) -> ProbedDurations {
    let mut units = Vec::new();
    for file in files.audio() {
        units.push(ProbedUnit {
            audio: AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            },
            duration_ms: probe_duration_ms(&file.path).map(|ms| ms.max(0) as u64),
        });
    }
    for (_, contribution) in audio_layout(files) {
        let UnitContribution::Runs(sheets) = contribution else {
            continue;
        };
        for sheet in sheets {
            let sheet_id = sheet.file.relative_path.as_str();
            let analysis = sheet_analysis(files, sheet_id).ok();
            for index in 0..sheet.sheet.playable_track_count() {
                units.push(ProbedUnit {
                    audio: AudioFile::SheetSlice {
                        file_id: sheet.audio.relative_path.clone(),
                        sheet_id: sheet_id.to_string(),
                        index: index as u32,
                    },
                    duration_ms: analysis
                        .as_ref()
                        .and_then(|analysis| slice_duration_ms(analysis, index)),
                });
            }
        }
    }
    ProbedDurations { units }
}

/// Open exactly the named units and record what they play for — the pane's
/// own read, for a candidate identification has not measured.
///
/// A unit naming audio the folder no longer holds is skipped rather than
/// written: it has nothing to measure, and a row saying so would claim the
/// folder was read when it was not.
pub fn probe_units(files: &CategorizedFiles, wanted: &[AudioFile]) -> ProbedDurations {
    let by_path: HashMap<&str, &crate::import::folder_scanner::ScannedFile> = files
        .audio()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    let mut sheets: HashMap<String, Option<CueFlacAnalysis>> = HashMap::new();
    let mut units = Vec::with_capacity(wanted.len());
    for audio in wanted {
        let Some(file) = by_path.get(audio.file_id()) else {
            warn!("{} is not this folder's audio", audio.file_id());
            continue;
        };
        let duration_ms = match audio {
            AudioFile::Standalone { .. } => {
                probe_duration_ms(&file.path).map(|ms| ms.max(0) as u64)
            }
            AudioFile::SheetSlice {
                sheet_id, index, ..
            } => sheets
                .entry(sheet_id.clone())
                .or_insert_with(|| sheet_analysis(files, sheet_id).ok())
                .as_ref()
                .and_then(|analysis| slice_duration_ms(analysis, *index as usize)),
        };
        units.push(ProbedUnit {
            audio: audio.clone(),
            duration_ms,
        });
    }
    ProbedDurations { units }
}

/// One slice's own playing time: the sheet's exact timing, or — for the last
/// track, which has no next-track boundary in the sheet — the container's total
/// minus the slice's start. The same reading the commit writes, so what the
/// pane showed is what lands on the track.
fn slice_duration_ms(analysis: &CueFlacAnalysis, index: usize) -> Option<u64> {
    let cue_track = analysis.cue_sheet.playable_tracks().nth(index)?;
    let duration = cue_track
        .track_duration_ms()
        .map(|d| d as i64)
        .or_else(|| {
            analysis
                .audio_files
                .iter()
                .find(|file| file.file_reference == cue_track.file_reference)
                .map(|file| {
                    file.probe.duration.as_millis() as i64 - cue_track.start_time_ms() as i64
                })
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
        let path = if single_file {
            sheet.audio.path.clone()
        } else {
            cue_dir.join(file_reference)
        };
        let probe = analyze_cue_audio(&path)?;
        audio_files.push(CueAnalyzedAudioFile {
            file_reference: file_reference.to_string(),
            path,
            probe,
        });
    }

    Ok(CueFlacAnalysis {
        cue_sheet: sheet.sheet.clone(),
        audio_files,
    })
}

/// A standalone audio file's duration. `None` just means the track lands
/// without one — `probe_audio_from_path` logs its own failure reason.
pub(crate) fn probe_duration_ms(file_path: &std::path::Path) -> Option<i64> {
    let Some(path_str) = file_path.to_str() else {
        warn!(
            "Cannot probe duration for non-UTF-8 path: {}",
            file_path.display()
        );
        return None;
    };
    let probe = crate::audio_codec::probe_audio_from_path(path_str)?;
    Some(probe.duration.as_millis() as i64)
}

/// Probe a container a track sheet carves tracks out of.
///
/// The codec is not re-judged here: the scan probes the audio a sheet names and
/// refuses the binding outright when bae cannot carve that container, so a
/// sheet that is still bound after the commit's own walk names a container that
/// already passed. What is left is the file being readable at all.
pub(crate) fn analyze_cue_audio(audio_path: &std::path::Path) -> Result<ProbeResult, ImportError> {
    let path_str = audio_path
        .to_str()
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("non-UTF-8 audio path: {:?}", audio_path),
        })?;
    crate::audio_codec::probe_audio_from_path(path_str).ok_or_else(|| ImportError::UnusableFile {
        detail: format!("audio file could not be read: {:?}", audio_path),
    })
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
