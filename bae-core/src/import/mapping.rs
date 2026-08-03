//! The mapping table: every source unit the folder offers, alongside the track
//! committing makes of it.
//!
//! One structure, not two lists to keep aligned. The editable track row lives
//! *inside* the row that produces it, so removing a row removes both halves and
//! no index addresses anything — which is what keeps the joining out of the
//! surfaces that render it.

use crate::cue_flac::CueSheet;
use crate::import::folder_scanner::{
    BoundTrackSheet, CandidateFile, CategorizedFiles, CollapsedDirectory, FileRole, FileRoleChoice,
    ScannedFile, SheetBinding, SheetDisc,
};
use crate::import::track_slots::{
    audio_layout, units_of, SlotFile, SlotReconciliation, SlotTable, TrackSlot, UnitContribution,
};
use crate::import::types::{AudioFile, RawTrackEdit};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use tracing::warn;

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it.
#[derive(Debug, Clone)]
pub struct MappingTable {
    pub rows: Vec<MappingRow>,
    /// The tally over the rows that become tracks. `None` when there is nothing
    /// to reconcile the folder against — no release is picked, or the tracklist
    /// was read off the folder's own files and so cannot disagree with it.
    pub reconciliation: Option<SlotReconciliation>,
}

impl MappingTable {
    /// No folder behind the pick, so no mapping: re-identify chooses a release
    /// for a release already in the library, whose files are bound already.
    pub fn empty() -> Self {
        Self {
            rows: Vec::new(),
            reconciliation: None,
        }
    }
}

/// One row of the mapping table.
#[derive(Debug, Clone)]
pub enum MappingRow {
    /// One source unit and what it becomes.
    Unit(MappingUnit),
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: SheetGroup,
        entries: Vec<MappingUnit>,
    },
    /// A directory whose files all do the same job, shown as one row.
    Directory(CollapsedDirectory),
}

impl MappingRow {
    /// The units this row carries: itself, or the entries a sheet carves. A
    /// collapsed directory carries none — it is one row standing in for files
    /// that are not the release's tracks.
    pub fn units(&self) -> &[MappingUnit] {
        match self {
            Self::Unit(unit) => std::slice::from_ref(unit),
            Self::Sheet { entries, .. } => entries.as_slice(),
            Self::Directory(_) => &[],
        }
    }

    fn units_mut(&mut self) -> &mut [MappingUnit] {
        match self {
            Self::Unit(unit) => std::slice::from_mut(unit),
            Self::Sheet { entries, .. } => entries.as_mut_slice(),
            Self::Directory(_) => &mut [],
        }
    }
}

/// One source unit, and the track committing makes of it.
#[derive(Debug, Clone)]
pub struct MappingUnit {
    pub source: MappingSource,
    pub becomes: MappingBecomes,
}

/// The left half of a row: what the folder offers for it.
#[derive(Debug, Clone)]
pub enum MappingSource {
    /// A file the folder holds, whole.
    File(MappingFile),
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry(MappingEntry),
    /// The source names a track this folder has nothing for: the left half is
    /// empty, and the row is offered the folder's audio to point it at.
    Missing,
}

/// What one of the folder's files is, as a row of the mapping table.
///
/// Narrower than the role the scan proposes: a track sheet is not a row here —
/// it heads a group of rows — so it has no variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingRole {
    /// Playable audio.
    Audio,
    /// The image that leads the release.
    Cover,
    /// Any other image.
    Artwork,
    /// Readable evidence: a rip log, a tracklist, a playlist, or a CUE that
    /// could not be parsed as a sheet.
    Document,
    /// In the folder and carried with the release, unrecognized.
    Other,
}

/// A file of the folder, as the mapping table's left half shows it.
#[derive(Debug, Clone)]
pub struct MappingFile {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub path: PathBuf,
    /// Probed playing time, where the folder's audio has been read. `None` for
    /// anything that is not audio, for audio nothing could be read from, and
    /// while no release is picked — nothing has opened the folder yet.
    pub probed_duration_ms: Option<u64>,
    pub role: MappingRole,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make.
    pub alternatives: Vec<FileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when [`Self::alternatives`] is empty.
    pub role_choice: Option<FileRoleChoice>,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
#[derive(Debug, Clone)]
pub struct MappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    pub title: Option<String>,
    /// How long the sheet says this entry runs.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    pub container_path: PathBuf,
}

/// The right half of a row: what committing makes of the source unit.
#[derive(Debug, Clone)]
pub enum MappingBecomes {
    /// A track of the release being committed. The row edits it in place.
    Track {
        track: RawTrackEdit,
        /// What the picked release names for this track, where it names one.
        source_position: Option<String>,
        source_duration_ms: Option<u64>,
    },
    /// The image that leads the release.
    Cover,
    /// Written with the release like every other folder file, just not one
    /// of its tracks.
    Kept,
    /// No release is picked yet, so what this becomes is the open question.
    AwaitingPick,
}

