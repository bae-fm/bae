//! What the release editor shows beside a library release's form: its
//! tracks laid out on the track table both hosts draw, each row the form
//! row that edits the track, the summary of its audio, and the files it
//! carries that no track plays from.

use bae_records::source_audio::SourceAudioSummary;
use bae_records::DbReleaseEditSheet;
use bae_types::mediums::TrackSide;

/// Display-only facts that accompany one persisted metadata row in the shared
/// editor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseEditTrackContext {
    /// The side of the release the track is on.
    pub side: TrackSide,
    /// The file the track's body plays from: its main segment's file. A
    /// pregap stored in the file before is not the track's source.
    pub source: crate::MappingContainer,
    /// Where the track's body starts in its source file, in milliseconds:
    /// for a track a CUE sheet carves from an image, its INDEX 01.
    pub start_ms: u64,
    pub duration_ms: u64,
    /// The length the track's source lists, kept only when it is more than
    /// [`bae_catalogs::medium_coverage::LENGTH_DISAGREEMENT_MS`] from
    /// `duration_ms`, as the import's mapping table keeps it.
    pub listed_length_ms: Option<u64>,
    /// How the track's place reads ("A1", "3"), which the editor shows and
    /// never edits.
    pub position_text: String,
    /// The CUE sheet that carves the track out of its source file; `None`
    /// for a track that plays a whole file, or whose sheet the release no
    /// longer holds.
    pub sheet: Option<DbReleaseEditSheet>,
}

/// One row of the editor's track table: the form row that edits a track,
/// the library's facts about the track, and the track's own artists, each
/// with how it stands to the library (`None` when it takes its album's or
/// names none).
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseEditTrackRow {
    pub edit: crate::RawTrackEdit,
    pub context: ReleaseEditTrackContext,
    /// What playing the row plays: the library's track, or nothing for a
    /// track a save adds.
    pub audition: crate::TrackAudition,
    pub artists: Option<crate::ArtistField>,
}

#[cfg(desktop)]
impl ReleaseEditTrackRow {
    /// The table's row of this track, naming `source` in its source cell,
    /// which plays the library's release from it.
    fn table_row(self, source: crate::TrackSource) -> crate::TrackTableRow {
        crate::TrackTableRow {
            track: self.edit,
            source,
            audition: self.audition,
            position: self.context.position_text,
            length_ms: self.context.duration_ms,
            listed_length_ms: self.context.listed_length_ms,
            artists: self.artists,
        }
    }
}

/// One entry of the editor's track table in release order: a track, or a
/// side or disc no track sits on, which keeps its heading until Save.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReleaseEditTableRow {
    Track(ReleaseEditTrackRow),
    EmptySide(bae_types::mediums::TrackSide),
}

/// Lay out `entries`, in release order, as the import pane lays out a
/// folder's tracks: a side per run of tracks on one side, and within it a
/// run per CUE sheet that carves consecutive tracks there, under the
/// sheet's caption. Each other track is a run of its own. A side no track
/// sits on has no run.
#[cfg(desktop)]
pub(crate) fn lay_out_tracks(entries: Vec<ReleaseEditTableRow>) -> Vec<crate::TrackTableSide> {
    let rows: Vec<&ReleaseEditTrackRow> = entries
        .iter()
        .filter_map(|entry| match entry {
            ReleaseEditTableRow::Track(row) => Some(row),
            ReleaseEditTableRow::EmptySide(_) => None,
        })
        .collect();
    let captions = sheet_captions(&rows);
    let mut sides: Vec<crate::TrackTableSide> = Vec::new();
    for entry in entries {
        let row = match entry {
            ReleaseEditTableRow::Track(row) => row,
            ReleaseEditTableRow::EmptySide(side) => {
                sides.push(crate::TrackTableSide { side, runs: Vec::new() });
                continue;
            }
        };
        if sides.last().is_none_or(|last| last.side != row.context.side) {
            sides.push(crate::TrackTableSide { side: row.context.side.clone(), runs: Vec::new() });
        }
        let side = sides.last_mut().expect("the row's side is the last one");
        let Some(sheet_id) = row.context.sheet.as_ref().map(|sheet| sheet.file_id.clone()) else {
            let name = row.context.source.name.clone();
            side.runs.push(crate::TrackTableRun::Track(
                row.table_row(crate::TrackSource::File { name }),
            ));
            continue;
        };
        let start_ms = row.context.start_ms;
        let row = row.table_row(crate::TrackSource::SheetSlice { start_ms });
        match side.runs.last_mut() {
            Some(crate::TrackTableRun::Sheet { sheet, rows }) if sheet.sheet_id == sheet_id => {
                rows.push(row)
            }
            _ => side.runs.push(crate::TrackTableRun::Sheet {
                sheet: captions
                    .iter()
                    .find(|caption| caption.sheet_id == sheet_id)
                    .expect("every sheet that carves a row has a caption")
                    .clone(),
                rows: vec![row],
            }),
        }
    }
    sides
}

