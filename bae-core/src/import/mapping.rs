//! The mapping table: every source unit the folder offers, alongside the track
//! committing makes of it.
//!
//! The draft holds one track per audio unit, in the folder's order, so row `i`
//! pairs unit `i` with draft track `i`. Surfaces render this projection without
//! joining separate lists.

use crate::import::audio_layout::{audio_layout, UnitContribution};
use crate::import::folder_scanner::{
    BoundTrackSheet, CandidateFile, CategorizedFiles, FileRole, ScannedFile, SheetBinding,
    SheetDisc, SheetReferenceOptions, TrackSheetFile,
};
use crate::import::probe::SourceDurations;
use crate::import::types::{AudioFile, CandidateDraft, CandidateTrack, RawTrackEdit};
use crate::import::ImportError;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTable {
    /// Every image the folder holds, in the scan's authoritative order.
    pub images: Vec<MappingImage>,
    /// The rows that can become release tracks, with a bound sheet retaining
    /// ownership of the slices it carves.
    pub track_sections: Vec<MappingTrackSection>,
    /// Files carried with the release but not represented by track rows.
    pub files: Vec<MappingFileRow>,
}

/// One side or disc in the track table.
///
/// Its content makes the physical source shape explicit: a side is either
/// independent track rows or the entries carved by one track sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingTrackSection {
    pub side: crate::album_detail::TrackSide,
    pub content: MappingTrackSectionContent,
}

/// What supplies the rows of one side or disc.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingTrackSectionContent {
    /// Rows supplied independently rather than carved by a track sheet.
    Tracks(Vec<TrackMapping>),
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: SheetGroup,
        entries: Vec<TrackMapping>,
    },
}

impl MappingTrackSection {
    /// The source-to-track mappings this section carries.
    pub fn mappings(&self) -> &[TrackMapping] {
        match &self.content {
            MappingTrackSectionContent::Tracks(mappings) => mappings,
            MappingTrackSectionContent::Sheet { entries, .. } => entries,
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
    /// The version the scan read — what a thumbnail and the lightbox draw.
    pub file: crate::import::folder_scanner::FileVersion,
}

/// One source-to-track mapping row: an audio unit of the folder, and the
/// draft track committing makes of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackMapping {
    pub source: MappingSource,
    /// The track of the release being committed. The row edits it in place.
    pub track: RawTrackEdit,
    /// The position this row commits, rendered from the track's own side and
    /// number and the release's format — `8`, `A1`, or `3` beneath a `Disc 2`
    /// heading.
    pub position: String,
    /// The duration this row displays: the applied release's length for the
    /// track where it lists one, otherwise the candidate's stored probe.
    pub duration_ms: Option<u64>,
    /// Whether the folder's length for this row and the applied release's
    /// are far enough apart for the row to say so.
    pub lengths_disagree: bool,
}

/// The left half of a row: what the folder offers for it.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingSource {
    /// A file the folder holds, whole.
    File(MappingFile),
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry(MappingEntry),
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

/// A track sheet, as the header of the group of rows it carves.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetGroup {
    pub sheet_id: String,
    pub name: String,
    pub size: u64,
    /// Absolute path — what opening the sheet to read it reaches.
    pub path: PathBuf,
    pub bound: SheetBound,
    /// Current FILE associations and choices, independent of track inclusion.
    pub reference_options: Vec<SheetReferenceOptions>,
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
    /// The sheet describes several audio files, one per distinct `FILE`
    /// reference. The rows name their own physical files; the group header has
    /// no single container to name.
    DescribesFiles { audio_file_count: u32 },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The physical audio files import independently.
    RefusedCodec { codec: String },
    /// All files are associated, but their lengths cannot contain the CUE boundaries.
    RefusedTiming,
}

/// The audio a track sheet describes.
#[derive(Debug, Clone, PartialEq)]
pub struct MappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub audio_format: crate::album_detail::AudioFormat,
}

