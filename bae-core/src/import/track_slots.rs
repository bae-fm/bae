//! The file↔release mapping, as a value.
//!
//! A **track slot** is one row of the release's track table: the audio on disk
//! and the track the source names for it, laid alongside each other. The
//! mapping is computed when a release is picked, so it is something to look at
//! and correct before anything is written, and the commit consumes what the
//! user saw rather than re-deriving it.
//!
//! Slots are **additive**: a bound track sheet carves one slot per track it
//! describes out of one container, a standalone audio file makes one, and they
//! land in the same ordered list. A folder holding a disc image plus loose
//! bonus tracks maps both — neither set is dropped for the other.
//!
//! A disagreement between the two sides is a slot, never an error.
//! [`TrackSlot::FileOnly`] is audio the source's tracklist does not account
//! for; [`TrackSlot::TrackOnly`] is a track no audio backs. The only refusal
//! left here is audio that will not decode.

use crate::db::DbTrack;
use crate::import::folder_scanner::{BoundTrackSheet, CategorizedFiles, ScannedFile};
use crate::import::probe::{sheet_analysis, SourceDurations};
use crate::import::types::{AudioFile, CueFlacAnalysis, TrackFile};
use crate::import::{ImportError, TrackUserEdit};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, warn};

/// Where one row sits in the run of rows a single container is carved into.
///
/// The mapping pane renders this as the link glyph's shape, which is the only
/// place a reader can see that eleven rows come out of one file rather than
/// eleven. Every row a whole file of its own backs is [`Whole`](Self::Whole).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSpan {
    /// One file, one row.
    Whole,
    /// The first of several rows carved out of one container.
    ContainerStart,
    /// Neither the first nor the last of them.
    ContainerMiddle,
    /// The last of them.
    ContainerEnd,
}

/// The audio behind one slot row, as the row displays it.
///
/// Everything here is a fact about the file rather than about the release, so
/// it is the same whichever release is picked — which is why picking a
/// different pressing replaces only the source's half of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotFile {
    /// Which of the folder's audio this row's samples come from. Equal to the
    /// row's [`TrackUserEdit::file`], and it is the value a "choose file" pick
    /// writes there.
    pub audio: AudioFile,
    /// The file's own name, without its directory prefix.
    pub name: String,
    /// The whole container's size in bytes, even where the row is one slice of
    /// it: a slice has no size of its own on disk.
    pub size: u64,
    /// Absolute path — what auditioning this row plays.
    pub path: std::path::PathBuf,
    /// This row's own playing time from the scan facts or sheet timing. `None`
    /// when a sheet gives the row no timing; inventing one would make a wrong
    /// pairing look right.
    pub duration_ms: Option<u64>,
    pub span: SlotSpan,
}

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
/// point at them. The same reasoning as `identify::ready`'s tolerance, one
/// track wide instead of a whole release, and with no consequence beyond a
/// mark: nothing here disables the commit.
pub const LENGTH_DISAGREEMENT_MS: u64 = 3_000;

/// Whether a row's two lengths are far enough apart to be worth pointing at.
///
/// Asked per row as it renders rather than settled when the mapping is
/// computed: re-pointing a row at a different file gives it two new lengths,
/// and an answer stored at selection would still be describing the pairing it
/// replaced.
pub fn lengths_disagree(file_ms: Option<u64>, release_ms: Option<u64>) -> bool {
    let (Some(file), Some(release)) = (file_ms, release_ms) else {
        return false;
    };
    file.abs_diff(release) > LENGTH_DISAGREEMENT_MS
}

/// One track as the source names it: the editable row projected from it, plus
/// the two facts only the source knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTrack {
    pub edit: TrackUserEdit,
    /// The source's own position string — `A1`, `1`, `1-2`, or arbitrary prose.
    pub position: Option<String>,
    /// How long the source says this track runs.
    pub duration_ms: Option<u64>,
}

/// One row of the file↔release mapping.
///
/// Every variant carries the editable track row, whose
/// [`file`](TrackUserEdit::file) names the audio bound to it. That binding is
/// the part that survives: it rides the edit to the commit, so a pairing the
/// user corrected is the one that gets written.
///
/// The rest is what the row shows. A paired row carries **both** durations —
/// the file's own and the source's — because that pair is the only thing that
/// catches a pairing which is complete but wrong, and counting cannot see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackSlot {
    /// The source names this track and audio on disk backs it.
    Paired {
        track: TrackUserEdit,
        position: Option<String>,
        source_duration_ms: Option<u64>,
        file: SlotFile,
    },
    /// Audio on disk the source's tracklist does not account for. Its title is
    /// blank until someone names it, and it has no position and no length in
    /// the source because the source says nothing about it.
    FileOnly {
        track: TrackUserEdit,
        file: SlotFile,
    },
    /// A track the source names with no audio bound to it.
    TrackOnly {
        track: TrackUserEdit,
        position: Option<String>,
        source_duration_ms: Option<u64>,
    },
}