/// The audio files a `FILE` line can play from, as the caption's Audio file
/// pop-up offers them.
#[cfg(desktop)]
fn reference_options(
    file_reference: String,
    bound: Option<crate::MappingContainer>,
    options: &[bae_records::DbSheetAudioOption],
) -> crate::folder_scanner::SheetReferenceOptions {
    use crate::folder_scanner::{SheetBindingOffer, SheetBindingOption, SheetReferenceOptions};
    let options = options
        .iter()
        .map(|option| SheetBindingOption {
            file_id: option.file_id.clone(),
            name: option.name.clone(),
            offer: SheetBindingOffer::of_probed(&option.content_type, option.fits),
        })
        .collect();
    SheetReferenceOptions { file_reference, bound, options }
}

/// The `bound` and `bound_audio` a caption names for a sheet whose lines
/// that play audio are `lines`, each with the file it plays from, and whose
/// lines that play none are `unbound`: a sheet with a line that plays none
/// names what it requests, as a sheet whose binding the person cleared does;
/// otherwise the one file, or how many files, `line_count` of them.
#[cfg(desktop)]
fn bound_lines(
    line_count: u32,
    mut lines: Vec<(crate::folder_scanner::SheetReferenceOptions, crate::MappingContainer)>,
    unbound: Option<&[bae_records::records::sheet_files::DbSheetLine]>,
) -> (crate::SheetBound, crate::SheetBoundAudio) {
    let audio = match line_count {
        0 => crate::SheetBoundAudio::None,
        1 if unbound.is_none() => {
            let (line, played) = lines.pop().expect("one line");
            return (crate::SheetBound::Describes(played), crate::SheetBoundAudio::One(line));
        }
        1 => crate::SheetBoundAudio::One(lines.pop().expect("one line").0),
        _ => crate::SheetBoundAudio::Several(lines.iter().map(|(line, _)| line.clone()).collect()),
    };
    let bound = match unbound {
        Some(unbound) => crate::SheetBound::Unresolved {
            requested: unbound.iter().map(|line| line.file_reference.clone()).collect(),
        },
        None => crate::SheetBound::DescribesFiles { audio_file_count: line_count },
    };
    (bound, audio)
}