/// Project the mapping table for one folder and the draft committing it.
///
/// Refuses a draft whose tracks are not the folder's audio units in order: the
/// draft is redrawn with every change to the folder's files, so a mismatch is a
/// stored state that contradicts itself.
///
/// `source_lengths` are the lengths the applied release lists for its tracks,
/// by the source index a track keeps; empty when no release was applied. A row
/// one names shows that length beside its file's own.
pub(crate) fn mapping_table(
    files: &CategorizedFiles,
    durations: &SourceDurations,
    draft: &CandidateDraft,
    source_lengths: &[Option<u64>],
) -> Result<MappingTable, ImportError> {
    let layout = audio_layout(files);
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
    let disc_options = disc_options(files);
    let sides: BTreeSet<_> = draft.tracks.iter().map(|track| track.edit.side).collect();
    let mut rows = RowBuilder {
        durations,
        source_lengths,
        medium: draft.pressing.facts.physical_medium(),
        multi_side: sides.len() > 1,
        tracks: draft.tracks.iter(),
    };
    let mut track_sections = Vec::new();
    let mut file_rows = Vec::new();
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
                bound: bound_of(
                    files,
                    TrackSheetFile {
                        file: &entry.file,
                        sheet,
                        binding,
                        disc: *disc,
                    },
                ),
                reference_options: files.sheet_binding_options(&entry.file.relative_path),
                assignment: *disc,
                disc_options: if binding.is_resolved() {
                    disc_options.clone()
                } else {
                    Vec::new()
                },
            })),
            FileRole::Audio => match contributions.get(entry.file.relative_path.as_str()) {
                Some(UnitContribution::Runs(sheets)) => {
                    for sheet in sheets.iter() {
                        let group = SheetGroup {
                            sheet_id: sheet.file.relative_path.clone(),
                            name: sheet.file.relative_path.clone(),
                            size: sheet.file.size,
                            path: sheet.file.path.clone(),
                            bound: bound_sheet(sheet),
                            reference_options: files
                                .sheet_binding_options(&sheet.file.relative_path),
                            assignment: sheet.disc,
                            disc_options: disc_options.clone(),
                        };
                        for (side, mapping) in rows.sheet_entries(sheet)? {
                            push_sheet_entry(&mut track_sections, &group, side, mapping);
                        }
                    }
                }
                Some(UnitContribution::Whole) => {
                    let (side, mapping) = rows.audio_row(entry)?;
                    push_track_mapping(&mut track_sections, side, mapping);
                }
                // A carving sheet speaks for this file, so the sheet's rows are
                // what it contributes and it has none of its own.
                Some(UnitContribution::SpokenFor) => {}
                None => unreachable!(
                    "{} carries the audio role, and the layout places every audio file",
                    entry.file.relative_path
                ),
            },
            // Nothing else the folder holds is in the tracklist — the role says
            // so on its own. The folder is the release, so all of it is still
            // carried.
            FileRole::Artwork => images.push(mapping_image(entry)),
            FileRole::Document => file_rows.push(carried(entry, MappingRole::Document)),
            FileRole::Other => file_rows.push(carried(entry, MappingRole::Other)),
        }
    }
    if let Some(extra) = rows.tracks.next() {
        return Err(ImportError::Internal {
            detail: format!(
                "draft track {} has no audio unit of the folder behind it",
                extra.edit.id
            ),
        });
    }

    Ok(MappingTable {
        images,
        track_sections,
        files: file_rows,
    })
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes.
pub fn mapping_tracks(table: &MappingTable) -> Vec<RawTrackEdit> {
    table
        .track_sections
        .iter()
        .flat_map(MappingTrackSection::mappings)
        .map(|mapping| mapping.track.clone())
        .collect()
}

/// Pairs the folder's audio units, as the walk over its files reaches them,
/// with the draft's tracks in order.
struct RowBuilder<'a> {
    durations: &'a SourceDurations,
    source_lengths: &'a [Option<u64>],
    medium: Option<crate::pressing::PhysicalMedium>,
    /// Whether the draft spans more than one side or disc — what decides that
    /// a row's position carries its side.
    multi_side: bool,
    tracks: std::slice::Iter<'a, CandidateTrack>,
}

impl RowBuilder<'_> {
    /// One row for a loose audio file.
    fn audio_row(
        &mut self,
        entry: &CandidateFile,
    ) -> Result<(crate::album_detail::TrackSide, TrackMapping), ImportError> {
        let unit = AudioFile::Standalone {
            file_id: entry.file.relative_path.clone(),
        };
        let duration_ms = self.durations.duration_of(&unit);
        self.row(
            &unit,
            MappingSource::File(mapping_file(entry, MappingRole::Audio, duration_ms)),
            duration_ms,
        )
    }

    /// One row per entry a carving sheet describes.
    fn sheet_entries(
        &mut self,
        sheet: &BoundTrackSheet<'_>,
    ) -> Result<Vec<(crate::album_detail::TrackSide, TrackMapping)>, ImportError> {
        sheet
            .sheet
            .playable_tracks()
            .enumerate()
            .map(|(index, track)| {
                let audio = sheet.audio_for(track);
                let unit = AudioFile::SheetSlice {
                    file_id: audio.relative_path.clone(),
                    sheet_id: sheet.file.relative_path.clone(),
                    index: index as u32,
                };
                let duration_ms = self.durations.duration_of(&unit);
                let format = audio
                    .source_audio
                    .as_ref()
                    .expect("a scanned audio file has source facts")
                    .format
                    .clone();
                let sample_rate = u64::try_from(format.sample_rate_hz)
                    .expect("a scanned audio file has a non-negative sample rate");
                let preview_target = crate::playback::PreviewTarget::sample_range(
                    audio.path.to_string_lossy().into_owned(),
                    crate::cue_flac::cue_frames_to_samples(track.start_cue_frames, sample_rate),
                    track
                        .end_cue_frames
                        .map(|frames| crate::cue_flac::cue_frames_to_samples(frames, sample_rate)),
                );
                self.row(
                    &unit,
                    MappingSource::SheetEntry(MappingEntry {
                        sheet_id: sheet.file.relative_path.clone(),
                        index: index as u32,
                        number: track.number,
                        title: track.title.clone(),
                        duration_ms,
                        container_id: audio.relative_path.clone(),
                        container_name: audio.file_name.clone(),
                        container_path: audio.path.clone(),
                        audio_format: format,
                        preview_target,
                    }),
                    duration_ms,
                )
            })
            .collect()
    }

    /// Pair `unit` with the next draft track, and say which side it sits on.
    fn row(
        &mut self,
        unit: &AudioFile,
        source: MappingSource,
        probed_duration_ms: Option<u64>,
    ) -> Result<(crate::album_detail::TrackSide, TrackMapping), ImportError> {
        let track = match self.tracks.next() {
            Some(track) if track.edit.file == *unit => track,
            Some(track) => {
                return Err(ImportError::Internal {
                    detail: format!(
                        "draft track {} plays {:?} where the folder's next audio is {unit:?}",
                        track.edit.id, track.edit.file
                    ),
                })
            }
            None => {
                return Err(ImportError::Internal {
                    detail: format!("the draft has no track for {unit:?}"),
                })
            }
        };
        let source_length = track
            .source_index
            .and_then(|index| self.source_lengths.get(index as usize).copied().flatten());
        let position = crate::util::format::compute_track_position(
            self.medium,
            track.edit.side,
            Some(track.edit.track_number),
            self.multi_side,
        );
        Ok((
            crate::util::format::track_side(&position),
            TrackMapping {
                source,
                track: track.edit.as_edit(),
                position: crate::util::format::track_position_text(&position),
                duration_ms: source_length.or(probed_duration_ms),
                lengths_disagree: crate::import::audio_layout::lengths_disagree(
                    probed_duration_ms,
                    source_length,
                ),
            },
        ))
    }
}

