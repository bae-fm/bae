//! The folder's audio, laid down as the units a release's tracks play.
//!
//! An **audio unit** is what one track's samples come from: a whole file, or
//! one slice a bound track sheet carves out of a container. Units are
//! **additive**: a bound sheet carves one unit per track it describes, a loose
//! audio file makes one, and they land in one ordered list. A folder holding a
//! disc image plus loose bonus tracks offers both — neither set is dropped for
//! the other. A candidate's draft holds one track per unit, in this order.

use crate::db::DbTrack;
use crate::import::folder_scanner::{BoundTrackSheet, CategorizedFiles, ScannedFile};
use crate::import::probe::{sheet_analysis, SourceDurations};
use crate::import::types::{AudioFile, CueFlacAnalysis, TrackAudio, TrackFile};
use crate::import::{ImportError, TrackUserEdit};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::debug;

/// How far a row's two lengths may differ before the row says so.
///
/// A source that rounds its track lengths to whole seconds is out by up to one;
/// lossy encoder delay and padding add a fraction more; a pregap counted on one
/// side and not the other is up to two on a Red Book disc. Three seconds
/// absorbs all of that.
///
/// What it deliberately does not absorb is a wrong pairing. Two different
/// tracks off one album differ by tens of seconds far more often than by three,
/// and the row shows both numbers regardless — this only decides whether to
/// point at them, with no consequence beyond a mark: nothing here disables the
/// commit.
pub(crate) const LENGTH_DISAGREEMENT_MS: u64 = 3_000;

/// Whether a row's two lengths are far enough apart to be worth pointing at.
/// `false` when either side has no number: there is nothing to compare, which
/// is not the same as agreeing.
pub(crate) fn lengths_disagree(file_ms: Option<u64>, release_ms: Option<u64>) -> bool {
    let (Some(file), Some(release)) = (file_ms, release_ms) else {
        return false;
    };
    file.abs_diff(release) > LENGTH_DISAGREEMENT_MS
}

/// What one of the folder's audio files contributes to the unit list.
#[derive(Debug, Clone)]
pub(crate) enum UnitContribution<'a> {
    /// The file backs one unit of its own.
    Whole,
    /// Track-sheet runs occupy this file's place in the order. Which run sits
    /// here is the disc assignment's to say, so a run's own container is not
    /// necessarily this file.
    Runs(Vec<BoundTrackSheet<'a>>),
    /// A carving sheet speaks for this file, so it adds nothing of its own.
    SpokenFor,
}

/// Every one of the folder's audio files, in the order it sits on disk, with
/// what each contributes to [`audio_units`].
///
/// A carving track sheet contributes one run — one entry per track it describes
/// — and the audio it speaks for contributes nothing of its own. Every other
/// audio file contributes one whole unit. That is what makes the mapping
/// additive rather than one shape winning.
///
/// Runs sit at the disk positions of the sheet-carved containers, but **which
/// run lands in which of those positions is the assignment's to say**: the runs
/// are ordered by `(disc number, sheet relative_path)` and laid into those
/// positions in that order, so disc one's tracks precede disc two's however the
/// rip spelled its filenames. Loose audio is untouched.
pub(crate) fn audio_layout(files: &CategorizedFiles) -> Vec<(&ScannedFile, UnitContribution<'_>)> {
    let carving = files.carving_sheets();

    // How many runs each container's position hosts, and which audio the
    // carving sheets speak for.
    let mut hosted: HashMap<&str, usize> = HashMap::new();
    let mut spoken_for: HashSet<&str> = HashSet::new();
    for sheet in &carving {
        for file_id in sheet_audio_ids(sheet) {
            spoken_for.insert(file_id);
        }
        *hosted
            .entry(
                sheet
                    .audio_files
                    .first()
                    .expect("a bound sheet resolves at least one audio file")
                    .1
                    .relative_path
                    .as_str(),
            )
            .or_default() += 1;
    }

    let mut ordered = carving;
    ordered.sort_by(|left, right| {
        left.disc_number().cmp(&right.disc_number()).then_with(|| {
            natord::compare_ignore_case(&left.file.relative_path, &right.file.relative_path)
        })
    });
    let mut runs = ordered.into_iter();

    files
        .audio()
        .map(|file| {
            let contribution = match hosted.get(file.relative_path.as_str()) {
                Some(count) => UnitContribution::Runs(runs.by_ref().take(*count).collect()),
                None if spoken_for.contains(file.relative_path.as_str()) => {
                    UnitContribution::SpokenFor
                }
                None => UnitContribution::Whole,
            };
            (file, contribution)
        })
        .collect()
}

