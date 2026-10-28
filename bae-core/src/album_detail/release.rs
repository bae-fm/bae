//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use super::*;
use crate::db::{DbReleaseDetail, DbReleaseSummary};

/// The per-release inputs `LibraryManager` reads from the DB, coven's cache and
/// this device's disk and threads into the release projections: whether a cloud
/// home exists at all (which gates `storage_actions`), what the disk has said of the originals the
/// library records this device holding, and the release's own cover reference.
/// Bundled so the constructors take one context rather than a pile of parameters.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveCtx<'a> {
    pub(crate) has_cloud_home: bool,
    pub(crate) originals: &'a OriginalsSeen,
    pub(crate) cover: Option<ImageRef>,
    pub(crate) transfer_action: Option<ReleaseStorageAction>,
    /// Whether the owning album is a compilation. Decides `TrackDetail::display_artist`
    /// — the one album-level fact the per-track artist decision needs.
    pub(crate) is_compilation: bool,
}

/// The slim projection a list view (Storage Manager, release pickers) renders one
/// row from. Every field is pre-computed; nothing downstream derives anything.
/// Composed into [`ReleaseDetail`] for detail views.
///
/// Invariant: `album_id` names an album that exists — enforced by the
/// `releases.album_id` FK and by `delete_release`, which removes the album when its
/// last release goes.
#[derive(Debug, Clone)]
pub struct ReleaseSummary {
    pub id: String,
    pub album_id: String,
    /// What the release is made of, each carrier with its count; empty if
    /// unknown.
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// Whether every release file has been attached for upload. Orthogonal to
    /// `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's files offline on this device. Meaningful
    /// only when `storage_state` is `Remote` — always `false` for a Local release,
    /// which is already a local file. Kept SEPARATE from `storage_state` so the two
    /// are never confused.
    pub pinned: bool,
    /// The transitions available right now, derived by the core from
    /// `storage_state`, whether this device holds the originals an upload reads,
    /// `pinned`, and whether a cloud home exists; empty without one, and while
    /// the release is `transitioning`. The UI renders these and never
    /// re-derives availability.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub transfer_action: Option<ReleaseStorageAction>,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<ReleaseUpload>,
    /// Whether a transition is under way: files waiting to upload, or a
    /// transfer running. It offers only cancelling; another storage action
    /// would race it.
    pub transitioning: bool,
    pub file_count: i64,
    pub total_size: i64,
    /// This release's *own* cover, or `None` when it has no cover row. Keyed on the
    /// release id, so two releases of one album each resolve to their own art rather
    /// than to the album's primary cover.
    pub cover: Option<ImageRef>,
}

impl ReleaseSummary {
    /// The one place `storage_actions` is derived: [`ReleaseDetail::from_raw`]
    /// projects its fat aggregate down to a slim row and routes through here too.
    pub(crate) fn from_raw(raw: DbReleaseSummary, ctx: &ReleaseResolveCtx<'_>) -> ReleaseSummary {
        let storage_state = storage_state(raw.remote);
        let upload = raw.upload.map(ReleaseUpload::from_raw);
        let transitioning = upload.is_some() || ctx.transfer_action.is_some();
        ReleaseSummary {
            id: raw.id,
            album_id: raw.album_id,
            media: raw.media,
            storage_state,
            pinned: raw.pinned,
            storage_actions: if transitioning {
                Vec::new()
            } else {
                available_storage_actions(
                    storage_state,
                    release_playability(&raw.unstored_audio, ctx.originals),
                    raw.pinned,
                    ctx.has_cloud_home,
                )
            },
            transfer_action: ctx.transfer_action,
            upload,
            transitioning,
            file_count: raw.file_count,
            total_size: raw.total_size,
            cover: ctx.cover.clone(),
        }
    }
}

/// A release's files in this device's upload queue: how many wait and how
/// many are being sent now, how many of its files are stored, and, while any is, of the release's whole size,
/// the bytes of its files stored and those sent of the files being sent. How
/// fast they go is the release's transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseUpload {
    pub progress: crate::library::UploadProgress,
    /// How many of the release's files are stored already.
    pub stored: u32,
    pub bar: Option<crate::library::UploadBar>,
}

impl ReleaseUpload {
    fn from_raw(raw: crate::db::DbReleaseUpload) -> Self {
        let count = |value: i64| u32::try_from(value).expect("a release's queued file count");
        let bytes = |value: i64| u64::try_from(value).expect("an upload's confirmed bytes");
        Self {
            progress: crate::library::UploadProgress {
                queued: count(raw.queued - raw.sending),
                uploading: count(raw.sending),
            },
            stored: count(raw.stored),
            bar: (raw.sending > 0).then(|| crate::library::UploadBar {
                bytes_done: bytes(raw.bytes_done),
                bytes_total: bytes(raw.bytes_total),
            }),
        }
    }
}