/// Append a loose row to the last run of loose rows on its side, or start one.
fn push_track_mapping(
    sections: &mut Vec<MappingTrackSection>,
    side: crate::album_detail::TrackSide,
    mapping: TrackMapping,
) {
    if let Some(MappingTrackSection {
        side: existing,
        content: MappingTrackSectionContent::Tracks(mappings),
    }) = sections.last_mut()
    {
        if *existing == side {
            mappings.push(mapping);
            return;
        }
    }
    sections.push(MappingTrackSection {
        side,
        content: MappingTrackSectionContent::Tracks(vec![mapping]),
    });
}

/// Append a sheet's entry under its sheet on its side, or start a group there:
/// a sheet the release splits across sides heads one group per side.
fn push_sheet_entry(
    sections: &mut Vec<MappingTrackSection>,
    sheet: &SheetGroup,
    side: crate::album_detail::TrackSide,
    mapping: TrackMapping,
) {
    if let Some(MappingTrackSection {
        side: existing,
        content:
            MappingTrackSectionContent::Sheet {
                sheet: group,
                entries,
            },
    }) = sections.last_mut()
    {
        if group.sheet_id == sheet.sheet_id && *existing == side {
            entries.push(mapping);
            return;
        }
    }
    sections.push(MappingTrackSection {
        side,
        content: MappingTrackSectionContent::Sheet {
            sheet: sheet.clone(),
            entries: vec![mapping],
        },
    });
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
        file: entry.file.version(),
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

/// What a sheet describes, as its header states it.
fn bound_of(files: &CategorizedFiles, sheet: TrackSheetFile<'_>) -> SheetBound {
    match sheet.binding {
        SheetBinding::Resolved { .. } => bound_sheet(
            &files
                .bound_sheet(sheet)
                .expect("a resolved binding names audio"),
        ),
        SheetBinding::Unresolved { files: associated } => SheetBound::Unresolved {
            requested: sheet
                .sheet
                .audio_file_references()
                .into_iter()
                .filter(|reference| {
                    !associated
                        .iter()
                        .any(|file| file.file_reference == *reference)
                })
                .map(str::to_string)
                .collect(),
        },
        SheetBinding::RefusedTiming { .. } => SheetBound::RefusedTiming,
        SheetBinding::RefusedCodec { codec } => SheetBound::RefusedCodec {
            codec: codec.clone(),
        },
    }
}

fn bound_sheet(sheet: &BoundTrackSheet<'_>) -> SheetBound {
    match sheet.audio_files.as_slice() {
        [(_, audio)] => SheetBound::Describes(container(audio)),
        [_, _, ..] => SheetBound::DescribesFiles {
            audio_file_count: u32::try_from(sheet.audio_files.len())
                .expect("sheet audio file count fits u32"),
        },
        [] => unreachable!("a bound sheet resolves at least one audio file"),
    }
}

/// The left half of a file's row: what the folder holds.
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
    }
}

/// The discs a sheet of this folder may be assigned to: never fewer than one
/// per track sheet the folder binds, so a folder holding three sheets can
/// always be told which sheet is which, and every disc a sheet already has.
fn disc_options(files: &CategorizedFiles) -> Vec<u32> {
    let bound = files.bound_sheets().len() as u32;
    let assigned = files.track_sheets().filter_map(|sheet| match sheet.disc {
        SheetDisc::Disc { number } => Some(number),
        SheetDisc::Ignored => None,
    });
    let mut options = (1..=bound.max(1)).collect::<BTreeSet<_>>();
    options.extend(assigned);
    options.into_iter().collect()
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
