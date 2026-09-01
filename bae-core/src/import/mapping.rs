//! The mapping table: every source unit the folder offers, alongside the track
//! committing makes of it.
//!
//! One structure, not two lists to keep aligned. The editable track row lives
//! *inside* the row that produces it, so removing a row removes both halves and
//! no index addresses anything — which is what keeps the joining out of the
//! surfaces that render it.

use crate::cue_flac::CueSheet;
use crate::import::folder_scanner::{
    BoundTrackSheet, CandidateFile, CategorizedFiles, FileRole, FileRoleChoice, ScannedFile,
    SheetBinding, SheetDisc,
};
use crate::import::probe::SourceDurations;
use crate::import::track_slots::{
    audio_layout, units_of, SlotReconciliation, SlotTable, TrackSlot, UnitContribution,
};
use crate::import::types::{AudioFile, RawTrackEdit};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use tracing::warn;

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<MappingImage>,
    /// The rows that can become release tracks, with a bound sheet retaining
    /// ownership of the slices it carves.
    pub track_groups: Vec<MappingTrackGroup>,
    /// Files carried with the release but not represented by track rows.
    pub files: Vec<MappingFileRow>,
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
            images: Vec::new(),
            track_groups: Vec::new(),
            files: Vec::new(),
            reconciliation: None,
        }
    }
}

/// One group in the track section of the mapping table.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingTrackGroup {
    /// One source unit and what it becomes.
    Unit(MappingUnit),
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: SheetGroup,
        entries: Vec<MappingUnit>,
    },
}

impl MappingTrackGroup {
    /// The units this group carries: itself, or the entries a sheet carves.
    pub fn units(&self) -> &[MappingUnit] {
        match self {
            Self::Unit(unit) => std::slice::from_ref(unit),
            Self::Sheet { entries, .. } => entries.as_slice(),
        }
    }

    fn units_mut(&mut self) -> &mut [MappingUnit] {
        match self {
            Self::Unit(unit) => std::slice::from_mut(unit),
            Self::Sheet { entries, .. } => entries.as_mut_slice(),
        }
    }
}

/// One row in the files section of the mapping table.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingFileRow {
    File(MappingFile),
    /// A sheet that currently carves no track rows and can be assigned audio.
    Sheet(SheetGroup),
}

/// One of the folder's images, as the gallery shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    pub size: u64,
    /// Absolute path — what a thumbnail and the lightbox read.
    pub path: PathBuf,
}

/// One source unit, and the track committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingUnit {
    pub source: MappingSource,
    pub becomes: MappingBecomes,
    /// The duration this row displays: the metadata source's value where one
    /// exists, otherwise the candidate's stored probe. This remains available
    /// while the row is waiting for metadata.
    pub duration_ms: Option<u64>,
}

/// The left half of a row: what the folder offers for it.
#[derive(Debug, Clone, PartialEq)]
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
/// it heads a group of rows — and images live in the table's gallery instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingRole {
    /// Playable audio.
    Audio,
    /// Readable evidence: a rip log, a tracklist, a playlist, or a CUE that
    /// could not be parsed as a sheet.
    Document,
    /// In the folder and carried with the release, unrecognized.
    Other,
}

/// A file of the folder, as the mapping table's left half shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingFile {
    pub file_id: String,
    /// The file's identity within the release — its relative path, the same
    /// name the storage manager lists it under. Every file lists flat; a
    /// directory shows up only as the prefix its files carry.
    pub name: String,
    pub size: u64,
    pub path: PathBuf,
    /// The whole-file audition target when this file currently supplies audio.
    pub preview_target: Option<crate::playback::PreviewTarget>,
    /// Playing time from the scan's stored facts. `None` for non-audio files.
    pub duration_ms: Option<u64>,
    pub audio_format: Option<crate::album_detail::AudioFormat>,
    pub role: MappingRole,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make.
    pub alternatives: Vec<FileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when [`Self::alternatives`] is empty.
    pub role_choice: Option<FileRoleChoice>,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    pub title: Option<String>,
    /// This slice's stored source duration: the next sheet boundary, or the
    /// scanned container duration closing the final entry.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    pub container_path: PathBuf,
    /// The exact window of the container that auditioning this entry plays.
    pub preview_target: crate::playback::PreviewTarget,
    pub audio_format: crate::album_detail::AudioFormat,
}

