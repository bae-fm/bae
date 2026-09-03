//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use super::*;
use crate::db::{DbArtist, DbReleaseDetail, DbReleaseSummary};

/// The per-release inputs `LibraryManager` reads from the DB and coven's cache and
/// threads into the release projections: whether a cloud home exists at all (which
/// gates `storage_actions`), whether this release's blobs are pinned on this device,
/// and its own cover reference. Bundled so the constructors take one context rather
/// than a pile of parameters.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveCtx {
    pub(crate) has_cloud_home: bool,
    pub(crate) pinned: bool,
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
    /// Release media such as "CD" or "Vinyl"; `None` if unknown.
    pub format: Option<String>,
    /// Local or Remote, from the shared `releases.remote` fact. Orthogonal to
    /// `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's blobs offline on this device. Meaningful
    /// only when `storage_state` is `Remote` — always `false` for a Local release,
    /// which is already a local file. Kept SEPARATE from `storage_state` so the two
    /// are never confused.
    pub pinned: bool,
    /// The transitions available right now, derived by the core from
    /// `storage_state`, `pinned`, and whether a cloud home exists; empty without
    /// one. The UI renders these and never re-derives availability, but does apply
    /// the in-flight-uploads gate from its outbox snapshot.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub transfer_action: Option<ReleaseStorageAction>,
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
    pub(crate) fn from_raw(raw: DbReleaseSummary, ctx: &ReleaseResolveCtx) -> ReleaseSummary {
        let storage_state = storage_state(raw.remote);
        ReleaseSummary {
            id: raw.id,
            album_id: raw.album_id,
            format: raw.format,
            storage_state,
            pinned: ctx.pinned,
            storage_actions: available_storage_actions(
                storage_state,
                ctx.pinned,
                ctx.has_cloud_home,
            ),
            transfer_action: ctx.transfer_action,
            file_count: raw.file_count,
            total_size: raw.total_size,
            cover: ctx.cover.clone(),
        }
    }
}

/// The fat projection for the album detail view: a [`ReleaseSummary`] plus what only
/// a detail view needs (tracks, files, gallery). Split this way so a list consumer
/// can render a row without loading tracks.
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub summary: ReleaseSummary,
    /// The stored `release_name`, else "$year $format", else "Release $N" from the
    /// release's position within its album. The resolver picks that position, so no
    /// consumer needs the index.
    pub display_name: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// Summed across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: i64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    pub files: Vec<FileDetail>,
    pub source_audio: Option<SourceAudioSummary>,
    pub image_files: Vec<FileDetail>,
    /// The cover, then every image file the release has — including cloud-only ones,
    /// which the lightbox fetches on demand.
    pub gallery_items: Vec<GalleryItem>,
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
    pub duration_ms: Option<i64>,
    pub side: TrackSide,
}

/// Release-backed context needed by the shared metadata editor in addition to
/// its writable raw form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEditDisplayContext {
    pub source_audio: Option<SourceAudioSummary>,
    pub tracks: Vec<ReleaseEditTrackContext>,
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
    pub(crate) fn from_raw(raw: &DbReleaseDetail) -> Result<Self, ReleaseEditLoadError> {
        let has_multiple_sides = raw
            .tracks
            .iter()
            .map(|entry| entry.track.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
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
            let position = crate::util::format::compute_track_position(
                raw.release.pressing.format.as_deref(),
                entry.track.side,
                entry.track.track_number,
                has_multiple_sides,
            );
            contexts.push(ReleaseEditTrackContext {
                track_id: entry.track.id.clone(),
                sources,
                duration_ms: entry.track.duration_ms,
                side: crate::util::format::track_side(&position),
            });
        }

        Ok(Self {
            source_audio: SourceAudioSummary::from_descriptors(
                raw.files
                    .iter()
                    .filter_map(|file| file.source_audio.as_ref()?.descriptor()),
            ),
            tracks: contexts,
        })
    }
}