/// A track sheet, as the header of the group of rows it carves.
#[derive(Debug, Clone)]
pub struct SheetGroup {
    pub sheet_id: String,
    pub name: String,
    /// Absolute path — what opening the sheet to read it reaches.
    pub path: PathBuf,
    pub bound: SheetBound,
    pub assignment: SheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// What a track sheet describes, with the facts its header shows about it.
///
/// [`SheetBinding`] enriched by the
/// container's name and size: a header states both which audio a sheet is on and
/// why it is on none, and carrying the binding separately would be a second way
/// to say the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SheetBound {
    /// The sheet describes this audio.
    Describes(MappingContainer),
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The audio imports as one track.
    RefusedCodec {
        container: MappingContainer,
        codec: String,
    },
}

impl SheetBound {
    /// The audio the sheet is on, where it is on any — the file whose rows this
    /// sheet's group stands for.
    pub fn container_id(&self) -> Option<&str> {
        match self {
            Self::Describes(container) | Self::RefusedCodec { container, .. } => {
                Some(container.file_id.as_str())
            }
            Self::Unresolved { .. } => None,
        }
    }
}

/// The audio a track sheet describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
}

/// Where the tracklist a folder is being committed as came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracklistSource {
    /// A release picked from a metadata source. Its tracklist and the folder's
    /// audio are two independent accounts of one disc, so the table tallies
    /// them against each other.
    Release,
    /// The folder's own files — their embedded tags, or the track sheets they
    /// come with. It cannot disagree with the folder, because it *is* the
    /// folder, so the table carries no tally.
    FileTags,
}

/// The tracklist a folder is being committed as, and the row identities the
/// editor addresses it by.
#[derive(Debug, Clone, Copy)]
pub struct PickedTracklist<'a> {
    /// The file↔tracklist pairing, whose rows correspond one-for-one and in
    /// order with the folder's audio units.
    pub slots: &'a SlotTable,
    /// Track row `n` of the table is addressed as `{track_id_prefix}-{n}`.
    pub track_id_prefix: &'a str,
    pub source: TracklistSource,
}

