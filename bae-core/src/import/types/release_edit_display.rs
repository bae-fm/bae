//! What the release editor shows beside a library release's form: its
//! tracks laid out by side and by the file each plays from, and the summary of
//! its audio.

use crate::db::source_audio::{SourceAudioLayout, SourceAudioSummary};
use bae_types::mediums::TrackSide;

/// One physical source file that supplies a persisted track. A track can span
/// several files; callers retain this order from the stored audio segments.
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
    pub sources: Vec<ReleaseEditTrackSource>,
    pub duration_ms: u64,
    /// How the track's place reads ("A1", "3"), which the editor shows and
    /// never edits.
    pub position_text: String,
}

#[cfg(desktop)]
impl ReleaseEditTrackContext {
    /// The file a CUE sheet carves this track from, when that one file is
    /// all it plays.
    fn sheet_source(&self) -> Option<&ReleaseEditTrackSource> {
        match self.sources.as_slice() {
            [source] if source.layout == SourceAudioLayout::Cue => Some(source),
            _ => None,
        }
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
    pub(crate) fn placed(
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
    #[error("track '{track_id}' has no stored audio segments")]
    MissingAudioSegments { track_id: String },
    #[error("track '{track_id}' references missing source file '{file_id}'")]
    MissingSourceFile { track_id: String, file_id: String },
    #[error("track '{track_id}' source file '{file_id}' has no stored audio layout")]
    MissingSourceAudio { track_id: String, file_id: String },
}

impl ReleaseEditDisplayContext {
    /// The editor's display of the release `raw`, from its tracks' audio as
    /// `tracks` reads it in release order.
    #[cfg(desktop)]
    pub(crate) fn from_raw(
        raw: &crate::db::DbReleaseDetail,
        tracks: Vec<crate::db::DbReleaseEditTrack>,
    ) -> Result<Self, ReleaseEditLoadError> {
        let mut contexts = Vec::with_capacity(tracks.len());
        for entry in tracks {
            let track_id = entry.track.id.clone();
            match entry.audio_format_count {
                0 => return Err(ReleaseEditLoadError::MissingAudioFormat { track_id }),
                1 => {}
                _ => return Err(ReleaseEditLoadError::MultipleAudioFormats { track_id }),
            }
            if entry.segments.is_empty() {
                return Err(ReleaseEditLoadError::MissingAudioSegments { track_id });
            }
            let mut sources: Vec<ReleaseEditTrackSource> = Vec::new();
            for segment in entry.segments {
                // A file a track plays in several segments is one source.
                if sources.iter().any(|source| source.file_id == segment.file_id) {
                    continue;
                }
                let file = segment.file.ok_or_else(|| ReleaseEditLoadError::MissingSourceFile {
                    track_id: track_id.clone(),
                    file_id: segment.file_id.clone(),
                })?;
                let layout =
                    file.layout.ok_or_else(|| ReleaseEditLoadError::MissingSourceAudio {
                        track_id: track_id.clone(),
                        file_id: segment.file_id.clone(),
                    })?;
                sources.push(ReleaseEditTrackSource {
                    file_id: segment.file_id,
                    name: file.name,
                    layout,
                });
            }
            let position = raw.mediums.position(&entry.track);
            contexts.push((
                position.side(),
                ReleaseEditTrackContext {
                    track_id,
                    sources,
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
mod tests {
    use super::*;
    use bae_types::mediums::TrackPosition;

    fn context(id: &str, sources: &[(&str, SourceAudioLayout)]) -> ReleaseEditTrackContext {
        ReleaseEditTrackContext {
            track_id: id.to_string(),
            sources: sources
                .iter()
                .map(|(file, layout)| ReleaseEditTrackSource {
                    file_id: file.to_string(),
                    name: format!("{file}.flac"),
                    layout: *layout,
                })
                .collect(),
            duration_ms: 1_000,
            position_text: String::new(),
        }
    }

    fn side(letter: &str) -> TrackSide {
        TrackSide::Sided { side_letter: letter.to_string() }
    }

    /// Each run as the track ids it holds, and the file a sheet run names.
    fn runs(side: &ReleaseEditTrackSide) -> Vec<(Option<&str>, Vec<&str>)> {
        side.runs
            .iter()
            .map(|run| {
                let source = match run {
                    ReleaseEditTrackRun::Track(_) => None,
                    ReleaseEditTrackRun::Sheet { source, .. } => Some(source.file_id.as_str()),
                };
                (source, run.tracks().iter().map(|track| track.track_id.as_str()).collect())
            })
            .collect()
    }

    #[test]
    fn tracks_lay_out_a_side_per_run_of_one_side_and_a_run_per_sheet_file() {
        use SourceAudioLayout::{Cue, File};
        let sides = ReleaseEditTrackSide::lay_out([
            (side("A"), context("a1", &[("image-a", Cue)])),
            (side("A"), context("a2", &[("image-a", Cue)])),
            (side("A"), context("a3", &[("image-a", Cue)])),
            (side("A"), context("a4", &[("loose", File)])),
            (side("B"), context("b1", &[("image-b", Cue)])),
            (side("B"), context("b2", &[("image-b", Cue)])),
            (side("B"), context("b3", &[("image-c", Cue)])),
            (side("B"), context("b4", &[("image-c", Cue)])),
            (side("B"), context("b5", &[("image-d", Cue)])),
            (side("B"), context("b6", &[("image-d", Cue), ("image-e", Cue)])),
        ]);

        assert_eq!(
            sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
            [&side("A"), &side("B")]
        );
        assert_eq!(
            runs(&sides[0]),
            [(Some("image-a"), vec!["a1", "a2", "a3"]), (None, vec!["a4"]),]
        );
        // Two sheets back to back are two runs; a file one track alone is
        // carved from stays in that track's row, and so does a track playing
        // two files.
        assert_eq!(
            runs(&sides[1]),
            [
                (Some("image-b"), vec!["b1", "b2"]),
                (Some("image-c"), vec!["b3", "b4"]),
                (None, vec!["b5"]),
                (None, vec!["b6"]),
            ]
        );
    }

    #[test]
    fn a_sheet_run_ends_at_its_side() {
        use SourceAudioLayout::Cue;
        let sides = ReleaseEditTrackSide::lay_out([
            (side("A"), context("a1", &[("image", Cue)])),
            (side("A"), context("a2", &[("image", Cue)])),
            (side("B"), context("b1", &[("image", Cue)])),
        ]);

        assert_eq!(runs(&sides[0]), [(Some("image"), vec!["a1", "a2"])]);
        assert_eq!(runs(&sides[1]), [(None, vec!["b1"])]);
    }

    #[test]
    fn placed_tracks_are_laid_out_again_at_their_new_places() {
        use SourceAudioLayout::File;
        let display = ReleaseEditDisplayContext {
            source_audio: None,
            sides: ReleaseEditTrackSide::lay_out(
                ["t1", "t2", "t3"].map(|id| (TrackSide::Flat, context(id, &[(id, File)]))),
            ),
        };

        let placed = display.placed([
            TrackPosition::Disc { disc: 1, number: 1 },
            TrackPosition::Disc { disc: 1, number: 2 },
            TrackPosition::Disc { disc: 2, number: 1 },
        ]);

        assert_eq!(
            placed.sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
            [&TrackSide::Disc { disc: 1 }, &TrackSide::Disc { disc: 2 }]
        );
        assert_eq!(
            placed
                .tracks()
                .map(|track| (track.track_id.as_str(), track.position_text.as_str()))
                .collect::<Vec<_>>(),
            [("t1", "1"), ("t2", "2"), ("t3", "1")]
        );
    }

    #[test]
    fn missing_segment_file_is_an_editor_load_error() {
        let release = crate::db::DbRelease::new_test("album-id", "release-id");
        let track = crate::db::DbTrack::new_test(&release.id, "track-id", "Track Title", 1);
        let raw = crate::db::DbReleaseDetail {
            mediums: crate::db::DbReleaseMediums(vec![crate::db::DbReleaseMedium::new_test(
                &release.id,
            )]),
            release,
            tracks: vec![crate::db::DbTrackWithArtists {
                track: track.clone(),
                artists: Vec::new(),
            }],
            files: Vec::new(),
            records: Vec::new(),
            unstored_audio: Vec::new(),
            pinned: false,
            remote: false,
            upload: None,
            download: None,
            transfer_action: None,
            cover_version: None,
        };
        let tracks = vec![crate::db::DbReleaseEditTrack {
            track,
            audio_format_count: 1,
            segments: vec![crate::db::DbReleaseEditSegment {
                file_id: "missing-file-id".to_string(),
                file: None,
            }],
        }];

        assert!(matches!(
            ReleaseEditDisplayContext::from_raw(&raw, tracks),
            Err(ReleaseEditLoadError::MissingSourceFile { file_id, .. })
                if file_id == "missing-file-id"
        ));
    }
}