/// The audio the folder offers, one entry per track it can produce, in the
/// order [`audio_layout`] lays it down.
fn units_of(layout: &[(&ScannedFile, UnitContribution<'_>)]) -> Vec<AudioFile> {
    let mut units = Vec::new();
    for (file, contribution) in layout {
        match contribution {
            UnitContribution::Whole => units.push(AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            }),
            UnitContribution::Runs(sheets) => {
                for sheet in sheets {
                    for (index, track) in sheet.sheet.playable_tracks().enumerate() {
                        units.push(AudioFile::SheetSlice {
                            file_id: sheet.audio_for(track).relative_path.clone(),
                            sheet_id: sheet.file.relative_path.clone(),
                            index: index as u32,
                        });
                    }
                }
            }
            UnitContribution::SpokenFor => {}
        }
    }
    units
}

/// The audio the folder offers, one entry per track it can produce.
///
/// The order is the scan's own, which is what "disk order" means everywhere
/// else in the import — the same order the file-metadata path reads embedded tags in,
/// so the two cannot pair a file's tags with another file's samples.
pub(crate) fn audio_units(files: &CategorizedFiles) -> Vec<AudioFile> {
    units_of(&audio_layout(files))
}

/// The candidate's effective audio rows in mapping order, reduced to the
/// duration evidence a metadata track layout compares against.
pub(crate) fn audio_durations(
    files: &CategorizedFiles,
    durations: &SourceDurations,
) -> Result<Vec<u64>, ImportError> {
    audio_units(files)
        .iter()
        .map(|audio| {
            durations
                .duration_of(audio)
                .ok_or_else(|| ImportError::UnusableFile {
                    detail: format!("{} has no measured duration", audio.file_id()),
                })
        })
        .collect()
}

/// Blank editable tracks over the candidate's physical audio layout. Direct
/// entry names nothing from files or sheets, but sheet slicing and
/// disc assignment remain physical facts about where the samples live.
pub(crate) fn direct_entry_track_rows(files: &CategorizedFiles) -> Vec<TrackUserEdit> {
    let sheet_discs: HashMap<&str, i32> = files
        .carving_sheets()
        .into_iter()
        .map(|sheet| {
            let disc = sheet
                .disc_number()
                .expect("a carving sheet has a disc assignment");
            (
                sheet.file.relative_path.as_str(),
                i32::try_from(disc).expect("sheet disc fits the database column"),
            )
        })
        .collect();

    // A release read from several folders gives each its own run of discs,
    // its loose audio first; one read from one folder gives loose audio no
    // disc at all.
    let layout = crate::import::folder_scanner::DiscLayout::of(
        &files.files,
        &files.parts,
        |entry| {
            matches!(
                &entry.role,
                crate::import::folder_scanner::FileRole::TrackSheet {
                    binding,
                    disc: crate::import::folder_scanner::SheetDisc::Disc { .. },
                    ..
                } if binding.is_resolved()
            )
        },
    );
    let mut numbers = std::collections::HashMap::<Option<i32>, i32>::new();
    audio_units(files)
        .into_iter()
        .enumerate()
        .map(|(index, audio)| {
            let side = match &audio {
                AudioFile::Standalone { file_id } => {
                    crate::import::folder_scanner::part_of(&files.parts, file_id)
                        .and_then(|part| layout.loose_disc(part))
                        .map(|disc| i32::try_from(disc).expect("disc number fits i32"))
                }
                AudioFile::SheetSlice { sheet_id, .. } => Some(
                    *sheet_discs
                        .get(sheet_id.as_str())
                        .expect("a sheet slice belongs to a carving sheet"),
                ),
            };
            // Tracks number from one on each disc of a release read from
            // several folders, and straight through one read from one.
            let track_number = if files.parts.is_empty() {
                i32::try_from(index + 1).expect("track position fits i32")
            } else {
                let number = numbers.entry(side).or_default();
                *number += 1;
                *number
            };
            TrackUserEdit {
                title: String::new(),
                side,
                track_number: Some(track_number),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: Some(audio),
            }
        })
        .collect()
}

/// Which of the folder's audio one bound sheet speaks for: the audio its
/// binding names, which is not necessarily what its `FILE` directives spell —
/// a directive may resolve by stem, and a single-file sheet may be bound by
/// the user.
fn sheet_audio_ids<'a>(bound: &BoundTrackSheet<'a>) -> Vec<&'a str> {
    bound
        .audio_files
        .iter()
        .map(|(_, audio)| audio.relative_path.as_str())
        .collect()
}