/// Project the mapping table for one folder, against the tracklist picked for
/// it.
///
/// `picked` is `None` in the identify phase: every audio row then reads
/// [`MappingBecomes::AwaitingPick`], and a cover or a document still says what
/// it becomes, because a role is a fact about the folder and needs no release.
pub fn mapping_table(
    files: &CategorizedFiles,
    picked: Option<PickedTracklist<'_>>,
) -> MappingTable {
    let layout = audio_layout(files);
    let units = units_of(&layout);
    // Slot row `i` is audio unit `i`: the two are the same list, so position is
    // the whole correspondence between them.
    let slot_of: HashMap<AudioFile, usize> = units
        .iter()
        .enumerate()
        .map(|(index, unit)| (unit.clone(), index))
        .collect();
    let contributions: HashMap<&str, &UnitContribution<'_>> = layout
        .iter()
        .map(|(file, contribution)| (file.relative_path.as_str(), contribution))
        .collect();
    let carving: BTreeSet<&str> = layout
        .iter()
        .filter_map(|(_, contribution)| match contribution {
            UnitContribution::Runs(sheets) => Some(sheets),
            UnitContribution::Whole | UnitContribution::SpokenFor => None,
        })
        .flatten()
        .map(|sheet| sheet.file.relative_path.as_str())
        .collect();

    let collapsed = files.collapsed_directories();
    let disc_options = disc_options(files, picked.as_ref());

    let mut builder = RowBuilder {
        picked,
        slot_of,
        next_track: 0,
    };
    let mut rows = Vec::with_capacity(files.files.len());
    let mut opened: BTreeSet<&str> = BTreeSet::new();

    for entry in &files.files {
        if let Some(directory) = collapsed_row(&collapsed, entry) {
            if opened.insert(directory.dir_prefix.as_str()) {
                rows.push(MappingRow::Directory(directory.clone()));
            }
            continue;
        }
        match &entry.role {
            // A carving sheet is named at the position its run occupies, which
            // the assignment decides and the sheet's own place on disk does not.
            FileRole::TrackSheet { .. } if carving.contains(entry.file.relative_path.as_str()) => {}
            FileRole::TrackSheet {
                sheet,
                binding,
                disc,
            } => rows.push(MappingRow::Sheet {
                sheet: SheetGroup {
                    sheet_id: entry.file.relative_path.clone(),
                    name: entry.file.file_name.clone(),
                    path: entry.file.path.clone(),
                    bound: bound_of(files, sheet, binding),
                    assignment: *disc,
                    disc_options: disc_options.clone(),
                },
                entries: Vec::new(),
            }),
            FileRole::Audio => match contributions.get(entry.file.relative_path.as_str()) {
                Some(UnitContribution::Runs(sheets)) => {
                    for sheet in sheets.iter() {
                        rows.push(MappingRow::Sheet {
                            sheet: SheetGroup {
                                sheet_id: sheet.file.relative_path.clone(),
                                name: sheet.file.file_name.clone(),
                                path: sheet.file.path.clone(),
                                bound: SheetBound::Describes(container(sheet.audio)),
                                assignment: sheet.disc,
                                disc_options: disc_options.clone(),
                            },
                            entries: builder.sheet_entries(sheet),
                        });
                    }
                }
                Some(UnitContribution::Whole) => {
                    rows.push(MappingRow::Unit(builder.audio_row(entry)))
                }
                // A carving sheet speaks for this file, so the sheet's rows are
                // what it contributes and it has none of its own.
                Some(UnitContribution::SpokenFor) => {}
                None => warn!(
                    "{} carries the audio role but the folder's layout does not place it",
                    entry.file.relative_path
                ),
            },
            // Nothing else the folder holds is in the tracklist, and no release
            // has to be picked to know it — the role says so on its own. The
            // folder is the release, so all of it is still carried.
            FileRole::Cover => rows.push(carried(entry, MappingRole::Cover)),
            FileRole::Artwork => rows.push(carried(entry, MappingRole::Artwork)),
            FileRole::Document => rows.push(carried(entry, MappingRole::Document)),
            FileRole::Other => rows.push(carried(entry, MappingRole::Other)),
        }
    }

    // The tracks the source names and the folder has nothing for. They sit past
    // every unit in the slot table, so they close the table.
    if let Some(picked) = picked {
        for index in units.len()..picked.slots.rows.len() {
            rows.push(MappingRow::Unit(MappingUnit {
                source: MappingSource::Missing,
                becomes: builder.track_at(index),
            }));
        }
    }

    let reconciliation = picked
        .filter(|picked| picked.source == TracklistSource::Release)
        .map(|_| tally(&rows));
    MappingTable {
        rows,
        reconciliation,
    }
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes.
pub fn mapping_tracks(table: &MappingTable) -> Vec<RawTrackEdit> {
    table
        .rows
        .iter()
        .flat_map(MappingRow::units)
        .filter_map(|unit| match &unit.becomes {
            MappingBecomes::Track { track, .. } => Some(track.clone()),
            MappingBecomes::Cover | MappingBecomes::Kept | MappingBecomes::AwaitingPick => None,
        })
        .collect()
}

/// The running state of one projection: which tracklist it is pairing against,
/// and how many track rows it has emitted.
struct RowBuilder<'a> {
    picked: Option<PickedTracklist<'a>>,
    slot_of: HashMap<AudioFile, usize>,
    next_track: usize,
}

impl RowBuilder<'_> {
    /// One row for a loose audio file.
    fn audio_row(&mut self, entry: &CandidateFile) -> MappingUnit {
        let unit = AudioFile::Standalone {
            file_id: entry.file.relative_path.clone(),
        };
        let probed = self.probed_duration_ms(&unit);
        MappingUnit {
            becomes: self.becomes_for(&unit),
            source: MappingSource::File(mapping_file(entry, MappingRole::Audio, probed)),
        }
    }

    /// One row per entry a carving sheet describes.
    fn sheet_entries(&mut self, sheet: &BoundTrackSheet<'_>) -> Vec<MappingUnit> {
        sheet
            .sheet
            .playable_tracks()
            .enumerate()
            .map(|(index, track)| {
                let unit = AudioFile::SheetSlice {
                    file_id: sheet.audio.relative_path.clone(),
                    sheet_id: sheet.file.relative_path.clone(),
                    index: index as u32,
                };
                MappingUnit {
                    becomes: self.becomes_for(&unit),
                    source: MappingSource::SheetEntry(MappingEntry {
                        sheet_id: sheet.file.relative_path.clone(),
                        index: index as u32,
                        number: track.number,
                        title: track.title.clone(),
                        duration_ms: track.track_duration_ms(),
                        container_id: sheet.audio.relative_path.clone(),
                        container_name: sheet.audio.file_name.clone(),
                        container_path: sheet.audio.path.clone(),
                    }),
                }
            })
            .collect()
    }

    /// This unit's playing time as the pairing pass probed it, where a release
    /// has been picked and the folder therefore opened.
    fn probed_duration_ms(&self, unit: &AudioFile) -> Option<u64> {
        let picked = self.picked?;
        let index = *self.slot_of.get(unit)?;
        picked
            .slots
            .audio
            .get(index)
            .and_then(|file: &SlotFile| file.probed_duration_ms)
    }

    /// What the unit becomes: the track the picked tracklist puts on it, or the
    /// open question a folder with no pick leaves.
    fn becomes_for(&mut self, unit: &AudioFile) -> MappingBecomes {
        if self.picked.is_none() {
            return MappingBecomes::AwaitingPick;
        }
        let Some(&index) = self.slot_of.get(unit) else {
            // Every unit this asks about was read off the same layout the index
            // was built from, so a unit missing from it cannot be produced.
            warn!("{unit:?} is not one of this folder's audio units");
            return MappingBecomes::AwaitingPick;
        };
        self.track_at(index)
    }

    /// The track at slot row `index`, taking the next row identity.
    fn track_at(&mut self, index: usize) -> MappingBecomes {
        let Some(picked) = self.picked else {
            return MappingBecomes::AwaitingPick;
        };
        let Some(slot) = picked.slots.rows.get(index) else {
            // Slot row `i` is audio unit `i` and the table is never shorter
            // than the folder's units, so a caller that pairs a folder with
            // another folder's slots is the only way here.
            warn!(
                "the picked tracklist has no row {index}; it does not describe this folder's audio"
            );
            return MappingBecomes::AwaitingPick;
        };
        let id = format!("{}-{}", picked.track_id_prefix, self.next_track);
        self.next_track += 1;
        let (source_position, source_duration_ms) = match slot {
            TrackSlot::Paired {
                position,
                source_duration_ms,
                ..
            }
            | TrackSlot::TrackOnly {
                position,
                source_duration_ms,
                ..
            } => (Some(position.clone()), *source_duration_ms),
            TrackSlot::FileOnly { .. } => (None, None),
        };
        MappingBecomes::Track {
            track: RawTrackEdit::from_user_edit(slot.track().clone(), id),
            source_position,
            source_duration_ms,
        }
    }
}