impl ReleaseDetail {
    /// Joins per-track artist names (falling back to the album's), projects each
    /// file's stored scan facts, groups tracks by side, builds the gallery, derives
    /// `display_name`, and composes the slim [`ReleaseSummary`].
    ///
    /// Both ordinary reads and subscriptions route through here, so the resolve
    /// logic stays in one place.
    /// Resolve a raw release-detail row into the projection. The returned anomaly
    /// count remains for the shared caller contract; source-audio facts no longer
    /// depend on track-format joins and therefore add no orphan cases here.
    pub(crate) fn from_raw(
        raw: DbReleaseDetail,
        album_artists: &[DbArtist],
        release_index: usize,
        ctx: &ReleaseResolveCtx,
    ) -> (ReleaseDetail, u32) {
        let release = raw.release;
        let audio_format_orphans = 0u32;

        let has_multiple_sides = {
            let mut sides = std::collections::HashSet::new();
            for t in &raw.tracks {
                sides.insert(t.track.side);
            }
            sides.len() > 1
        };

        let tracks: Vec<TrackDetail> = raw
            .tracks
            .into_iter()
            .map(|entry| {
                let artist_names = if entry.artists.is_empty() {
                    join_artist_names(album_artists)
                } else {
                    join_artist_names(&entry.artists)
                };
                let position = crate::util::format::compute_track_position(
                    release.pressing.format.as_deref(),
                    entry.track.side,
                    entry.track.track_number,
                    has_multiple_sides,
                );
                let position_text = crate::util::format::track_position_text(&position);
                // On a compilation each row carries its own artist, because the
                // album header names no single one; on a single-artist album the
                // row would only repeat the header, so it shows nothing.
                let display_artist = ctx.is_compilation.then(|| artist_names.clone());
                TrackDetail {
                    id: entry.track.id,
                    title: entry.track.title,
                    side: entry.track.side,
                    track_number: entry.track.track_number,
                    duration_ms: entry.track.duration_ms,
                    artist_names,
                    display_artist,
                    position_text,
                    position,
                }
            })
            .collect();

        let track_groups: Vec<TrackGroup> = crate::util::format::group_tracks_by_side(&tracks);

        let files: Vec<FileDetail> = raw
            .files
            .into_iter()
            .map(|f| FileDetail {
                is_image: f.content_type.is_image(),
                content_type: f.content_type.to_string(),
                source_audio: f.source_audio,
                id: f.id,
                original_filename: f.original_filename,
                file_size: f.file_size,
            })
            .collect();
        let image_files: Vec<FileDetail> = files.iter().filter(|f| f.is_image).cloned().collect();
        let source_audio = SourceAudioSummary::from_descriptors(
            files
                .iter()
                .filter_map(|file| file.source_audio.as_ref()?.descriptor()),
        );

        let file_count = files.len() as i64;
        let total_size: i64 = files.iter().map(|f| f.file_size).sum();
        let total_duration_ms: i64 = tracks.iter().filter_map(|t| t.duration_ms).sum();

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

        let display_name = release_display_name(
            release.release_name.as_deref(),
            release.pressing.year,
            release.pressing.format.as_deref(),
            (release_index + 1) as i64,
        );

        // Route the slim row through `ReleaseSummary::from_raw` rather than build it
        // here, so the `storage_actions` derivation stays in one place.
        let summary = ReleaseSummary::from_raw(
            DbReleaseSummary {
                id: release.id.clone(),
                album_id: release.album_id.clone(),
                format: release.pressing.format.clone(),
                remote: release.remote,
                any_file_id: files.first().map(|f| f.id.clone()),
                file_count,
                total_size,
            },
            ctx,
        );

        let detail = ReleaseDetail {
            summary,
            display_name,
            year: release.pressing.year,
            label: release.pressing.label,
            catalog_number: release.pressing.catalog_number,
            country: release.pressing.country,
            total_duration_ms,
            tracks,
            track_groups,
            files,
            source_audio,
            image_files,
            gallery_items: gallery,
        };
        (detail, audio_format_orphans)
    }
}

#[cfg(test)]
mod release_edit_display_tests {
    use super::*;

    #[test]
    fn missing_segment_file_is_an_editor_load_error() {
        let release = crate::db::DbRelease::new_test("album-id", "release-id");
        let track = crate::db::DbTrack::new_test(&release.id, "track-id", "Track Title", Some(1));
        let now = chrono::Utc::now();
        let format = crate::db::DbAudioFormat {
            id: "format-id".to_string(),
            track_id: track.id.clone(),
            content_type: crate::util::content_type::ContentType::Flac,
            pregap_ms: None,
            generated_pregap_ms: None,
            pregap_samples: None,
            generated_pregap_samples: None,
            sample_rate: 44_100,
            bits_per_sample: Some(16),
            channels: 2,
            track_loudness_lufs: None,
            track_peak_linear: None,
            created_at: now,
        };
        let raw = crate::db::DbReleaseDetail {
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
            identities: Vec::new(),
        };

        assert!(matches!(
            ReleaseEditDisplayContext::from_raw(&raw),
            Err(ReleaseEditLoadError::MissingSourceFile { file_id, .. })
                if file_id == "missing-file-id"
        ));
    }
}