impl TrackSlot {
    pub fn track(&self) -> &TrackUserEdit {
        match self {
            Self::Paired { track, .. }
            | Self::FileOnly { track, .. }
            | Self::TrackOnly { track, .. } => track,
        }
    }

    pub fn into_track(self) -> TrackUserEdit {
        match self {
            Self::Paired { track, .. }
            | Self::FileOnly { track, .. }
            | Self::TrackOnly { track, .. } => track,
        }
    }

    /// The audio this slot is bound to — `None` exactly for
    /// [`TrackOnly`](Self::TrackOnly).
    pub fn file(&self) -> Option<&AudioFile> {
        self.track().file.as_ref()
    }
}

/// The tally above the slot table: how many files the folder offers against how
/// many tracks the source names, and which way they disagree.
///
/// Computed rather than left to each UI to subtract, and stated rather than
/// enforced — a disagreement is something to read, never something that
/// disables the commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotReconciliation {
    /// Both sides account for every row.
    Agrees { count: u32 },
    /// Audio the source's tracklist does not reach.
    MoreFiles { files: u32, tracks: u32 },
    /// Tracks the source names with nothing on disk behind them.
    MoreTracks { files: u32, tracks: u32 },
}

/// The whole slot table for one folder and one picked release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotTable {
    pub rows: Vec<TrackSlot>,
    pub reconciliation: SlotReconciliation,
    /// Every audio unit the folder offers, in disk order — what a row with no
    /// file is offered to choose from, and what re-pairing two rows swaps
    /// between them.
    pub audio: Vec<SlotFile>,
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
        for file_id in sheet_audio_ids(files, sheet) {
            spoken_for.insert(file_id);
        }
        *hosted
            .entry(sheet.audio.relative_path.as_str())
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
pub(crate) fn units_of(layout: &[(&ScannedFile, UnitContribution<'_>)]) -> Vec<AudioFile> {
    let mut units = Vec::new();
    for (file, contribution) in layout {
        match contribution {
            UnitContribution::Whole => units.push(AudioFile::Standalone {
                file_id: file.relative_path.clone(),
            }),
            UnitContribution::Runs(sheets) => {
                for sheet in sheets {
                    for index in 0..sheet.sheet.playable_track_count() {
                        units.push(AudioFile::SheetSlice {
                            file_id: sheet.audio.relative_path.clone(),
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
/// else in the import — the same order the File Tags path reads embedded tags in,
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
    let mut next_number: HashMap<i32, i32> = HashMap::new();

    audio_units(files)
        .into_iter()
        .map(|audio| {
            let side = match &audio {
                AudioFile::Standalone { .. } => 1,
                AudioFile::SheetSlice { sheet_id, .. } => *sheet_discs
                    .get(sheet_id.as_str())
                    .expect("a sheet slice belongs to a carving sheet"),
            };
            let number = next_number.entry(side).or_insert(0);
            *number += 1;
            TrackUserEdit {
                title: String::new(),
                side,
                track_number: Some(*number),
                artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                file: Some(audio),
            }
        })
        .collect()
}

/// Which of the folder's audio one bound sheet speaks for.
///
/// A sheet that names one file for the whole disc speaks for the audio it is
/// *bound* to, which is not necessarily what its `FILE` directive spells — the
/// two differ exactly when the binding is the user's. A sheet that names one
/// file per track has no single binding to stand in for its references, so
/// those resolve as written, inside the sheet's own directory.
fn sheet_audio_ids<'a>(files: &'a CategorizedFiles, bound: &BoundTrackSheet<'a>) -> Vec<&'a str> {
    files
        .sheet_audio_files(bound)
        .into_iter()
        .map(|audio| audio.relative_path.as_str())
        .collect()
}

/// The file↔release mapping's rows, as editable tracks.
///
/// The folder's audio (in disk order) and `source_tracks` (in the source's own
/// order) are laid alongside each other: row `i` binds source track `i` to
/// audio unit `i`, and whichever side runs out first leaves its leftovers at
/// the tail — audio with no track carries a blank title and a file, tracks with
/// no audio carry a title and no file.
///
/// Two consequences the callers rely on. Row `i` stands for source track `i`
/// for every `i` the source names, so the row the commit writes and the track
/// the source described stay together without either side carrying an index.
/// And the leftovers sit next to the rows they follow on disk rather than in a
/// footer of their own.
///
/// This is the whole mapping, and it costs nothing: no file is opened. The
/// display facts a slot row shows — sizes, spans, probed lengths — are hung on
/// these rows by [`slot_table`], which is the one that pays for them.
pub(crate) fn map_source_rows(
    source_tracks: &[TrackUserEdit],
    units: &[AudioFile],
) -> Vec<TrackUserEdit> {
    let mut rows: Vec<TrackUserEdit> = Vec::with_capacity(units.len().max(source_tracks.len()));

    for (index, unit) in units.iter().enumerate() {
        match source_tracks.get(index) {
            Some(track) => {
                let mut track = track.clone();
                track.file = Some(unit.clone());
                rows.push(track);
            }
            None => {
                // The source says nothing about this audio, so the row starts
                // blank and continues the numbering of the row above it.
                let (side, track_number) = match rows.last() {
                    Some(previous) => (previous.side, previous.track_number.map(|n| n + 1)),
                    None => (1, Some(1)),
                };
                rows.push(TrackUserEdit {
                    title: String::new(),
                    side,
                    track_number,
                    artist_assignments: crate::import::TrackArtistAssignments::AlbumArtists,
                    file: Some(unit.clone()),
                });
            }
        }
    }

    for track in source_tracks.iter().skip(units.len()) {
        let mut track = track.clone();
        track.file = None;
        rows.push(track);
    }

    rows
}

/// The file↔release mapping the pane renders: every row with the two sides it
/// pairs, the tally above them, and the audio a row with no file can be
/// pointed at.
///
/// Opens nothing. The playing times come from `durations`, which
/// identification measured and stored; a unit with no row there shows no
/// length until something reads it.
pub(crate) fn slot_table(
    source_tracks: &[SourceTrack],
    files: &CategorizedFiles,
    durations: &SourceDurations,
) -> SlotTable {
    let audio = slot_files(files, durations);
    let units: Vec<AudioFile> = audio.iter().map(|file| file.audio.clone()).collect();
    let source_edits: Vec<TrackUserEdit> = source_tracks
        .iter()
        .map(|track| track.edit.clone())
        .collect();

    let rows = map_source_rows(&source_edits, &units)
        .into_iter()
        .enumerate()
        .map(|(index, track)| {
            match (source_tracks.get(index), audio.get(index)) {
                (Some(source), Some(file)) => TrackSlot::Paired {
                    track,
                    position: source.position.clone(),
                    source_duration_ms: source.duration_ms,
                    file: file.clone(),
                },
                (None, Some(file)) => TrackSlot::FileOnly {
                    track,
                    file: file.clone(),
                },
                (Some(source), None) => TrackSlot::TrackOnly {
                    track,
                    position: source.position.clone(),
                    source_duration_ms: source.duration_ms,
                },
                // `map_source_rows` yields exactly `max(len, len)` rows, so an
                // index past both sides cannot be produced.
                (None, None) => unreachable!("a row belongs to at least one side"),
            }
        })
        .collect();

    let files_count = audio.len() as u32;
    let tracks_count = source_tracks.len() as u32;
    let reconciliation = match files_count.cmp(&tracks_count) {
        std::cmp::Ordering::Equal => SlotReconciliation::Agrees { count: files_count },
        std::cmp::Ordering::Greater => SlotReconciliation::MoreFiles {
            files: files_count,
            tracks: tracks_count,
        },
        std::cmp::Ordering::Less => SlotReconciliation::MoreTracks {
            files: files_count,
            tracks: tracks_count,
        },
    };

    SlotTable {
        rows,
        reconciliation,
        audio,
    }
}

/// Every audio unit the folder offers, with the facts a slot row shows about
/// it: name, size, where to play it from, its span in a container's run, and
/// its own playing time as `durations` records it.
///
/// Same order and same entries as [`audio_units`] — this is that list with the
/// stored measurements hung on it.
pub(crate) fn slot_files(files: &CategorizedFiles, durations: &SourceDurations) -> Vec<SlotFile> {
    let units = audio_units(files);
    let by_path: HashMap<&str, &ScannedFile> = files
        .audio()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();

    let mut out = Vec::with_capacity(units.len());
    for (index, unit) in units.iter().enumerate() {
        let Some(file) = by_path.get(unit.file_id()) else {
            // `audio_units` reads the same list, so a unit naming audio that is
            // not there cannot happen. Skipping keeps this total rather than
            // panicking on a state the type does not forbid.
            warn!("{} is not this folder's audio", unit.file_id());
            continue;
        };
        out.push(SlotFile {
            audio: unit.clone(),
            name: file.file_name.clone(),
            size: file.size,
            path: file.path.clone(),
            duration_ms: durations.duration_of(unit),
            span: span_at(&units, index),
        });
    }
    out
}

/// Where the unit at `index` sits in the run of units one container is carved
/// into. A run is the contiguous stretch of slices naming the same sheet —
/// contiguous by construction, since [`audio_units`] emits a sheet's slices
/// together at its container's position.
fn span_at(units: &[AudioFile], index: usize) -> SlotSpan {
    let AudioFile::SheetSlice { sheet_id, .. } = &units[index] else {
        return SlotSpan::Whole;
    };
    let same = |unit: Option<&AudioFile>| matches!(unit, Some(AudioFile::SheetSlice { sheet_id: other, .. }) if other == sheet_id);
    let leads = !same(index.checked_sub(1).map(|before| &units[before]));
    let trails = !same(units.get(index + 1));
    match (leads, trails) {
        (true, true) => SlotSpan::Whole,
        (true, false) => SlotSpan::ContainerStart,
        (false, false) => SlotSpan::ContainerMiddle,
        (false, true) => SlotSpan::ContainerEnd,
    }
}

/// Bind each track to the audio holding its samples and yield the
/// [`TrackFile`]s the run pass consumes.
///
/// `rows` is the mapping the commit settled: one `(track, audio)` pair per row
/// that will be written, in track order. Every `DbTrack` moves into a
/// `TrackFile` variant with its `duration_ms` filled in — from the sheet's own
/// timing for a slice, from a probe for a standalone file — and every slice of
/// one sheet shares that sheet's single parsed analysis.
///
/// A row whose title is blank is titled after its audio file's name. An empty
/// title is a track nobody can find again: it renders as a blank row in every
/// list, sorts nowhere, and matches no search. The file's own name is the one
/// fact about that track that is certainly true, and it is what the slot table
/// showed on that very row — so it is what gets written. Reading the file's
/// embedded tag instead would let a second metadata authority into an import
/// whose authority the user already chose, and would write something the slot
/// table never displayed.
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
        match &audio {
            AudioFile::Standalone { .. } => {
                let source_audio =
                    file.source_audio
                        .clone()
                        .ok_or_else(|| ImportError::UnusableFile {
                            detail: format!("{} has no scanned audio facts", file.relative_path),
                        })?;
                let duration_ms = source_audio.duration_ms;
                db_track.duration_ms =
                    Some(
                        i64::try_from(duration_ms).map_err(|_| ImportError::UnusableFile {
                            detail: format!(
                                "{} is too long to represent in milliseconds",
                                file.relative_path
                            ),
                        })?,
                    );
                track_files.push(TrackFile::Standalone {
                    db_track,
                    file_path: file.path.clone(),
                    source_audio,
                });
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
                db_track.duration_ms =
                    Some(
                        i64::try_from(duration_ms).map_err(|_| ImportError::UnusableFile {
                            detail: format!(
                                "{sheet_id} track {} is too long to represent in milliseconds",
                                cue_index + 1
                            ),
                        })?,
                    );
                track_files.push(TrackFile::CueBacked {
                    db_track,
                    file_path: file.path.clone(),
                    cue_pair: analysis,
                    cue_index,
                });
            }
        }
    }
    Ok(track_files)
}

/// The persisted scanned audio a binding names. Audio that is no longer there
/// is a refusal: the mapping named samples this import cannot read, and the
/// folder changed under the choice.
fn audio_file<'a>(
    files: &'a CategorizedFiles,
    audio: &AudioFile,
) -> Result<&'a ScannedFile, ImportError> {
    let file = files
        .audio()
        .find(|file| file.relative_path == audio.file_id())
        .ok_or_else(|| ImportError::UnusableFile {
            detail: format!("{} is no longer in the folder", audio.file_id()),
        })?;
    let metadata = std::fs::metadata(&file.path).map_err(|error| ImportError::UnusableFile {
        detail: format!("cannot read {}: {error}", file.path.display()),
    })?;
    let modified_at_ns = super::folder_scanner::file_modified_at_ns(&file.path, &metadata)
        .map_err(|error| ImportError::UnusableFile {
            detail: error.to_string(),
        })?;
    if metadata.len() != file.size || modified_at_ns != file.modified_at_ns {
        return Err(ImportError::UnusableFile {
            detail: format!("{} changed after it was scanned", file.path.display()),
        });
    }
    Ok(file)
}

/// A file's name without its extension — the title an unnamed slot writes.
fn file_title(file: &ScannedFile) -> String {
    std::path::Path::new(&file.file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&file.file_name)
        .to_string()
}

#[cfg(test)]
#[path = "track_slots_tests.rs"]
mod tests;