/// One row for a file that is not one of the release's tracks: the cover, or
/// something the folder carries alongside them. Nothing has to be opened to
/// know what it becomes, so it shows no probed length.
fn carried(entry: &CandidateFile, role: MappingRole) -> MappingRow {
    MappingRow::Unit(MappingUnit {
        becomes: match role {
            MappingRole::Cover => MappingBecomes::Cover,
            MappingRole::Audio
            | MappingRole::Artwork
            | MappingRole::Document
            | MappingRole::Other => MappingBecomes::Kept,
        },
        source: MappingSource::File(mapping_file(entry, role, None)),
    })
}

/// The collapsed-directory row a file is stood for by, where it is in one.
fn collapsed_row<'a>(
    collapsed: &'a [CollapsedDirectory],
    entry: &CandidateFile,
) -> Option<&'a CollapsedDirectory> {
    let dir_prefix = entry.file.dir_prefix.as_deref()?;
    collapsed
        .iter()
        .find(|directory| directory.dir_prefix == dir_prefix)
}

/// One of the folder's audio files, as the container a sheet's header names.
fn container(audio: &ScannedFile) -> MappingContainer {
    MappingContainer {
        file_id: audio.relative_path.clone(),
        name: audio.file_name.clone(),
        size: audio.size,
    }
}

/// The container a binding names, or `None` when the folder no longer holds it
/// as audio — which is the same thing as the sheet describing nothing.
fn container_of(files: &CategorizedFiles, file_id: &str) -> Option<MappingContainer> {
    files
        .audio()
        .find(|audio| audio.relative_path == file_id)
        .map(container)
}

/// What a sheet describes, as its header states it.
///
/// A binding naming audio the folder no longer holds as audio — a container
/// somebody took out of the tracklist — describes nothing, and says so with what
/// the directive asked for, which is the same answer an unresolved directive
/// gives.
fn bound_of(files: &CategorizedFiles, sheet: &CueSheet, binding: &SheetBinding) -> SheetBound {
    let unresolved = || SheetBound::Unresolved {
        requested: sheet
            .audio_file_references()
            .into_iter()
            .map(str::to_string)
            .collect(),
    };
    match binding {
        SheetBinding::Unresolved => unresolved(),
        SheetBinding::Describes { file_id } => match container_of(files, file_id) {
            Some(container) => SheetBound::Describes(container),
            None => unresolved(),
        },
        SheetBinding::RefusedCodec { file_id, codec } => match container_of(files, file_id) {
            Some(container) => SheetBound::RefusedCodec {
                container,
                codec: codec.clone(),
            },
            None => unresolved(),
        },
    }
}

