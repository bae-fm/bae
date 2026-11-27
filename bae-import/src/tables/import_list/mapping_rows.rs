//! The pane's mapping table, read in one statement.
//!
//! The scan stores the folder's audio layout with its files (the units in play
//! order, each one's length and preview range, and what each track sheet's
//! `FILE` lines can be bound to), and the draft stores each track's place. The
//! statement joins each draft track to its audio unit and returns a row per
//! draft track, the rows grouped into sides and runs, each sheet's header, the
//! files that offer no audio unit, the audio no draft track reads, the gallery
//! of images, and the count of rows nobody has named. The code here only turns that document into the
//! mapping table's types and refuses a draft whose tracks are not units of
//! the folder in its order.

use super::*;
use crate::folder_scanner::{SheetBindingOffer, SheetReferenceOptions};
use crate::mapping::{MappingFile, MappingFileRow, MappingImage, MappingRole, MappingTable};
use crate::{
    ImportError, MappingContainer, SheetBound, SheetBoundAudio, SheetGroup, TrackAudition,
    TrackSource, TrackTableRow, TrackTableRun, TrackTableSide,
};

#[derive(serde::Deserialize)]
struct AudioJson {
    file_id: String,
    name: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
}

#[derive(serde::Deserialize)]
struct RowJson {
    /// The draft track the row maps.
    track: crate::tables::import_state::StoredTrack,
    /// The track's own artists, each with how it stands to the library, as
    /// the standing clauses wrote them; `None` when it has none of its own.
    artists: Option<super::ArtistFieldJson>,
    /// The track's number on its side.
    number: u32,
    /// The length the track's source lists. The statement sets it only when
    /// `bae_lengths_disagree` finds that it differs too much from the audio's
    /// own length.
    listed_ms: Option<u64>,
    /// The folder's audio unit the track plays; `None` when the folder has no
    /// such unit.
    unit: Option<UnitJson>,
}

/// One audio unit of the folder: the audio file it plays from, the length
/// it measures, and what part of the file it is.
#[derive(serde::Deserialize)]
struct UnitJson {
    /// The unit's place among the folder's units.
    position: u32,
    audio: Option<AudioJson>,
    length_ms: u64,
    source: UnitSourceJson,
}

#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UnitSourceJson {
    /// The whole file.
    Standalone,
    /// One entry a track sheet carves out of the file.
    SheetSlice {
        /// Where the entry starts in its audio, in milliseconds, from the
        /// sheet's INDEX 01.
        start_ms: u64,
        /// The sample the entry starts at in its audio.
        start: u64,
        end: Option<u64>,
    },
}

#[derive(serde::Deserialize)]
struct ReferenceJson {
    file_reference: String,
    file_id: Option<String>,
    options: Vec<OptionJson>,
}

/// A sheet's FILE lines that play audio, tagged by how many there are.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BoundAudioJson {
    None,
    One { reference: ReferenceJson },
    Several { references: Vec<ReferenceJson> },
}

#[derive(serde::Deserialize)]
struct OptionJson {
    file_id: String,
    offer: String,
    codec: Option<String>,
}

#[derive(serde::Deserialize)]
struct SheetJson {
    sheet_id: String,
    name: String,
    bound: BoundJson,
    place: String,
    assigned: Option<bae_records::DbSheetSideColumns>,
    bound_audio: BoundAudioJson,
    unbound_references: Option<Vec<ReferenceJson>>,
    side_choices: Vec<bae_records::DbSheetSideColumns>,
}

/// What a track sheet is bound to, tagged by the binding the statement
/// reads from the scan and from how many audio files the sheet is bound to.
#[derive(serde::Deserialize)]
#[serde(tag = "binding", rename_all = "snake_case")]
enum BoundJson {
    /// Resolved, and bound to no audio file, which [`sheet_group`] refuses.
    ResolvedToNone,
    /// Resolved, and bound to this one audio file.
    Describes {
        audio: AudioJson,
    },
    /// Resolved, and bound to this many audio files.
    DescribesFiles {
        audio_count: u32,
    },
    /// Bound to none, with the files its `FILE` lines ask for and the
    /// folder lacks.
    Unresolved {
        requested: Vec<String>,
    },
    RefusedTiming,
    RefusedCodec {
        codec: String,
    },
}

