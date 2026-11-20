//! What the release editor shows beside a library release's form: its
//! tracks on the table's sides and runs by the file each plays from, each
//! beside the
//! form row that edits it, the summary of its audio, and the files it
//! carries that no track plays from.

use bae_records::source_audio::{SourceAudioLayout, SourceAudioSummary};
use bae_records::DbReleaseEditSheet;
use bae_types::mediums::TrackSide;

/// The file a persisted track's body plays from: its main segment's file. A
/// pregap stored in the file before is not the track's source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseEditTrackSource {
    pub file_id: String,
    pub name: String,
    pub layout: SourceAudioLayout,
}

/// Display-only facts that accompany one persisted metadata row in the shared
/// editor. `track_id` is the database row identity, not a positional join.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ReleaseEditTrackContext {
    pub track_id: String,
    /// The side of the release the track is on.
    pub side: TrackSide,
    pub source: ReleaseEditTrackSource,
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

#[cfg(desktop)]
impl ReleaseEditTrackContext {
    /// The CUE sheet that carves this track's body, and the file it carves
    /// the body from.
    fn carved_from(&self) -> Option<(&DbReleaseEditSheet, &ReleaseEditTrackSource)> {
        self.sheet.as_ref().map(|sheet| (sheet, &self.source))
    }
}

/// One row of the editor's track table: the form row that edits a track,
/// the library's facts about the track, and the track's own artists, each
/// with how it stands to the library (`None` when it takes its album's or
/// names none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackRow {
    pub edit: crate::RawTrackEdit,
    pub context: ReleaseEditTrackContext,
    pub artists: Option<crate::ArtistField>,
}

/// One side of the release as the editor's track table lays it out (a disc
/// of a multi-disc set, a side of a record, or the one flat side) and the
/// runs of rows on it, in table order. The same shape the import mapping
/// table draws, so a release reads the same before and after it is imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackSide {
    pub side: TrackSide,
    pub runs: Vec<ReleaseEditTrackRun>,
}

/// The CUE sheet over a run of the editor's tracks, as its caption names it:
/// the sheet's file id and name, the side of the release the editor's form
/// puts its first track on, with its heading, which the place pop-up names,
/// and the sides and discs the release holds, which the pop-up offers, and the text of its `FILE` line with the audio files
/// the line can play from, which its Audio file pop-up offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditSheet {
    pub file_id: String,
    pub name: String,
    pub place: bae_records::DbSheetSideChoice,
    /// The sides and discs of the release, in order.
    pub side_choices: Vec<bae_records::DbSheetSideChoice>,
    pub file_reference: String,
    pub audio_options: Vec<ReleaseEditAudioOption>,
}

/// One audio file of the release as a choice for a sheet's `FILE` line:
/// the file, whether the line plays from it in the editor's form, and
/// whether it can back the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditAudioOption {
    pub file_id: String,
    pub name: String,
    pub bound: bool,
    pub offer: crate::folder_scanner::SheetBindingOffer,
}

#[cfg(desktop)]
impl ReleaseEditSheet {
    /// The caption of the library's sheet `sheet` on a release whose sides
    /// and discs are `side_choices`: a file whose codec bae can't carve
    /// tracks out of is refused naming the codec, and one shorter than the
    /// line's tracks is refused for its timing, as the import pane refuses
    /// them.
    fn of(sheet: DbReleaseEditSheet, side_choices: &[bae_records::DbSheetSideChoice]) -> Self {
        let DbReleaseEditSheet { file_id, name, place, file_reference, audio_options } = sheet;
        Self {
            file_id,
            name,
            place,
            side_choices: side_choices.to_vec(),
            file_reference,
            audio_options: audio_options
                .into_iter()
                .map(|option| ReleaseEditAudioOption {
                    offer: crate::folder_scanner::SheetBindingOffer::of_probed(
                        &option.content_type,
                        option.fits,
                    ),
                    file_id: option.file_id,
                    name: option.name,
                    bound: option.bound,
                })
                .collect(),
        }
    }
}