/// The tally over a table's rows: how many will write a track against how many
/// the picked release names.
///
/// The same rule [`slot_table`](crate::import::track_slots::slot_table) states
/// over its own two sides, asked of the rows that are left — so a table nobody
/// has edited restates the number it was built with, and one a row has left
/// restates it without re-opening the folder.
fn tally(rows: &[MappingRow]) -> SlotReconciliation {
    let units: Vec<&MappingUnit> = rows.iter().flat_map(MappingRow::units).collect();
    let files = units
        .iter()
        .filter(|unit| matches!(&unit.becomes, MappingBecomes::Track { track, .. } if track.file.is_some()))
        .count() as u32;
    let tracks = units
        .iter()
        .filter(|unit| {
            matches!(
                &unit.becomes,
                MappingBecomes::Track {
                    source_position: Some(_),
                    ..
                }
            )
        })
        .count() as u32;
    match files.cmp(&tracks) {
        std::cmp::Ordering::Equal => SlotReconciliation::Agrees { count: files },
        std::cmp::Ordering::Greater => SlotReconciliation::MoreFiles { files, tracks },
        std::cmp::Ordering::Less => SlotReconciliation::MoreTracks { files, tracks },
    }
}

/// Write an edited track row back onto the row that commits it, found by the
/// track's own identity — which the projection makes unique across the table.
///
/// A row nothing matches leaves the table alone: an editor holding a row the
/// table no longer has is editing something that has already left it.
pub fn mapping_with_track(mut table: MappingTable, track: RawTrackEdit) -> MappingTable {
    let mut wrote = false;
    for unit in table.rows.iter_mut().flat_map(MappingRow::units_mut) {
        let MappingBecomes::Track {
            track: existing, ..
        } = &mut unit.becomes
        else {
            continue;
        };
        if existing.id == track.id {
            *existing = track.clone();
            wrote = true;
        }
    }
    if !wrote {
        warn!("{} is not a row of this mapping table", track.id);
    }
    table
}

/// Drop the row that commits the track with `track_id` — a track the release
/// names that this folder has nothing for, taken out of the import.
///
/// Nothing is persisted: the folder is unchanged, the release is simply
/// committed without that track.
pub fn mapping_without_track(table: MappingTable, track_id: &str) -> MappingTable {
    remove(
        table,
        &|unit| matches!(&unit.becomes, MappingBecomes::Track { track, .. } if track.id == track_id),
    )
}

/// Drop every row the file `file_id` backs.
///
/// One container backs every entry of the sheet bound to it, so that sheet's
/// whole group leaves with it — the group *is* the container's rows. Excluding a
/// file the tracklist does not draw on leaves the table as it was.
pub fn mapping_without_file(table: MappingTable, file_id: &str) -> MappingTable {
    let mut table = table;
    table.rows.retain(|row| match row {
        MappingRow::Sheet { sheet, .. } => sheet.bound.container_id() != Some(file_id),
        MappingRow::Unit(_) | MappingRow::Directory(_) => true,
    });
    remove(table, &|unit| match &unit.source {
        MappingSource::File(file) => file.file_id == file_id,
        MappingSource::SheetEntry(entry) => entry.container_id == file_id,
        MappingSource::Missing => false,
    })
}

/// Drop every unit the predicate names, wherever it sits, and restate the tally
/// over what is left. A table with no tally keeps none — the folder's own tags
/// cannot disagree with the folder.
fn remove(mut table: MappingTable, should_remove: &dyn Fn(&MappingUnit) -> bool) -> MappingTable {
    table.rows.retain_mut(|row| match row {
        MappingRow::Unit(unit) => !should_remove(unit),
        MappingRow::Sheet { entries, .. } => {
            entries.retain(|entry| !should_remove(entry));
            true
        }
        MappingRow::Directory(_) => true,
    });
    if table.reconciliation.is_some() {
        table.reconciliation = Some(tally(&table.rows));
    }
    table
}

/// The left half of a file's row: what the folder holds, and the roles it may
/// be put in.
fn mapping_file(
    entry: &CandidateFile,
    role: MappingRole,
    probed_duration_ms: Option<u64>,
) -> MappingFile {
    MappingFile {
        file_id: entry.file.relative_path.clone(),
        name: entry.file.file_name.clone(),
        size: entry.file.size,
        path: entry.file.path.clone(),
        probed_duration_ms,
        role,
        alternatives: entry.role_alternatives().to_vec(),
        role_choice: entry.role_choice(),
    }
}

