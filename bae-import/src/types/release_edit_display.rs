//! What the release editor shows beside a library release's form: its
//! tracks laid out by side and by the file each plays from, and the summary of
//! its audio.

use bae_records::source_audio::{SourceAudioLayout, SourceAudioSummary};
use bae_types::mediums::TrackSide;

/// The file a persisted track's body plays from: its main segment's file. A
/// pregap stored in the file before is not the track's source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackSource {
    pub file_id: String,
    pub name: String,
    pub layout: SourceAudioLayout,
}

/// Display-only facts that accompany one persisted metadata row in the shared
/// editor. `track_id` is the database row identity, not a positional join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackContext {
    pub track_id: String,
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

/// One side of the release as the editor's track table lays it out — a disc
/// of a multi-disc set, a side of a record, or the one flat side — and the
/// runs of tracks on it, in table order. The same shape the import mapping
/// table draws, so a release reads the same before and after it is imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditTrackSide {
    pub side: TrackSide,
    pub runs: Vec<ReleaseEditTrackRun>,
}

/// Consecutive tracks of one side that the table draws together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseEditTrackRun {
    /// A track whose row names its own source.
    Track(ReleaseEditTrackContext),
    /// Two or more consecutive tracks a CUE sheet carves from one file, which
    /// is named once above them.
    Sheet { source: ReleaseEditTrackSource, tracks: Vec<ReleaseEditTrackContext> },
}

impl ReleaseEditTrackRun {
    pub fn tracks(&self) -> &[ReleaseEditTrackContext] {
        match self {
            Self::Track(track) => std::slice::from_ref(track),
            Self::Sheet { tracks, .. } => tracks,
        }
    }

    #[cfg(desktop)]
    fn into_tracks(self) -> Vec<ReleaseEditTrackContext> {
        match self {
            Self::Track(track) => vec![track],
            Self::Sheet { tracks, .. } => tracks,
        }
    }
}

impl ReleaseEditTrackSide {
    /// Lay out tracks in release order: a side per run of tracks on one side,
    /// and within it a run per file a sheet carves several consecutive tracks
    /// from; every other track is a run of its own.
    #[cfg(desktop)]
    pub(crate) fn lay_out(
        tracks: impl IntoIterator<Item = (TrackSide, ReleaseEditTrackContext)>,
    ) -> Vec<Self> {
        let mut sides: Vec<Self> = Vec::new();
        for (side, track) in tracks {
            match sides.last_mut() {
                Some(last) if last.side == side => last.append(track),
                _ => sides.push(Self { side, runs: vec![ReleaseEditTrackRun::Track(track)] }),
            }
        }
        sides
    }

    /// Add the next track: onto the last run when a sheet carves both from
    /// the same file, else as a run of its own.
    #[cfg(desktop)]
    fn append(&mut self, track: ReleaseEditTrackContext) {
        let carved_with = |source: &ReleaseEditTrackSource| {
            track.sheet_source().is_some_and(|next| next.file_id == source.file_id)
        };
        match self.runs.pop() {
            Some(ReleaseEditTrackRun::Sheet { source, mut tracks }) if carved_with(&source) => {
                tracks.push(track);
                self.runs.push(ReleaseEditTrackRun::Sheet { source, tracks });
            }
            Some(ReleaseEditTrackRun::Track(last))
                if last.sheet_source().is_some_and(carved_with) =>
            {
                let source =
                    last.sheet_source().expect("the guard read the track's sheet source").clone();
                self.runs.push(ReleaseEditTrackRun::Sheet { source, tracks: vec![last, track] });
            }
            last => {
                self.runs.extend(last);
                self.runs.push(ReleaseEditTrackRun::Track(track));
            }
        }
    }

    pub fn tracks(&self) -> impl Iterator<Item = &ReleaseEditTrackContext> {
        self.runs.iter().flat_map(ReleaseEditTrackRun::tracks)
    }
}

/// Release-backed context needed by the shared metadata editor in addition to
/// its writable raw form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditDisplayContext {
    pub source_audio: Option<SourceAudioSummary>,
    /// The release's tracks, laid out as the table draws them.
    pub sides: Vec<ReleaseEditTrackSide>,
}

impl ReleaseEditDisplayContext {
    /// Every track, in release order.
    pub fn tracks(&self) -> impl Iterator<Item = &ReleaseEditTrackContext> {
        self.sides.iter().flat_map(ReleaseEditTrackSide::tracks)
    }

    /// The same tracks at the places `positions` gives them, in release
    /// order, laid out again: what an applied catalog reads them as.
    #[cfg(desktop)]
    pub fn placed(
        self,
        positions: impl IntoIterator<Item = bae_types::mediums::TrackPosition>,
    ) -> Self {
        let tracks = self
            .sides
            .into_iter()
            .flat_map(|side| side.runs)
            .flat_map(ReleaseEditTrackRun::into_tracks);
        Self {
            source_audio: self.source_audio,
            sides: ReleaseEditTrackSide::lay_out(tracks.zip(positions).map(|(track, position)| {
                (
                    position.side(),
                    ReleaseEditTrackContext { position_text: position.text(), ..track },
                )
            })),
        }
    }
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
}

impl ReleaseEditDisplayContext {
    /// The editor's display of the release `raw`, from its tracks' audio as
    /// `tracks` reads it in release order.
    #[cfg(desktop)]
    pub fn from_raw(
        raw: &bae_records::DbReleaseDetail,
        tracks: Vec<bae_records::DbReleaseEditTrack>,
    ) -> Result<Self, ReleaseEditLoadError> {
        let mut contexts = Vec::with_capacity(tracks.len());
        for entry in tracks {
            let track_id = entry.track.id.clone();
            match entry.audio_format_count {
                0 => return Err(ReleaseEditLoadError::MissingAudioFormat { track_id }),
                1 => {}
                _ => return Err(ReleaseEditLoadError::MultipleAudioFormats { track_id }),
            }
            let mut mains = entry
                .segments
                .into_iter()
                .filter(|segment| segment.role == bae_records::DbAudioSegmentRole::Main);
            let main = match (mains.next(), mains.next()) {
                (Some(main), None) => main,
                (None, _) => return Err(ReleaseEditLoadError::MissingMainSegment { track_id }),
                (Some(_), Some(_)) => {
                    return Err(ReleaseEditLoadError::MultipleMainSegments { track_id })
                }
            };
            let file = main.file.ok_or_else(|| ReleaseEditLoadError::MissingSourceFile {
                track_id: track_id.clone(),
                file_id: main.file_id.clone(),
            })?;
            let layout = file.layout.ok_or_else(|| ReleaseEditLoadError::MissingSourceAudio {
                track_id: track_id.clone(),
                file_id: main.file_id.clone(),
            })?;
            let source = ReleaseEditTrackSource { file_id: main.file_id, name: file.name, layout };
            let position = raw.mediums.position(&entry.track);
            contexts.push((
                position.side(),
                ReleaseEditTrackContext {
                    track_id,
                    source,
                    duration_ms: entry.track.duration_ms,
                    position_text: position.text(),
                },
            ));
        }

        Ok(Self {
            source_audio: SourceAudioSummary::from_files(
                raw.files.iter().filter_map(|file| file.source_audio.as_ref()?.summarized()),
            ),
            sides: ReleaseEditTrackSide::lay_out(contexts),
        })
    }
}

#[cfg(test)]
#[path = "release_edit_display_tests.rs"]
mod tests;