/// Bind each track to the audio holding its samples and yield the
/// [`TrackFile`]s the run pass consumes.
///
/// `rows` is the mapping the commit settled: one `(track, audio)` pair per row
/// that will be written, in track order. Every `DbTrack` moves into a
/// `TrackFile` with its `duration_ms` filled in — from the sheet's own
/// timing for a slice, from a probe for a standalone file — and every slice of
/// one sheet shares that sheet's single parsed analysis.
///
/// A row whose title is blank is titled after its audio file's name. An empty
/// title is a track nobody can find again: it renders as a blank row in every
/// list, sorts nowhere, and matches no search. The file's own name is the one
/// fact about that track that is certainly true, and it is what the mapping
/// table showed on that very row — so it is what gets written. Reading the
/// file's embedded tag instead would let a second metadata authority into an
/// import whose authority the user already chose, and would write something
/// the table never displayed.
pub(crate) fn resolve_track_files(
    rows: Vec<(DbTrack, AudioFile)>,
    files: &CategorizedFiles,
) -> Result<Vec<TrackFile>, ImportError> {
    debug!("Binding {} tracks to the folder's audio", rows.len());
    let mut analyses: HashMap<String, Arc<CueFlacAnalysis>> = HashMap::new();
    let mut track_files = Vec::with_capacity(rows.len());

    for (mut db_track, audio) in rows {
        let file = audio_file(files, &audio)?;
        if db_track.title.trim().is_empty() {
            db_track.title = file_title(file);
        }
        let (audio, duration_ms) = match &audio {
            AudioFile::Standalone { .. } => {
                let source_audio =
                    file.source_audio
                        .clone()
                        .ok_or_else(|| ImportError::UnusableFile {
                            detail: format!("{} has no scanned audio facts", file.relative_path),
                        })?;
                let duration_ms = source_audio.duration_ms;
                (
                    TrackAudio::Standalone {
                        file_path: file.path.clone(),
                        source_audio,
                    },
                    duration_ms,
                )
            }
            AudioFile::SheetSlice {
                sheet_id, index, ..
            } => {
                let analysis = match analyses.get(sheet_id) {
                    Some(analysis) => Arc::clone(analysis),
                    None => {
                        let analysis = Arc::new(sheet_analysis(files, sheet_id)?);
                        analyses.insert(sheet_id.clone(), Arc::clone(&analysis));
                        analysis
                    }
                };
                let cue_index = *index as usize;
                let duration_ms =
                    crate::import::probe::sheet_track_duration_ms(&analysis, cue_index, sheet_id)?;
                (
                    TrackAudio::CueBacked {
                        cue_pair: analysis,
                        cue_index,
                    },
                    duration_ms,
                )
            }
        };
        db_track.duration_ms =
            Some(
                i64::try_from(duration_ms).map_err(|_| ImportError::UnusableFile {
                    detail: format!(
                        "{} is too long to represent in milliseconds",
                        file.relative_path
                    ),
                })?,
            );
        track_files.push(TrackFile { db_track, audio });
    }
    Ok(track_files)
}

/// The persisted scanned audio a binding names. Audio that is no longer among
/// the candidate's files is a refusal: the mapping named samples this import
/// cannot read, and the folder changed under the choice. Whether each file on
/// disk is still the one scanned is the import's own check, made once for
/// every release file before any is read
/// ([`crate::import::file_identity::validate_scanned_file_identities`]).
fn audio_file<'a>(
    files: &'a CategorizedFiles,
    audio: &AudioFile,
) -> Result<&'a ScannedFile, ImportError> {
    files
        .audio()
        .find(|file| file.relative_path == audio.file_id())
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{} is no longer in the folder", audio.file_id()),
        })
}

/// A file's name without its extension — the title an unnamed row writes.
fn file_title(file: &ScannedFile) -> String {
    std::path::Path::new(&file.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&file.file_name)
        .to_string()
}

#[cfg(test)]
#[path = "audio_layout_tests.rs"]
mod tests;