#[derive(serde::Deserialize)]
struct RunJson {
    sheet: Option<SheetJson>,
    rows: Vec<RowJson>,
}

#[derive(serde::Deserialize)]
struct SideJson {
    /// The side's kind and ordinal, as the `TrackPlaces` fragment names them.
    kind: String,
    ordinal: u32,
    runs: Vec<RunJson>,
}

#[derive(serde::Deserialize)]
struct ImageJson {
    file_id: String,
    name: String,
    #[serde(deserialize_with = "path_column::from_hex")]
    path: std::path::PathBuf,
    size: u64,
    modified_at_ns: i64,
}

#[derive(serde::Deserialize)]
struct CarriedFileJson {
    file_id: String,
    name: String,
    size: u64,
    role: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum CarriedJson {
    File(CarriedFileJson),
    Sheet(SheetJson),
}

/// The statement's document, decoded: the statement's rows, before `table`
/// joins the draft's tracks to them.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MappingRows {
    document: String,
}

#[derive(serde::Deserialize)]
struct Document {
    /// The folder's images; `None` when it holds none.
    images: Option<Vec<ImageJson>>,
    /// The track table's sides; `None` when the folder offers no track.
    sides: Option<Vec<SideJson>>,
    /// The files that offer no audio unit, and the CUE sheets that carve no
    /// track of the draft; `None` when there are none.
    files: Option<Vec<CarriedJson>>,
    /// `None` when the draft's tracks read all the folder's audio.
    left_out: Option<crate::LeftOutAudio>,
    untitled: u32,
    tracks: u32,
    /// How long the draft's tracks play together, in milliseconds.
    length_ms: u64,
}

/// The mapping statement's document, read: the mapping table, and how long
/// its tracks play together.
pub(super) struct MappingRead {
    pub(super) table: MappingTable,
    /// The sum of the tracks' lengths, in milliseconds.
    pub(super) track_length_ms: u64,
    pub(super) left_out_audio: Option<crate::LeftOutAudio>,
}

impl MappingRows {
    /// The mapping table's rows as the statement wrote them in one JSON
    /// document.
    pub(super) fn of_document(document: String) -> Self {
        Self { document }
    }

    /// The mapping table over the draft's tracks, each marked by whether
    /// the preview player holds `previewed`, its stretch of audio. Refuses a
    /// draft whose tracks are not units of the folder's audio in its order:
    /// every write that changes the folder's files rebuilds the draft, so a
    /// mismatch is a
    /// stored state that contradicts itself.
    pub(super) fn read(
        &self,
        has_album_artists: bool,
        previewed: Option<&bae_track_audio::PreviewTarget>,
    ) -> Result<MappingRead, ImportError> {
        let internal = |detail: String| ImportError::Internal { detail };
        let document: Document = serde_json::from_str(&self.document)
            .map_err(|error| internal(format!("the mapping statement's document: {error}")))?;
        let mut previous_unit = None;
        let track_sides = document
            .sides
            .map(|sides| {
                sides
                    .into_iter()
                    .map(|side| track_side(side, has_album_artists, previewed, &mut previous_unit))
                    .collect()
            })
            .transpose()?;
        let files = document
            .files
            .map(|files| files.into_iter().map(carried_row).collect::<Result<Vec<_>, _>>())
            .transpose()?;
        let table = MappingTable {
            images: document.images.map(|images| {
                images
                    .into_iter()
                    .map(|image| MappingImage {
                        file_id: image.file_id,
                        name: image.name,
                        file: crate::folder_scanner::FileVersion {
                            path: image.path,
                            size: image.size,
                            modified_at_ns: image.modified_at_ns,
                        },
                    })
                    .collect()
            }),
            track_sides,
            files,
            track_count: document.tracks,
            untitled_track_count: document.untitled,
        };
        Ok(MappingRead {
            table,
            track_length_ms: document.length_ms,
            left_out_audio: document.left_out,
        })
    }
}