/// The discs a sheet of this folder may be assigned to.
///
/// One per disc the picked tracklist names, and never fewer than one per track
/// sheet the folder binds: a folder holding three sheets can always be told
/// which sheet is which, whatever the release the metadata came from says.
fn disc_options(files: &CategorizedFiles, picked: Option<&PickedTracklist<'_>>) -> Vec<u32> {
    let named = picked.map_or(0, |picked| {
        picked
            .slots
            .rows
            .iter()
            .map(|row| row.track().side)
            .collect::<BTreeSet<_>>()
            .len()
    }) as u32;
    let bound = files.bound_sheets().len() as u32;
    (1..=named.max(bound).max(1)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::folder_scanner::{
        collect_release_candidate_files_with_scope, StoredCandidateEdits,
    };
    use crate::import::track_slots::{slot_table, SourceTrack};
    use crate::import::TrackUserEdit;
    use std::fs;
    use std::path::Path;

    /// 44.1 kHz / 2-channel / 16-bit STREAMINFO declaring one second of audio —
    /// enough for the scan's validation and the container probe.
    ///
    /// The 34-byte STREAMINFO packs the sample rate (20 bits), channels − 1
    /// (3 bits) and bits-per-sample − 1 (5 bits) across three bytes, then the
    /// total sample count and an MD5 signature.
    fn synthetic_flac_bytes() -> Vec<u8> {
        const CHANNELS_MINUS_1: u8 = 1;
        const BPS_MINUS_1: u8 = 15;
        let sample_rate: u32 = 44_100;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        buf.extend_from_slice(&[0x80, 0x00, 0x00, 34]);
        buf.extend_from_slice(&[0x10, 0x00, 0x10, 0x00]);
        buf.extend_from_slice(&[0u8; 6]);
        buf.push((sample_rate >> 12) as u8);
        buf.push(((sample_rate >> 4) & 0xFF) as u8);
        buf.push(
            (((sample_rate & 0x0F) as u8) << 4) | (CHANNELS_MINUS_1 << 1) | (BPS_MINUS_1 >> 4),
        );
        buf.push((BPS_MINUS_1 & 0x0F) << 4);
        buf.extend_from_slice(&44_100u32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 16]);
        buf.resize(18_000, 0);
        buf
    }

    fn write_flac(path: &Path) {
        fs::write(path, synthetic_flac_bytes()).expect("write flac");
    }

    /// A sheet naming one container for the whole disc, its entries a fifth of
    /// a second apart so every entry but the last has a length of its own.
    fn cue_sheet_text(audio_file_name: &str, count: usize) -> String {
        let mut text = String::from("PERFORMER \"Artist Name\"\nTITLE \"Album Title\"\n");
        text.push_str(&format!("FILE \"{audio_file_name}\" WAVE\n"));
        for index in 0..count {
            text.push_str(&format!("  TRACK {:02} AUDIO\n", index + 1));
            text.push_str(&format!("    TITLE \"Sheet Track {}\"\n", index + 1));
            text.push_str(&format!("    INDEX 01 00:00:{:02}\n", index * 15));
        }
        text
    }

    fn scan(root: &Path) -> CategorizedFiles {
        collect_release_candidate_files_with_scope(
            root,
            crate::import::ReleaseFileScope::Recursive,
            &StoredCandidateEdits::none(),
        )
        .expect("scan succeeds")
    }

    fn source_tracks(count: usize) -> Vec<SourceTrack> {
        (0..count)
            .map(|index| SourceTrack {
                edit: TrackUserEdit {
                    title: format!("Track Title {}", index + 1),
                    side: 1,
                    track_number: Some(index as i32 + 1),
                    artist_names: Vec::new(),
                    file: None,
                },
                position: (index + 1).to_string(),
                duration_ms: Some(180_000),
            })
            .collect()
    }

    fn becomes(row: &MappingRow) -> Vec<&MappingBecomes> {
        match row {
            MappingRow::Unit(unit) => vec![&unit.becomes],
            MappingRow::Sheet { entries, .. } => entries.iter().map(|e| &e.becomes).collect(),
            MappingRow::Directory(_) => Vec::new(),
        }
    }

    fn file_row(row: &MappingRow) -> &MappingFile {
        match row {
            MappingRow::Unit(MappingUnit {
                source: MappingSource::File(file),
                ..
            }) => file,
            other => panic!("expected a file row, got {other:?}"),
        }
    }

    /// Nothing is picked yet, so every audio row is an open question — but a
    /// cover is still the cover and a rip log is still carried, because a role
    /// is a fact about the folder and needs no release.
    #[test]
    fn with_no_pick_the_audio_rows_await_one_and_the_rest_still_say_what_they_become() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));
        fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");
        fs::write(tmp.path().join("rip.log"), b"log").expect("write log");

        let table = mapping_table(&scan(tmp.path()), None);

        assert!(table.reconciliation.is_none());
        let kinds: Vec<(&str, &MappingBecomes)> = table
            .rows
            .iter()
            .map(|row| (file_row(row).name.as_str(), becomes(row)[0]))
            .collect();
        assert!(matches!(
            kinds[0],
            ("01.flac", MappingBecomes::AwaitingPick)
        ));
        assert!(matches!(
            kinds[1],
            ("02.flac", MappingBecomes::AwaitingPick)
        ));
        assert!(matches!(kinds[2], ("cover.jpg", MappingBecomes::Cover)));
        assert!(matches!(kinds[3], ("rip.log", MappingBecomes::Kept)));
        // A row nothing has opened has no probed length to show.
        assert_eq!(file_row(&table.rows[0]).probed_duration_ms, None);
        assert_eq!(file_row(&table.rows[0]).role, MappingRole::Audio);
        assert_eq!(file_row(&table.rows[3]).role, MappingRole::Document);
    }

    /// A bound sheet is one group row over its entries: the entries carry the
    /// sheet's own titles and timings on the left, and on the right each is the
    /// track the pick puts on that slice.
    #[test]
    fn a_sheet_s_entries_carry_its_own_titles_and_bind_to_its_slices() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 3),
        )
        .expect("write cue");

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(3), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );

        assert_eq!(table.rows.len(), 1, "the sheet is the folder's only row");
        let MappingRow::Sheet { sheet, entries } = &table.rows[0] else {
            panic!("expected a sheet row, got {:?}", table.rows[0]);
        };
        assert_eq!(sheet.sheet_id, "CDImage.cue");
        assert_eq!(sheet.assignment, SheetDisc::Disc { number: 1 });
        assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
        let SheetBound::Describes(container) = &sheet.bound else {
            panic!("expected a bound sheet, got {:?}", sheet.bound);
        };
        assert_eq!(container.name, "CDImage.flac");
        assert_eq!(entries.len(), 3);

        for (index, entry) in entries.iter().enumerate() {
            let MappingSource::SheetEntry(source) = &entry.source else {
                panic!("expected a sheet entry, got {:?}", entry.source);
            };
            assert_eq!(source.index, index as u32);
            assert_eq!(source.number, index as u32 + 1);
            assert_eq!(
                source.title.as_deref(),
                Some(&*format!("Sheet Track {}", index + 1))
            );
            assert_eq!(source.container_id, "CDImage.flac");
            // Every entry but the last has a next-entry boundary in the sheet.
            assert_eq!(source.duration_ms.is_some(), index < 2);

            let MappingBecomes::Track { track, .. } = &entry.becomes else {
                panic!("expected a track, got {:?}", entry.becomes);
            };
            assert_eq!(
                track.file,
                Some(AudioFile::SheetSlice {
                    file_id: "CDImage.flac".to_string(),
                    sheet_id: "CDImage.cue".to_string(),
                    index: index as u32,
                }),
            );
            // The right half is the release's tracklist, not the sheet's.
            assert_eq!(track.title, format!("Track Title {}", index + 1));
        }
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::Agrees { count: 3 }),
        );
    }

    /// A release naming more tracks than the folder holds closes the table with
    /// one empty-left row per track nothing backs.
    #[test]
    fn tracks_the_folder_has_nothing_for_close_the_table() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(4), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );

        assert_eq!(table.rows.len(), 4);
        assert!(matches!(
            table.rows[2],
            MappingRow::Unit(MappingUnit {
                source: MappingSource::Missing,
                ..
            }),
        ));
        let MappingRow::Unit(MappingUnit {
            becomes: MappingBecomes::Track { track, .. },
            ..
        }) = &table.rows[3]
        else {
            panic!("expected a track row, got {:?}", table.rows[3]);
        };
        assert_eq!(track.title, "Track Title 4");
        assert_eq!(track.file, None, "nothing on disk backs it");
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::MoreTracks {
                files: 2,
                tracks: 4,
            }),
        );
    }

    /// The tracks the commit writes are the table's own rows, in the order the
    /// table lays them out, each addressable on its own.
    #[test]
    fn the_commit_tracks_are_the_table_s_rows_in_order() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 2),
        )
        .expect("write cue");
        write_flac(&tmp.path().join("bonus.flac"));
        fs::write(tmp.path().join("cover.jpg"), fake_jpeg()).expect("write cover");

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(4), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );

        let tracks = mapping_tracks(&table);
        assert_eq!(
            tracks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec![
                "import-track-0",
                "import-track-1",
                "import-track-2",
                "import-track-3",
            ],
        );
        assert_eq!(
            tracks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            vec![
                "Track Title 1",
                "Track Title 2",
                "Track Title 3",
                "Track Title 4",
            ],
        );
        // The cover is not a track, the sheet's two slices lead the bonus file
        // exactly as the folder's audio units do, and the fourth track is the
        // one the folder has nothing for.
        assert_eq!(
            tracks.iter().map(|t| t.file.clone()).collect::<Vec<_>>(),
            vec![
                Some(AudioFile::SheetSlice {
                    file_id: "CDImage.flac".to_string(),
                    sheet_id: "CDImage.cue".to_string(),
                    index: 0,
                }),
                Some(AudioFile::SheetSlice {
                    file_id: "CDImage.flac".to_string(),
                    sheet_id: "CDImage.cue".to_string(),
                    index: 1,
                }),
                Some(AudioFile::Standalone {
                    file_id: "bonus.flac".to_string(),
                }),
                None,
            ],
        );
    }

    /// A sheet whose `FILE` directive names audio that is not in the folder
    /// describes nothing — and says what it was looking for, so the header can
    /// state it while it offers the folder's own audio instead. It also carries
    /// its own path, which is what opens it in the document viewer.
    #[test]
    fn a_sheet_that_describes_nothing_says_what_it_asked_for() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.wav", 3),
        )
        .expect("write cue");

        let table = mapping_table(&scan(tmp.path()), None);
        // The sheet is named where it sits on disk, after the loose audio that
        // sorts before it — a sheet that carves nothing occupies no run.
        let Some(MappingRow::Sheet { sheet, entries }) = table
            .rows
            .iter()
            .find(|row| matches!(row, MappingRow::Sheet { .. }))
        else {
            panic!("expected a sheet row among {:?}", table.rows);
        };
        assert_eq!(
            sheet.bound,
            SheetBound::Unresolved {
                requested: vec!["CDImage.wav".to_string()],
            },
        );
        assert_eq!(sheet.path, tmp.path().join("CDImage.cue"));
        assert!(entries.is_empty(), "it carves nothing");
    }

    /// Editing a row writes the track back onto the row that commits it, found
    /// by the track's own id, and leaves every other row alone.
    #[test]
    fn with_track_writes_the_edited_row_back_by_its_id() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(2), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );

        let mut edited = mapping_tracks(&table)[1].clone();
        edited.title = "Renamed".to_string();
        let table = mapping_with_track(table, edited);

        let titles: Vec<String> = mapping_tracks(&table)
            .into_iter()
            .map(|track| track.title)
            .collect();
        assert_eq!(titles, vec!["Track Title 1", "Renamed"]);
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::Agrees { count: 2 }),
            "naming a row changes nothing about the tally",
        );
    }

    /// Dropping a track the folder has nothing for takes its row out and
    /// restates the tally over what is left.
    #[test]
    fn without_track_drops_the_row_and_restates_the_tally() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(3), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::MoreTracks {
                files: 2,
                tracks: 3,
            }),
        );

        let table = mapping_without_track(table, "import-track-2");

        assert_eq!(table.rows.len(), 2);
        assert_eq!(mapping_tracks(&table).len(), 2);
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::Agrees { count: 2 }),
        );
    }

    /// Excluding the audio a sheet describes takes the whole group with it:
    /// twelve entries are one file's rows, and the file is leaving.
    #[test]
    fn without_file_takes_a_sheet_s_whole_group_with_its_container() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("CDImage.flac"));
        fs::write(
            tmp.path().join("CDImage.cue"),
            cue_sheet_text("CDImage.flac", 3),
        )
        .expect("write cue");
        write_flac(&tmp.path().join("bonus.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(3), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::MoreFiles {
                files: 4,
                tracks: 3,
            }),
        );

        let table = mapping_without_file(table, "CDImage.flac");

        assert_eq!(table.rows.len(), 1, "only the bonus file is left");
        assert_eq!(mapping_tracks(&table).len(), 1);
        assert_eq!(
            table.reconciliation,
            Some(SlotReconciliation::MoreFiles {
                files: 1,
                tracks: 0,
            }),
            "the three tracks the release names left with the audio backing them",
        );
    }

    /// Excluding a file the table holds no rows for changes nothing — the same
    /// table, the same tally.
    #[test]
    fn without_file_for_something_the_table_does_not_hold_changes_nothing() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(2), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "import-track",
                source: TracklistSource::Release,
            }),
        );

        let after = mapping_without_file(table.clone(), "nothing-here.flac");

        assert_eq!(after.rows.len(), table.rows.len());
        assert_eq!(mapping_tracks(&after), mapping_tracks(&table));
        assert_eq!(after.reconciliation, table.reconciliation);
    }

    /// A table with no tally keeps none through an edit: the folder's own tags
    /// cannot disagree with the folder, however many rows are left.
    #[test]
    fn an_edit_to_a_table_with_no_tally_leaves_it_without_one() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        write_flac(&tmp.path().join("01.flac"));
        write_flac(&tmp.path().join("02.flac"));

        let files = scan(tmp.path());
        let slots = slot_table(&source_tracks(2), &files);
        let table = mapping_table(
            &files,
            Some(PickedTracklist {
                slots: &slots,
                track_id_prefix: "unknown-track",
                source: TracklistSource::FileTags,
            }),
        );
        assert!(table.reconciliation.is_none());

        let table = mapping_without_file(table, "01.flac");

        assert_eq!(table.rows.len(), 1);
        assert!(table.reconciliation.is_none());
    }

    /// JPEG magic bytes — what the scan's image validation reads.
    fn fake_jpeg() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]
    }
}
