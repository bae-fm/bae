//! What the release editor shows beside a library release's form: its
//! tracks on the table's sides and runs by the file each plays from, each
//! beside the
//! form row that edits it, the summary of its audio, and the files it
//! carries that no track plays from.

use bae_records::source_audio::{SourceAudioLayout, SourceAudioSummary};
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
    pub duration_ms: u64,
    /// How the track's place reads ("A1", "3"), which the editor shows and
    /// never edits.
    pub position_text: String,
}

#[cfg(desktop)]
impl ReleaseEditTrackContext {
    /// The file a CUE sheet carves this track's body from.
    fn sheet_source(&self) -> Option<&ReleaseEditTrackSource> {
        (self.source.layout == SourceAudioLayout::Cue).then_some(&self.source)
    }
}

/// One row of the editor's track table: the form row that edits a track,
/// the library's facts about the track, and how the track's own artists
/// stand to the library (`None` when it takes its album's or names none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackRow {
    pub edit: crate::RawTrackEdit,
    pub context: ReleaseEditTrackContext,
    pub artist_standing: Option<crate::ArtistFieldStanding>,
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

/// Consecutive rows of one side that the table draws together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseEditTrackRun {
    /// A track whose row names its own source.
    Track(ReleaseEditTrackRow),
    /// Two or more consecutive tracks a CUE sheet carves from one file, which
    /// is named once above them.
    Sheet { source: ReleaseEditTrackSource, rows: Vec<ReleaseEditTrackRow> },
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
    /// side, and within it a run per file a sheet carves several consecutive
    /// tracks from; every other track is a run of its own.
    #[cfg(desktop)]
    pub(crate) fn lay_out(rows: impl IntoIterator<Item = ReleaseEditTrackRow>) -> Vec<Self> {
        let mut sides: Vec<Self> = Vec::new();
        for row in rows {
            match sides.last_mut() {
                Some(last) if last.side == row.context.side => last.append(row),
                _ => sides.push(Self {
                    side: row.context.side.clone(),
                    runs: vec![ReleaseEditTrackRun::Track(row)],
                }),
            }
        }
        sides
    }

    /// Add the next row: onto the last run when a sheet carves both tracks
    /// from the same file, else as a run of its own.
    #[cfg(desktop)]
    fn append(&mut self, row: ReleaseEditTrackRow) {
        let carved_with = |source: &ReleaseEditTrackSource| {
            row.context.sheet_source().is_some_and(|next| next.file_id == source.file_id)
        };
        match self.runs.pop() {
            Some(ReleaseEditTrackRun::Sheet { source, mut rows }) if carved_with(&source) => {
                rows.push(row);
                self.runs.push(ReleaseEditTrackRun::Sheet { source, rows });
            }
            Some(ReleaseEditTrackRun::Track(last))
                if last.context.sheet_source().is_some_and(carved_with) =>
            {
                let source = last
                    .context
                    .sheet_source()
                    .expect("the guard read the track's sheet source")
                    .clone();
                self.runs.push(ReleaseEditTrackRun::Sheet { source, rows: vec![last, row] });
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
            duration_ms: entry.track.duration_ms,
            position_text: entry.position.text(),
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
        audio_format: file.source_audio.as_ref().map(|audio| audio.format.clone()),
        role: match file.role {
            ReleaseFileRole::Audio => MappingRole::Audio,
            // The library keeps no sheet's binding: a sheet is the text it is.
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