/// One side of the track table, its runs of loose tracks and of the tracks
/// a sheet carves, each row at its place on the side. `previous_unit` is the
/// unit the row before this side's first plays.
fn track_side(
    side: SideJson,
    has_album_artists: bool,
    previewed: Option<&bae_track_audio::PreviewTarget>,
    previous_unit: &mut Option<u32>,
) -> Result<TrackTableSide, ImportError> {
    let unplaced =
        |reason: String| ImportError::Internal { detail: format!("a track side: {reason}") };
    let mut runs = Vec::new();
    for run in side.runs {
        let mut rows = Vec::new();
        for row in run.rows {
            let position =
                bae_records::records::track_position_of(&side.kind, side.ordinal, row.number)
                    .map_err(unplaced)?;
            rows.push(table_row(
                row,
                position.text(),
                has_album_artists,
                previewed,
                previous_unit,
            )?);
        }
        match run.sheet {
            Some(sheet) => runs.push(TrackTableRun::Sheet { sheet: sheet_group(sheet)?, rows }),
            None => runs.extend(rows.into_iter().map(TrackTableRun::Track)),
        }
    }
    Ok(TrackTableSide {
        side: bae_records::records::track_side_of(&side.kind, side.ordinal).map_err(unplaced)?,
        runs,
    })
}

/// One file no track plays from, or a sheet that carves no track.
fn carried_row(carried: CarriedJson) -> Result<MappingFileRow, ImportError> {
    Ok(match carried {
        CarriedJson::Sheet(sheet) => MappingFileRow::Sheet(sheet_group(sheet)?),
        CarriedJson::File(file) => MappingFileRow::File(MappingFile {
            file_id: file.file_id,
            name: file.name,
            size: file.size,
            audition: None,
            role: carried_role(&file.role)?,
        }),
    })
}

fn carried_role(role: &str) -> Result<MappingRole, ImportError> {
    match role {
        "audio" => Ok(MappingRole::Audio),
        "artwork" => Ok(MappingRole::Image),
        "document" => Ok(MappingRole::Document),
        "damaged_image" => Ok(MappingRole::DamagedImage),
        "other" => Ok(MappingRole::Other),
        other => Err(ImportError::Internal {
            detail: format!(
                "the statement returned an unknown role {other} for a file no track plays from"
            ),
        }),
    }
}

fn container(audio: &AudioJson) -> MappingContainer {
    MappingContainer { file_id: audio.file_id.clone(), name: audio.name.clone() }
}

fn sheet_group(sheet: SheetJson) -> Result<SheetGroup, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let bound = match sheet.bound {
        BoundJson::Describes { audio } => SheetBound::Describes(container(&audio)),
        BoundJson::DescribesFiles { audio_count } => {
            SheetBound::DescribesFiles { audio_file_count: audio_count }
        }
        BoundJson::ResolvedToNone => {
            return Err(internal(format!("{} is bound to no audio", sheet.sheet_id)))
        }
        BoundJson::Unresolved { requested } => SheetBound::Unresolved { requested },
        BoundJson::RefusedTiming => SheetBound::RefusedTiming,
        BoundJson::RefusedCodec { codec } => SheetBound::RefusedCodec { codec },
    };
    let assigned = match (sheet.place.as_str(), sheet.assigned) {
        // A sheet on a side whose tracks the draft leaves out is on none of
        // the draft's.
        ("side", choice) => choice.map(|choice| choice.decoded().map_err(internal)).transpose()?,
        ("ignored", None) => None,
        (place, assigned) => {
            return Err(internal(format!(
                "track sheet {} is {place} with the choice {}",
                sheet.sheet_id,
                if assigned.is_some() { "of a side" } else { "of none" },
            )))
        }
    };
    let side_choices = sheet
        .side_choices
        .into_iter()
        .map(|choice| choice.decoded().map_err(internal))
        .collect::<Result<_, _>>()?;
    let bound_audio = match sheet.bound_audio {
        BoundAudioJson::None => SheetBoundAudio::None,
        BoundAudioJson::One { reference } => SheetBoundAudio::One(reference_options(reference)?),
        BoundAudioJson::Several { references } => SheetBoundAudio::Several(
            references.into_iter().map(reference_options).collect::<Result<_, _>>()?,
        ),
    };
    let unbound_references = sheet
        .unbound_references
        .map(|references| references.into_iter().map(reference_options).collect())
        .transpose()?;
    Ok(SheetGroup {
        sheet_id: sheet.sheet_id,
        name: sheet.name,
        bound,
        bound_audio,
        unbound_references,
        assigned,
        side_choices,
    })
}