/// The caption of each CUE sheet that carves one of `rows`: the side its
/// first track is on, the sides and discs its place pop-up
/// offers, and each of its `FILE` lines in the order the rows reach them,
/// with the file the line plays from and the audio files it can play from.
/// The sheet's count of lines, which its query counts, decides whether the
/// caption names the one file or how many.
/// A file whose codec bae can't carve tracks out of is refused naming the
/// codec, and one shorter than the line's tracks is refused for its timing,
/// as the import pane refuses them.
#[cfg(desktop)]
fn sheet_captions(rows: &[&ReleaseEditTrackRow]) -> Vec<crate::SheetGroup> {
    use crate::folder_scanner::SheetReferenceOptions;
    /// A sheet and its `FILE` lines so far, each with the file it plays
    /// from.
    type Lines<'a> =
        (&'a DbReleaseEditSheet, Vec<(SheetReferenceOptions, crate::MappingContainer)>);
    let mut sheets: Vec<Lines<'_>> = Vec::new();
    for row in rows {
        let Some(sheet) = &row.context.sheet else { continue };
        let at = match sheets.iter().position(|(seen, _)| seen.file_id == sheet.file_id) {
            Some(at) => at,
            None => {
                sheets.push((sheet, Vec::new()));
                sheets.len() - 1
            }
        };
        let lines = &mut sheets[at].1;
        if lines.iter().any(|(line, _)| line.file_reference == sheet.file_reference) {
            continue;
        }
        let played = row.context.source.clone();
        let line = reference_options(
            sheet.file_reference.clone(),
            Some(played.clone()),
            &sheet.audio_options,
        );
        lines.push((line, played));
    }
    sheets
        .into_iter()
        .map(|(sheet, lines)| {
            let (bound, bound_audio) =
                bound_lines(sheet.line_count, lines, sheet.unbound_lines.as_deref());
            crate::SheetGroup {
                sheet_id: sheet.file_id.clone(),
                name: sheet.name.clone(),
                bound,
                bound_audio,
                unbound_references: unbound_references(sheet.unbound_lines.as_deref()),
                assigned: Some(sheet.place.clone()),
                side_choices: sheet.side_choices.clone(),
            }
        })
        .collect()
}

/// The lines of a sheet that play no audio, each with the audio files it
/// can play from, as the caption's amber menus offer them; `None` where every
/// line plays audio.
#[cfg(desktop)]
fn unbound_references(
    lines: Option<&[bae_records::records::sheet_files::DbSheetLine]>,
) -> Option<Vec<crate::folder_scanner::SheetReferenceOptions>> {
    lines.map(|lines| {
        lines
            .iter()
            .map(|line| reference_options(line.file_reference.clone(), None, &line.options))
            .collect()
    })
}

/// The Files list's row of a CUE sheet none of whose tracks stay: the sheet's
/// caption with the form's bindings, on the original side if the form
/// still places the sheet.
#[cfg(desktop)]
pub(crate) fn sheet_without_tracks_group(
    sheet: bae_records::records::sheet_files::DbSheetWithoutTracks,
) -> Result<crate::SheetGroup, String> {
    let lines = sheet
        .bound
        .into_iter()
        .map(|line| {
            let option = line.options.iter().find(|option| option.bound).ok_or_else(|| {
                format!("bound line {} has no bound audio file", line.file_reference)
            })?;
            let played = crate::MappingContainer {
                file_id: option.file_id.clone(),
                name: option.name.clone(),
            };
            let reference =
                reference_options(line.file_reference, Some(played.clone()), &line.options);
            Ok((reference, played))
        })
        .collect::<Result<_, String>>()?;
    let (bound, bound_audio) = bound_lines(sheet.bound_count, lines, sheet.unbound.as_deref());
    Ok(crate::SheetGroup {
        sheet_id: sheet.file_id,
        name: sheet.name,
        bound,
        bound_audio,
        unbound_references: unbound_references(sheet.unbound.as_deref()),
        assigned: sheet.place,
        side_choices: sheet.side_choices,
    })
}

/// What the library holds about a release beside its form: the summary of
/// its audio, its tracks' audio and places, and the files no track plays
/// from.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseEditDisplayContext {
    pub source_audio: Option<SourceAudioSummary>,
    /// The release's tracks, in release order.
    pub tracks: Vec<ReleaseEditTrackContext>,
    /// The files the release carries that no track plays from, in the
    /// release's file order: what its Files section lists under the tracks.
    pub files: Vec<crate::MappingFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseEditLoadError {
    #[error("track '{track_id}' has no stored audio format")]
    MissingAudioFormat { track_id: String },
    #[error("track '{track_id}' has more than one stored audio format")]
    MultipleAudioFormats { track_id: String },
    #[error("track '{track_id}' has no stored main audio segment")]
    MissingMainSegment { track_id: String },
    #[error("track '{track_id}' has more than one stored main audio segment")]
    MultipleMainSegments { track_id: String },
    #[error("track '{track_id}' references missing source file '{file_id}'")]
    MissingSourceFile { track_id: String, file_id: String },
    #[error("track '{track_id}' source file '{file_id}' has no stored audio layout")]
    MissingSourceAudio { track_id: String, file_id: String },
    #[error("track '{track_id}' has no row in the editor's form")]
    MissingFormRow { track_id: String },
}

#[cfg(desktop)]
impl ReleaseEditTrackContext {
    /// The library's facts about the track `entry`, from the audio it
    /// records.
    pub(crate) fn of_track(
        entry: bae_records::DbReleaseEditTrack,
    ) -> Result<Self, ReleaseEditLoadError> {
        let track_id = entry.track.id.clone();
        match entry.audio_format_count {
            0 => return Err(ReleaseEditLoadError::MissingAudioFormat { track_id }),
            1 => {}
            _ => return Err(ReleaseEditLoadError::MultipleAudioFormats { track_id }),
        }
        let main = match (entry.main_segment_count, entry.main_segment) {
            (1, Some(main)) => main,
            (0, _) | (_, None) => {
                return Err(ReleaseEditLoadError::MissingMainSegment { track_id });
            }
            _ => return Err(ReleaseEditLoadError::MultipleMainSegments { track_id }),
        };
        let file = main.file.ok_or_else(|| ReleaseEditLoadError::MissingSourceFile {
            track_id: track_id.clone(),
            file_id: main.file_id.clone(),
        })?;
        if file.layout.is_none() {
            return Err(ReleaseEditLoadError::MissingSourceAudio {
                track_id,
                file_id: main.file_id,
            });
        }
        Ok(Self {
            side: entry.position.side(),
            source: crate::MappingContainer { file_id: main.file_id, name: file.name },
            start_ms: main.start_ms,
            duration_ms: entry.track.duration_ms,
            listed_length_ms: entry.listed_length_ms,
            position_text: entry.position.text(),
            sheet: entry.sheet,
        })
    }
}

#[cfg(desktop)]
impl ReleaseEditDisplayContext {
    /// The editor's display of a release with the files `files`, from the
    /// audio each entry of `tracks` records, in release order.
    pub fn from_raw(
        files: &bae_records::DbReleaseFiles,
        tracks: impl IntoIterator<Item = bae_records::DbReleaseEditTrack>,
    ) -> Result<Self, ReleaseEditLoadError> {
        Ok(Self {
            source_audio: files.source_audio.clone(),
            tracks: tracks
                .into_iter()
                .map(ReleaseEditTrackContext::of_track)
                .collect::<Result<_, _>>()?,
            files: Self::carried_files(files),
        })
    }

    /// The files of `files` that no track plays from, as the release's Files
    /// section lists them.
    pub(crate) fn carried_files(files: &bae_records::DbReleaseFiles) -> Vec<crate::MappingFile> {
        files.others.iter().map(carried_file).collect()
    }
}

/// A file of the release no track plays from, as its Files section lists it.
/// It shows no length and no audition: the release's tracks are what play.
#[cfg(desktop)]
pub(crate) fn carried_file(file: &bae_records::DbFile) -> crate::MappingFile {
    use crate::MappingRole;
    use bae_records::ReleaseFileRole;
    crate::MappingFile {
        file_id: file.id.clone(),
        name: file.original_filename.clone(),
        size: file.file_size,
        preview_target: None,
        role: match file.role {
            ReleaseFileRole::Audio => MappingRole::Audio,
            ReleaseFileRole::TrackSheet | ReleaseFileRole::Document(_) => MappingRole::Document,
            ReleaseFileRole::Artwork => MappingRole::Image,
            ReleaseFileRole::DamagedImage => MappingRole::DamagedImage,
            ReleaseFileRole::Other => MappingRole::Other,
        },
    }
}

#[cfg(test)]
#[path = "release_edit_display_tests.rs"]
mod tests;