/// The right half of a row: what committing makes of the source unit.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingBecomes {
    /// A track of the release being committed. The row edits it in place.
    Track {
        track: RawTrackEdit,
        /// The position this row commits, rendered from the track's own side
        /// and number and the release's format — `8`, `A1`, `2-3`. `None`
        /// where the track has no number. The same fact in every metadata
        /// mode, because it reads the draft rather than the picked source.
        position: Option<String>,
        /// Whether the source's tracklist contains this track. False exactly
        /// for a row that exists only because audio was found for it.
        named_by_source: bool,
    },
    /// No release is picked yet, so what this becomes is the open question.
    AwaitingPick,
}

/// A track sheet, as the header of the group of rows it carves.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetGroup {
    pub sheet_id: String,
    pub name: String,
    pub size: u64,
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
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct MappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub audio_format: crate::album_detail::AudioFormat,
}

/// Where the tracklist a folder is being committed as came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracklistSource {
    /// A release picked from a metadata source. Its tracklist and the folder's
    /// audio are two independent accounts of one disc, so the table tallies
    /// them against each other.
    ExternalRelease,
    /// Track rows derived from the candidate's files, either from their tags
    /// or as blank manual rows. They cannot disagree with the candidate because
    /// the candidate itself determines their physical slots.
    CandidateFiles,
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
    /// The pressing format of the release being committed — what decides
    /// whether a row's position reads `8`, `A1`, or `2-3`.
    pub format: Option<&'a str>,
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
    durations: &SourceDurations,
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

    let disc_options = disc_options(files, picked.as_ref());

    let multi_side = picked.as_ref().is_some_and(|picked| {
        let mut sides = picked.slots.rows.iter().map(|slot| slot.track().side);
        let first = sides.next();
        sides.any(|side| Some(side) != first)
    });
    let mut builder = RowBuilder {
        picked,
        durations,
        slot_of,
        next_track: 0,
        multi_side,
    };
    let mut track_groups = Vec::with_capacity(units.len());
    let mut file_rows = Vec::with_capacity(files.files.len().saturating_sub(units.len()));
    // The folder's images become one gallery beside the rows while retaining
    // the scan's order.
    let mut images: Vec<MappingImage> = Vec::new();

    for entry in &files.files {
        match &entry.role {
            // A carving sheet is named at the position its run occupies, which
            // the assignment decides and the sheet's own place on disk does not.
            FileRole::TrackSheet { .. } if carving.contains(entry.file.relative_path.as_str()) => {}
            FileRole::TrackSheet {
                sheet,
                binding,
                disc,
            } => file_rows.push(MappingFileRow::Sheet(SheetGroup {
                sheet_id: entry.file.relative_path.clone(),
                name: entry.file.relative_path.clone(),
                size: entry.file.size,
                path: entry.file.path.clone(),
                bound: bound_of(files, sheet, binding),
                assignment: *disc,
                disc_options: disc_options.clone(),
            })),
            FileRole::Audio => match contributions.get(entry.file.relative_path.as_str()) {
                Some(UnitContribution::Runs(sheets)) => {
                    for sheet in sheets.iter() {
                        track_groups.push(MappingTrackGroup::Sheet {
                            sheet: SheetGroup {
                                sheet_id: sheet.file.relative_path.clone(),
                                name: sheet.file.relative_path.clone(),
                                size: sheet.file.size,
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
                    track_groups.push(MappingTrackGroup::Unit(builder.audio_row(entry)))
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
            FileRole::Artwork => {
                images.push(mapping_image(entry));
            }
            FileRole::Document => file_rows.push(carried(entry, MappingRole::Document)),
            FileRole::Other => file_rows.push(carried(entry, MappingRole::Other)),
        }
    }

    // The tracks the source names and the folder has nothing for. They sit past
    // every unit in the slot table, so they close the table.
    if let Some(picked) = picked {
        for index in units.len()..picked.slots.rows.len() {
            let (becomes, metadata_duration_ms) = builder.track_at(index);
            track_groups.push(MappingTrackGroup::Unit(MappingUnit {
                source: MappingSource::Missing,
                becomes,
                duration_ms: metadata_duration_ms,
            }));
        }
    }

    let reconciliation = picked
        .filter(|picked| picked.source == TracklistSource::ExternalRelease)
        .map(|_| tally(&track_groups));
    MappingTable {
        images,
        track_groups,
        files: file_rows,
        reconciliation,
    }
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes.
pub fn mapping_tracks(table: &MappingTable) -> Vec<RawTrackEdit> {
    table
        .track_groups
        .iter()
        .flat_map(MappingTrackGroup::units)
        .filter_map(|unit| match &unit.becomes {
            MappingBecomes::Track { track, .. } => Some(track.clone()),
            MappingBecomes::AwaitingPick => None,
        })
        .collect()
}

/// The running state of one projection: which tracklist it is pairing against,
/// and how many track rows it has emitted.
struct RowBuilder<'a> {
    picked: Option<PickedTracklist<'a>>,
    durations: &'a SourceDurations,
    slot_of: HashMap<AudioFile, usize>,
    next_track: usize,
    /// Whether the tracklist spans more than one side or disc — what decides
    /// that a row's position carries its side.
    multi_side: bool,
}

impl RowBuilder<'_> {
    /// One row for a loose audio file.
    fn audio_row(&mut self, entry: &CandidateFile) -> MappingUnit {
        let unit = AudioFile::Standalone {
            file_id: entry.file.relative_path.clone(),
        };
        let duration_ms = self.duration_ms(&unit);
        self.unit(
            &unit,
            MappingSource::File(mapping_file(entry, MappingRole::Audio, duration_ms)),
            duration_ms,
        )
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
                let duration_ms = self.duration_ms(&unit);
                let sample_rate = u64::try_from(
                    sheet
                        .audio
                        .source_audio
                        .as_ref()
                        .expect("a scanned audio file has source facts")
                        .format
                        .sample_rate_hz,
                )
                .expect("a scanned audio file has a non-negative sample rate");
                let preview_target = crate::playback::PreviewTarget::sample_range(
                    sheet.audio.path.to_string_lossy().into_owned(),
                    crate::cue_flac::cue_frames_to_samples(track.start_cue_frames, sample_rate),
                    track
                        .end_cue_frames
                        .map(|frames| crate::cue_flac::cue_frames_to_samples(frames, sample_rate)),
                );
                self.unit(
                    &unit,
                    MappingSource::SheetEntry(MappingEntry {
                        sheet_id: sheet.file.relative_path.clone(),
                        index: index as u32,
                        number: track.number,
                        title: track.title.clone(),
                        duration_ms,
                        container_id: sheet.audio.relative_path.clone(),
                        container_name: sheet.audio.file_name.clone(),
                        container_path: sheet.audio.path.clone(),
                        audio_format: sheet
                            .audio
                            .source_audio
                            .as_ref()
                            .expect("a scanned audio file has source facts")
                            .format
                            .clone(),
                        preview_target,
                    }),
                    duration_ms,
                )
            })
            .collect()
    }

    /// This unit's playing time as the stored measurements record it. A unit
    /// nothing has read yet shows none, whether or not a release is picked.
    fn duration_ms(&self, unit: &AudioFile) -> Option<u64> {
        self.durations.duration_of(unit)
    }

    /// What the unit becomes: the track the picked tracklist puts on it, or the
    /// open question a folder with no pick leaves.
    fn unit(
        &mut self,
        unit: &AudioFile,
        source: MappingSource,
        probed_duration_ms: Option<u64>,
    ) -> MappingUnit {
        let (becomes, metadata_duration_ms) = self.becomes_for(unit);
        MappingUnit {
            source,
            becomes,
            duration_ms: metadata_duration_ms.or(probed_duration_ms),
        }
    }

    fn becomes_for(&mut self, unit: &AudioFile) -> (MappingBecomes, Option<u64>) {
        if self.picked.is_none() {
            return (MappingBecomes::AwaitingPick, None);
        }
        let Some(&index) = self.slot_of.get(unit) else {
            // Every unit this asks about was read off the same layout the index
            // was built from, so a unit missing from it cannot be produced.
            warn!("{unit:?} is not one of this folder's audio units");
            return (MappingBecomes::AwaitingPick, None);
        };
        self.track_at(index)
    }

    /// The track at slot row `index`, taking the next row identity.
    fn track_at(&mut self, index: usize) -> (MappingBecomes, Option<u64>) {
        let Some(picked) = self.picked else {
            return (MappingBecomes::AwaitingPick, None);
        };
        let Some(slot) = picked.slots.rows.get(index) else {
            // Slot row `i` is audio unit `i` and the table is never shorter
            // than the folder's units, so a caller that pairs a folder with
            // another folder's slots is the only way here.
            warn!(
                "the picked tracklist has no row {index}; it does not describe this folder's audio"
            );
            return (MappingBecomes::AwaitingPick, None);
        };
        let id = format!("{}-{}", picked.track_id_prefix, self.next_track);
        self.next_track += 1;
        let (named_by_source, source_duration_ms) = match slot {
            TrackSlot::Paired {
                named_by_source,
                source_duration_ms,
                ..
            }
            | TrackSlot::TrackOnly {
                named_by_source,
                source_duration_ms,
                ..
            } => (*named_by_source, *source_duration_ms),
            TrackSlot::FileOnly { .. } => (false, None),
        };
        let edit = slot.track();
        let position = crate::util::format::ungrouped_track_position_text(
            &crate::util::format::compute_track_position(
                picked.format,
                edit.side,
                edit.track_number,
                self.multi_side,
            ),
        );
        (
            MappingBecomes::Track {
                track: RawTrackEdit::from_user_edit(edit.clone(), id),
                position,
                named_by_source,
            },
            source_duration_ms,
        )
    }
}

/// One row for a file that is not one of the release's tracks: something the
/// folder carries alongside them. Nothing has to be opened to know what it
/// becomes, so it shows no source length.
fn carried(entry: &CandidateFile, role: MappingRole) -> MappingFileRow {
    MappingFileRow::File(mapping_file(entry, role, None))
}

/// One of the folder's images, as the gallery carries it.
///
/// Which image leads the release is not a property of the image: it is the
/// cover choice, which the stored row answers first, then the picked release's
/// own art, then the folder's images by name. The gallery lists what the folder
/// has; the card shows what was chosen.
fn mapping_image(entry: &CandidateFile) -> MappingImage {
    MappingImage {
        file_id: entry.file.relative_path.clone(),
        name: entry.file.file_name.clone(),
        size: entry.file.size,
        path: entry.file.path.clone(),
    }
}

/// One of the folder's audio files, as the container a sheet's header names.
fn container(audio: &ScannedFile) -> MappingContainer {
    MappingContainer {
        file_id: audio.relative_path.clone(),
        name: audio.file_name.clone(),
        size: audio.size,
        audio_format: audio
            .source_audio
            .as_ref()
            .expect("a scanned audio file has source facts")
            .format
            .clone(),
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
fn tally(groups: &[MappingTrackGroup]) -> SlotReconciliation {
    let units: Vec<&MappingUnit> = groups.iter().flat_map(MappingTrackGroup::units).collect();
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
                    named_by_source: true,
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
    for unit in table
        .track_groups
        .iter_mut()
        .flat_map(MappingTrackGroup::units_mut)
    {
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

/// Drop every unit the predicate names, wherever it sits, and restate the tally
/// over what is left. A table with no tally keeps none — the folder's own tags
/// cannot disagree with the folder.
fn remove(mut table: MappingTable, should_remove: &dyn Fn(&MappingUnit) -> bool) -> MappingTable {
    table.track_groups.retain_mut(|group| match group {
        MappingTrackGroup::Unit(unit) => !should_remove(unit),
        MappingTrackGroup::Sheet { entries, .. } => {
            entries.retain(|entry| !should_remove(entry));
            true
        }
    });
    if table.reconciliation.is_some() {
        table.reconciliation = Some(tally(&table.track_groups));
    }
    table
}

/// The left half of a file's row: what the folder holds, and the roles it may
/// be put in.
fn mapping_file(entry: &CandidateFile, role: MappingRole, duration_ms: Option<u64>) -> MappingFile {
    let preview_target = (role == MappingRole::Audio).then(|| {
        crate::playback::PreviewTarget::whole_file(entry.file.path.to_string_lossy().into_owned())
    });
    MappingFile {
        file_id: entry.file.relative_path.clone(),
        name: entry.file.relative_path.clone(),
        size: entry.file.size,
        path: entry.file.path.clone(),
        preview_target,
        duration_ms,
        audio_format: entry
            .file
            .source_audio
            .as_ref()
            .map(|audio| audio.format.clone()),
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
#[path = "mapping_tests.rs"]
mod tests;