/// The fat projection for the album detail view: a [`ReleaseSummary`] plus what only
/// a detail view needs (tracks, files, gallery). Split this way so a list consumer
/// can render a row without loading tracks.
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub summary: ReleaseSummary,
    /// The stored `release_name`, else its year and media, else its position
    /// within its album. The resolver picks that position, so no consumer needs
    /// the index.
    pub name: ReleaseName,
    pub year: Option<i32>,
    /// Every label the release is on, each with its catalog number.
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// What the pressing is: its area, media, status, packaging and details.
    pub facts: bae_types::pressing::PressingFacts,
    /// Summed across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: u64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    pub files: Vec<FileDetail>,
    pub source_audio: Option<SourceAudioSummary>,
    pub image_files: Vec<FileDetail>,
    /// Image files eligible for cover normalization; previews use `image_files`.
    pub cover_files: Vec<FileDetail>,
    /// The cover, then every image file the release has — including cloud-only ones,
    /// which the lightbox fetches on demand.
    pub gallery_items: Vec<GalleryItem>,
    /// Whether this device can play the release now, and why not.
    pub playability: ReleasePlayability,
    /// The download control's state for the release, from its entry in the
    /// download queue as it was read; `None` when there is no control to show.
    pub download_status: Option<ReleaseDownloadStatus>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<crate::import::ReleaseRecord>,
}

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
    Sheet {
        source: ReleaseEditTrackSource,
        tracks: Vec<ReleaseEditTrackContext>,
    },
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
                _ => sides.push(Self {
                    side,
                    runs: vec![ReleaseEditTrackRun::Track(track)],
                }),
            }
        }
        sides
    }

    /// Add the next track: onto the last run when a sheet carves both from
    /// the same file, else as a run of its own.
    #[cfg(desktop)]
    fn append(&mut self, track: ReleaseEditTrackContext) {
        let carved_with = |source: &ReleaseEditTrackSource| {
            track
                .sheet_source()
                .is_some_and(|next| next.file_id == source.file_id)
        };
        match self.runs.pop() {
            Some(ReleaseEditTrackRun::Sheet { source, mut tracks }) if carved_with(&source) => {
                tracks.push(track);
                self.runs
                    .push(ReleaseEditTrackRun::Sheet { source, tracks });
            }
            Some(ReleaseEditTrackRun::Track(last))
                if last.sheet_source().is_some_and(carved_with) =>
            {
                let source = last
                    .sheet_source()
                    .expect("the guard read the track's sheet source")
                    .clone();
                self.runs.push(ReleaseEditTrackRun::Sheet {
                    source,
                    tracks: vec![last, track],
                });
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
    pub(crate) fn placed(self, positions: impl IntoIterator<Item = TrackPosition>) -> Self {
        let tracks = self
            .sides
            .into_iter()
            .flat_map(|side| side.runs)
            .flat_map(ReleaseEditTrackRun::into_tracks);
        Self {
            source_audio: self.source_audio,
            sides: ReleaseEditTrackSide::lay_out(tracks.zip(positions).map(|(track, position)| {
                (
                    crate::util::format::track_side(&position),
                    ReleaseEditTrackContext {
                        position_text: crate::util::format::track_position_text(&position),
                        ..track
                    },
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
    #[cfg(desktop)]
    pub(crate) fn from_raw(raw: &DbReleaseDetail) -> Result<Self, ReleaseEditLoadError> {
        let mut contexts = Vec::with_capacity(raw.tracks.len());
        for entry in &raw.tracks {
            let formats = raw
                .audio_formats
                .iter()
                .filter(|format| format.track_id == entry.track.id)
                .collect::<Vec<_>>();
            let format = match formats.as_slice() {
                [] => {
                    return Err(ReleaseEditLoadError::MissingAudioFormat {
                        track_id: entry.track.id.clone(),
                    });
                }
                [format] => *format,
                _ => {
                    return Err(ReleaseEditLoadError::MultipleAudioFormats {
                        track_id: entry.track.id.clone(),
                    });
                }
            };
            let mut segments = raw
                .audio_segments
                .iter()
                .filter(|segment| segment.audio_format_id == format.id)
                .collect::<Vec<_>>();
            segments.sort_by_key(|segment| segment.segment_index);
            if segments.is_empty() {
                return Err(ReleaseEditLoadError::MissingAudioSegments {
                    track_id: entry.track.id.clone(),
                });
            }

            let mut sources = Vec::new();
            for segment in segments {
                if sources
                    .iter()
                    .any(|source: &ReleaseEditTrackSource| source.file_id == segment.file_id)
                {
                    continue;
                }
                let file = raw
                    .files
                    .iter()
                    .find(|file| file.id == segment.file_id)
                    .ok_or_else(|| ReleaseEditLoadError::MissingSourceFile {
                        track_id: entry.track.id.clone(),
                        file_id: segment.file_id.clone(),
                    })?;
                let layout = file
                    .source_audio
                    .as_ref()
                    .and_then(|audio| audio.layout)
                    .ok_or_else(|| ReleaseEditLoadError::MissingSourceAudio {
                        track_id: entry.track.id.clone(),
                        file_id: segment.file_id.clone(),
                    })?;
                sources.push(ReleaseEditTrackSource {
                    file_id: file.id.clone(),
                    name: file.original_filename.clone(),
                    layout,
                });
            }
            let position = raw.mediums.position(&entry.track);
            contexts.push((
                crate::util::format::track_side(&position),
                ReleaseEditTrackContext {
                    track_id: entry.track.id.clone(),
                    sources,
                    duration_ms: entry.track.duration_ms,
                    position_text: crate::util::format::track_position_text(&position),
                },
            ));
        }

        Ok(Self {
            source_audio: SourceAudioSummary::from_files(
                raw.files
                    .iter()
                    .filter_map(|file| file.source_audio.as_ref()?.summarized()),
            ),
            sides: ReleaseEditTrackSide::lay_out(contexts),
        })
    }
}

impl ReleaseDetail {
    /// Joins per-track artist names (falling back to the album's), projects each
    /// file's stored scan facts, groups tracks by side, builds the gallery, derives
    /// `name`, and composes the slim [`ReleaseSummary`].
    ///
    /// Both ordinary reads and subscriptions route through here, so the resolve
    /// logic stays in one place.
    /// Resolve a raw release-detail row into the projection. The returned anomaly
    /// count remains for the shared caller contract; source-audio facts no longer
    /// depend on track-format joins and therefore add no orphan cases here.
    pub(crate) fn from_raw(
        raw: DbReleaseDetail,
        album_artists: &[crate::credits::LibraryCredit],
        release_index: usize,
        ctx: &ReleaseResolveCtx<'_>,
    ) -> (ReleaseDetail, u32) {
        let release = raw.release;
        let download = raw.download;
        let audio_format_orphans = 0u32;

        let tracks: Vec<TrackDetail> = raw
            .tracks
            .into_iter()
            .map(|entry| {
                let artist_names = if entry.artists.is_empty() {
                    crate::credits::library_line(album_artists)
                } else {
                    crate::credits::library_line(&entry.artists)
                };
                let position = raw.mediums.position(&entry.track);
                let position_text = crate::util::format::track_position_text(&position);
                // On a compilation each row carries its own artist, because the
                // album header names no single one; on a single-artist album the
                // row would only repeat the header, so it shows nothing.
                let display_artist = ctx.is_compilation.then(|| artist_names.clone());
                TrackDetail {
                    id: entry.track.id,
                    title: entry.track.title,
                    duration_ms: entry.track.duration_ms,
                    artist_names,
                    display_artist,
                    position_text,
                    position,
                }
            })
            .collect();

        let track_groups: Vec<TrackGroup> = crate::util::format::group_tracks_by_side(&tracks);

        let remote = !raw.files.is_empty() && raw.files.iter().all(|file| file.stored);
        let playability = release_playability(&raw.unstored_audio, ctx.originals);
        let mut cover_files = Vec::new();
        let mut file_uploads: std::collections::HashMap<String, FileUpload> = raw
            .file_uploads
            .into_iter()
            .map(|upload| (upload.release_file_id.clone(), FileUpload::from_raw(upload)))
            .collect();
        let files: Vec<FileDetail> = raw
            .files
            .into_iter()
            .map(|f| {
                let supports_cover = f.content_type.is_supported_cover();
                let file = FileDetail {
                    place: FilePlace::of(&f.original_filename),
                    kind: FileKind::of(&f.content_type, &f.original_filename),
                    is_image: f.content_type.is_image(),
                    content_type: f.content_type.to_string(),
                    source_audio: f.source_audio,
                    upload: file_uploads.remove(&f.id),
                    id: f.id,
                    original_filename: f.original_filename,
                    file_size: f.file_size,
                };
                if supports_cover {
                    cover_files.push(file.clone());
                }
                file
            })
            .collect();
        let image_files: Vec<FileDetail> = files.iter().filter(|f| f.is_image).cloned().collect();
        let source_audio = SourceAudioSummary::from_files(
            files
                .iter()
                .filter_map(|file| file.source_audio.as_ref()?.summarized()),
        );

        let file_count = files.len() as i64;
        let total_size: i64 = files.iter().map(|f| f.file_size).sum();
        let total_duration_ms: u64 = tracks.iter().map(|t| t.duration_ms).sum();

        let mut gallery = Vec::new();
        // One resolve of the `covers` row feeds both the gallery's "Cover" slot and
        // the summary's `cover`. The lightbox fetches the bytes by image id and
        // caches them under `(id, version)`; coven owns where they sit on disk.
        if let Some(cover_ref) = &ctx.cover {
            gallery.push(GalleryItem {
                id: "cover".to_string(),
                label: "Cover".to_string(),
                source: GallerySource::Cover(cover_ref.clone()),
            });
        }
        // An image file likewise has no stable bae path: the lightbox fetches its
        // bytes by file id through `read_gallery_bytes`, and coven reads them from
        // wherever they are — the user's own file when Local, the cache when Remote.
        for f in &image_files {
            gallery.push(GalleryItem {
                id: f.id.clone(),
                label: f.original_filename.clone(),
                source: GallerySource::ReleaseFile {
                    file_id: f.id.clone(),
                },
            });
        }

        let name = ReleaseName::of(
            release.release_name.as_deref(),
            release.pressing.year,
            &release.pressing.facts.media,
            (release_index + 1) as i64,
        );

        // Route the slim row through `ReleaseSummary::from_raw` rather than build it
        // here, so the `storage_actions` derivation stays in one place.
        let summary = ReleaseSummary::from_raw(
            DbReleaseSummary {
                id: release.id.clone(),
                album_id: release.album_id.clone(),
                media: release.pressing.facts.media.clone(),
                remote,
                pinned: raw.pinned,
                upload: raw.upload,
                file_count,
                total_size,
                unstored_audio: raw.unstored_audio,
            },
            ctx,
        );

        let download_status =
            release_download_status(summary.pinned, &summary.storage_actions, download.as_ref());
        let detail = ReleaseDetail {
            download_status,
            summary,
            name,
            year: release.pressing.year,
            labels: release.pressing.labels,
            facts: release.pressing.facts,
            total_duration_ms,
            tracks,
            track_groups,
            files,
            source_audio,
            image_files,
            cover_files,
            gallery_items: gallery,
            playability,
            records: raw.records,
        };
        (detail, audio_format_orphans)
    }
}

#[cfg(test)]
mod release_edit_display_tests {
    use super::*;

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
        TrackSide::Sided {
            side_letter: letter.to_string(),
        }
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
                (
                    source,
                    run.tracks()
                        .iter()
                        .map(|track| track.track_id.as_str())
                        .collect(),
                )
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
            (
                side("B"),
                context("b6", &[("image-d", Cue), ("image-e", Cue)]),
            ),
        ]);

        assert_eq!(
            sides.iter().map(|side| &side.side).collect::<Vec<_>>(),
            [&side("A"), &side("B")]
        );
        assert_eq!(
            runs(&sides[0]),
            [
                (Some("image-a"), vec!["a1", "a2", "a3"]),
                (None, vec!["a4"]),
            ]
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
            placed
                .sides
                .iter()
                .map(|side| &side.side)
                .collect::<Vec<_>>(),
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
        let now = chrono::Utc::now();
        let format = crate::db::DbAudioFormat {
            id: "format-id".to_string(),
            track_id: track.id.clone(),
            content_type: bae_util::content_type::ContentType::Flac,
            pregap_samples: None,
            generated_pregap_samples: None,
            sample_rate: std::num::NonZeroU32::new(44_100).unwrap(),
            bits_per_sample: Some(std::num::NonZeroU32::new(16).unwrap()),
            channels: std::num::NonZeroU32::new(2).unwrap(),
            track_loudness_lufs: None,
            track_peak_linear: None,
            created_at: now,
        };
        let raw = crate::db::DbReleaseDetail {
            mediums: crate::db::DbReleaseMediums(vec![crate::db::DbReleaseMedium::new_test(
                &release.id,
            )]),
            release,
            tracks: vec![crate::db::DbTrackWithArtists {
                track,
                artists: Vec::new(),
            }],
            files: Vec::new(),
            audio_formats: vec![format.clone()],
            audio_segments: vec![crate::db::DbAudioSegment {
                id: "segment-id".to_string(),
                audio_format_id: format.id,
                segment_index: 0,
                role: crate::db::DbAudioSegmentRole::Main,
                file_id: "missing-file-id".to_string(),
                start_sample: 0,
                end_sample: None,
                start_byte: None,
                end_byte: None,
                created_at: now,
            }],
            records: Vec::new(),
            unstored_audio: Vec::new(),
            pinned: false,
            upload: None,
            file_uploads: Vec::new(),
            download: None,
        };

        assert!(matches!(
            ReleaseEditDisplayContext::from_raw(&raw),
            Err(ReleaseEditLoadError::MissingSourceFile { file_id, .. })
                if file_id == "missing-file-id"
        ));
    }
}

#[cfg(test)]
#[path = "release_tests.rs"]
mod tests;