/// One FILE line of a sheet, with the audio it plays and the choices the
/// caption offers for it.
fn reference_options(reference: ReferenceJson) -> Result<SheetReferenceOptions, ImportError> {
    Ok(SheetReferenceOptions {
        file_reference: reference.file_reference,
        // A folder's audio is named by its relative path, its id.
        bound: reference.file_id.map(|file_id| MappingContainer { name: file_id.clone(), file_id }),
        options: reference
            .options
            .into_iter()
            .map(|option| {
                Ok(crate::folder_scanner::SheetBindingOption {
                    name: option.file_id.clone(),
                    file_id: option.file_id,
                    offer: match (option.offer.as_str(), option.codec) {
                        ("offered", None) => SheetBindingOffer::Offered,
                        ("refused_timing", None) => SheetBindingOffer::RefusedTiming,
                        ("refused_codec", Some(codec)) => SheetBindingOffer::RefusedCodec { codec },
                        ("refused_unreadable", None) => SheetBindingOffer::RefusedUnreadable,
                        (other, _) => {
                            return Err(ImportError::Internal {
                                detail: format!(
                                    "the statement returned an unknown sheet offer {other}"
                                ),
                            })
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>, ImportError>>()?,
    })
}

fn table_row(
    row: RowJson,
    position: String,
    has_album_artists: bool,
    previewed: Option<&bae_track_audio::PreviewTarget>,
    previous_unit: &mut Option<u32>,
) -> Result<TrackTableRow, ImportError> {
    let internal = |detail: String| ImportError::Internal { detail };
    let track = row.track.track()?;
    let unit = row.unit.ok_or_else(|| {
        internal(format!(
            "draft track {} plays {:?}, which is no audio unit of the folder",
            track.edit.id, track.edit.file
        ))
    })?;
    if Some(unit.position) <= *previous_unit {
        return Err(internal(format!(
            "the draft's tracks are not the folder's audio units in order: one plays unit {} \
             after {previous_unit:?}",
            unit.position
        )));
    }
    *previous_unit = Some(unit.position);
    let audio = unit.audio.ok_or_else(|| internal("an audio unit names no file".to_owned()))?;
    let (source, audition) = match unit.source {
        UnitSourceJson::Standalone => (
            TrackSource::File { name: audio.file_id },
            bae_track_audio::PreviewTarget::whole_file(audio.path),
        ),
        UnitSourceJson::SheetSlice { start_ms, start, end } => (
            TrackSource::SheetSlice { start_ms },
            bae_track_audio::PreviewTarget::sample_range(audio.path, start, end),
        ),
    };
    let track = track.edit.as_edit();
    Ok(TrackTableRow {
        artist: crate::TrackArtistDisplay::of(
            &track,
            row.artists.map(super::ArtistFieldJson::field).transpose()?,
            has_album_artists,
        )
        .map_err(ImportError::Database)?,
        track,
        source,
        audition: TrackAudition::Preview(crate::FileAudition::of(audition, previewed)),
        position,
        length_ms: unit.length_ms,
        listed_length_ms: row.listed_ms,
    })
}

#[cfg(test)]
impl crate::tables::ImportTables {
    /// The audio the pane of the candidate at `key` says an import leaves out.
    pub(crate) async fn load_left_out_audio_for_test(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<Option<crate::LeftOutAudio>, ImportError> {
        let (mapping, album_artists) = self.load_pane_mapping(key).await?;
        mapping.read(album_artists.is_some(), None).map(|read| read.left_out_audio)
    }
}

impl crate::tables::ImportTables {
    /// The mapping table the pane of the candidate at `key` shows.
    pub(crate) async fn load_mapping_table(
        &self,
        key: &crate::CandidateKey,
    ) -> Result<MappingTable, ImportError> {
        let (mapping, album_artists) = self.load_pane_mapping(key).await?;
        mapping.read(album_artists.is_some(), None).map(|read| read.table)
    }
}