/// Consecutive rows of one side that the table draws together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseEditTrackRun {
    /// A track whose row names its own source.
    Track(ReleaseEditTrackRow),
    /// Two or more consecutive tracks a CUE sheet carves from one file, which
    /// is named once above them with the sheet.
    Sheet {
        sheet: ReleaseEditSheet,
        source: ReleaseEditTrackSource,
        rows: Vec<ReleaseEditTrackRow>,
    },
}

impl ReleaseEditTrackRun {
    pub fn rows(&self) -> &[ReleaseEditTrackRow] {
        match self {
            Self::Track(row) => std::slice::from_ref(row),
            Self::Sheet { rows, .. } => rows,
        }
    }
}

impl ReleaseEditTrackSide {
    /// Lay out `rows`, in release order: a side per run of tracks on one
    /// side, and within it a run per `FILE` line of a sheet that carves
    /// several consecutive tracks from one file, whose caption offers the
    /// sides and discs `side_choices`; every other track is a run of its own.
    #[cfg(desktop)]
    pub(crate) fn lay_out(
        rows: impl IntoIterator<Item = ReleaseEditTrackRow>,
        side_choices: &[bae_records::DbSheetSideChoice],
    ) -> Vec<Self> {
        let mut sides: Vec<Self> = Vec::new();
        for row in rows {
            match sides.last_mut() {
                Some(last) if last.side == row.context.side => last.append(row, side_choices),
                _ => sides.push(Self {
                    side: row.context.side.clone(),
                    runs: vec![ReleaseEditTrackRun::Track(row)],
                }),
            }
        }
        sides
    }

    /// Add the next row: onto the last run when one sheet carves both tracks
    /// from under the same `FILE` line and from the same file, else as a run
    /// of its own.
    #[cfg(desktop)]
    fn append(
        &mut self,
        row: ReleaseEditTrackRow,
        side_choices: &[bae_records::DbSheetSideChoice],
    ) {
        let line = |sheet_file_id: &str, file_reference: &str, source: &ReleaseEditTrackSource| {
            row.context.carved_from().is_some_and(|(next_sheet, next_source)| {
                next_sheet.file_id == sheet_file_id
                    && next_sheet.file_reference == file_reference
                    && next_source.file_id == source.file_id
            })
        };
        match self.runs.pop() {
            Some(ReleaseEditTrackRun::Sheet { sheet, source, mut rows })
                if line(&sheet.file_id, &sheet.file_reference, &source) =>
            {
                rows.push(row);
                self.runs.push(ReleaseEditTrackRun::Sheet { sheet, source, rows });
            }
            Some(ReleaseEditTrackRun::Track(last))
                if last.context.carved_from().is_some_and(|(sheet, source)| {
                    line(&sheet.file_id, &sheet.file_reference, source)
                }) =>
            {
                let (sheet, source) =
                    last.context.carved_from().expect("the guard read the track's sheet");
                let (sheet, source) =
                    (ReleaseEditSheet::of(sheet.clone(), side_choices), source.clone());
                self.runs.push(ReleaseEditTrackRun::Sheet { sheet, source, rows: vec![last, row] });
            }
            last => {
                self.runs.extend(last);
                self.runs.push(ReleaseEditTrackRun::Track(row));
            }
        }
    }

    pub fn rows(&self) -> impl Iterator<Item = &ReleaseEditTrackRow> {
        self.runs.iter().flat_map(ReleaseEditTrackRun::rows)
    }
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
                return Err(ReleaseEditLoadError::MissingMainSegment { track_id })
            }
            _ => return Err(ReleaseEditLoadError::MultipleMainSegments { track_id }),
        };
        let file = main.file.ok_or_else(|| ReleaseEditLoadError::MissingSourceFile {
            track_id: track_id.clone(),
            file_id: main.file_id.clone(),
        })?;
        let layout = file.layout.ok_or_else(|| ReleaseEditLoadError::MissingSourceAudio {
            track_id: track_id.clone(),
            file_id: main.file_id.clone(),
        })?;
        Ok(Self {
            track_id,
            side: entry.position.side(),
            source: ReleaseEditTrackSource { file_id: main.file_id, name: file.name, layout },
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
        tracks: Vec<bae_records::DbReleaseEditTrack>,
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
fn carried_file(file: &bae_records::DbFile) -> crate::MappingFile {
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
