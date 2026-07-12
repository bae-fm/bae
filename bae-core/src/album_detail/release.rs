//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use tracing::{debug, warn};

use super::*;
use crate::db::{DbArtist, DbReleaseDetail, DbReleaseSummary};

/// The per-release inputs the `LibraryManager` reads from the DB / coven cache
/// and threads into the release projections: whether a cloud home exists at all
/// (gates `storage_actions`), whether coven keeps this release's blobs pinned on
/// this device (the orthogonal coven-cache property), and this release's own
/// cover reference (image id + version from the `covers` row). Bundled so
/// `ReleaseSummary` / `ReleaseDetail` construction takes one context instead of
/// a parameter pile.
#[derive(Debug, Clone)]
pub(crate) struct ReleaseResolveCtx {
    pub(crate) has_cloud_home: bool,
    pub(crate) pinned: bool,
    pub(crate) cover: Option<ImageRef>,
    pub(crate) transfer_action: Option<ReleaseStorageAction>,
}

/// Resolved release summary: the slim projection that list views (storage
/// manager, release pickers, etc.) render one row per entity. Every field
/// is pre-computed; no downstream consumer needs to derive anything.
///
/// Composed into [`ReleaseDetail`] for detail views. Interned into the
/// UI-side "releases" slice — see notes on summary/detail composition at
/// the top of this file.
///
/// Invariant: `album_id` refers to an album that exists. Every release
/// belongs to an album (enforced by the `releases.album_id` FK and by
/// `delete_release`, which removes the album when its last release goes).
#[derive(Debug, Clone)]
pub struct ReleaseSummary {
    pub id: String,
    pub album_id: String,
    /// Audio format (e.g. "FLAC", "MP3"). `None` if unknown.
    pub format: Option<String>,
    /// The release's storage state — Local (local) or Remote (cloud) —
    /// derived from the shared `releases.remote` fact. Orthogonal to `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the orthogonal coven-cache property, asked of coven's cache.
    /// Meaningful only when `storage_state` is `Remote` (always `false` for an
    /// Local release, which is already a local file). Kept SEPARATE from
    /// `storage_state` so the two concepts are never confused.
    pub pinned: bool,
    /// Storage transitions available for this release right now, computed by
    /// the core from `storage_state`, `pinned`, and whether a cloud home exists.
    /// The UI renders these (the album-detail "Storage…" sheet and the Storage
    /// Manager row context menu); it never re-derives availability. Empty with
    /// no cloud home. The in-flight-uploads gate lives in the UI: it consults
    /// the outbox snapshot's release groups before showing these actions.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub transfer_action: Option<ReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    /// Reference to this release's own cover (image id + version), or `None` when
    /// the release has no cover row. Keyed on the release id — covers are stored
    /// per release — so two releases of one album resolve to their own art rather
    /// than the album's primary cover.
    pub cover: Option<ImageRef>,
}

impl ReleaseSummary {
    /// Project a slim `DbReleaseSummary` row. `storage_state` derives from the
    /// shared `remote` gate; `pinned`, `has_cloud_home`, and the release's own
    /// `cover` come from `ctx` (the manager reads them from coven's cache, the
    /// configured home, and the `covers` row). Single source of truth for the
    /// `storage_actions` derivation: [`ReleaseDetail::from_raw`] projects its fat
    /// aggregate down to the slim row and routes through here too.
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

/// Resolved release detail: the fat projection for the album detail view.
/// Composes a [`ReleaseSummary`] (slim fields) with the per-release data
/// that only the detail view needs (tracks, files, gallery). Split this
/// way so a list consumer can display a row without loading tracks.
///
/// `display_name` is pre-computed: the release's own `release_name`, or a
/// "`year format`" derivation, or "Release $N" using the release's
/// position within its album. The resolver picks the position; consumers
/// never need the index.
///
#[derive(Debug, Clone)]
pub struct ReleaseDetail {
    pub summary: ReleaseSummary,
    /// Human-readable name for picker UI: the stored `release_name`, or
    /// "$year $format", or "Release $N" fallback.
    pub display_name: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// Total duration across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: i64,
    pub tracks: Vec<TrackDetail>,
    pub track_groups: Vec<TrackGroup>,
    pub files: Vec<FileDetail>,
    pub image_files: Vec<FileDetail>,
    /// Cover (if on disk) followed by every image file the release has —
    /// including cloud-only ones not yet downloaded (those carry no local path;
    /// the lightbox fetches them on demand).
    pub gallery_items: Vec<GalleryItem>,
}

impl ReleaseDetail {
    /// Project a fat `DbReleaseDetail` into the display-ready detail. Joins
    /// per-track artist names (falling back to album artists), formats audio
    /// labels, groups tracks by side, builds the gallery, derives the picker
    /// `display_name` from the release's `release_index` within its album, and
    /// composes the slim [`ReleaseSummary`]. `ctx` carries the coven-read inputs
    /// (pin state, cloud-home presence, this release's own cover).
    ///
    /// Both the manager and the upload observer route through here (the observer
    /// holds the same `Database` and a `CovenHandle`, so it can emit
    /// `ReleaseUpdated` events without owning a manager) so the resolve logic
    /// stays in one place.
    pub(crate) fn from_raw(
        raw: DbReleaseDetail,
        album_artists: &[DbArtist],
        release_index: usize,
        ctx: &ReleaseResolveCtx,
    ) -> ReleaseDetail {
        let release = raw.release;

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
                TrackDetail {
                    id: entry.track.id,
                    title: entry.track.title,
                    side: entry.track.side,
                    track_number: entry.track.track_number,
                    duration_ms: entry.track.duration_ms,
                    artist_names,
                    position_text,
                    position,
                }
            })
            .collect();

        let track_groups: Vec<TrackGroup> = crate::util::format::group_tracks_by_side(&tracks);

        // Describe each audio file's format. Audio segments carry the owning file
        // id; a single-file CUE rip has many track formats sharing one file through
        // their segments. Group by file id (first row wins — every row for one
        // file shares the file-level codec/rate/depth/channels). A file's audio
        // duration (for deriving a lossy file's average bitrate) is the sum of its
        // tracks' durations, kept as `Some(sum)` only while every contributing track
        // has a known duration; one unknown duration makes the file's total unknown
        // so no bitrate is derived from a partial sum.
        let audio_formats = raw.audio_formats;
        let audio_segments = raw.audio_segments;
        let audio_format_by_id: std::collections::HashMap<&str, &crate::db::DbAudioFormat> =
            audio_formats
                .iter()
                .map(|format| (format.id.as_str(), format))
                .collect();
        let track_durations: std::collections::HashMap<&str, Option<i64>> = tracks
            .iter()
            .map(|t| (t.id.as_str(), t.duration_ms))
            .collect();
        let mut file_format = std::collections::HashMap::new();
        let mut file_audio_duration_ms: std::collections::HashMap<&str, Option<i64>> =
            std::collections::HashMap::new();
        for segment in &audio_segments {
            let Some(af) = audio_format_by_id
                .get(segment.audio_format_id.as_str())
                .copied()
            else {
                warn!(
                    "audio segment {} references missing audio_format {}; skipping format attribution",
                    segment.id, segment.audio_format_id
                );
                continue;
            };
            let file_id = segment.file_id.as_str();
            file_format.entry(file_id).or_insert(af);
            // `af` is joined from this release's tracks, so the lookup should be
            // present; its value is the track's own (optional) duration. A missing
            // entry means that join invariant broke — log it rather than silently
            // folding it in as unknown. Fold into the file total: any unknown
            // duration collapses the file's total to unknown.
            let track_dur = segment
                .end_sample
                .map(|end| {
                    (end.saturating_sub(segment.start_sample) * 1000) / af.sample_rate.max(1)
                })
                .or_else(|| match track_durations.get(af.track_id.as_str()) {
                    Some(duration) => *duration,
                    None => {
                        warn!(
                            "audio_format {} references track {} absent from the release; \
                             treating its duration as unknown",
                            af.id, af.track_id
                        );
                        None
                    }
                });
            let slot = file_audio_duration_ms.entry(file_id).or_insert(Some(0));
            *slot = match (*slot, track_dur) {
                (Some(acc), Some(d)) => Some(acc + d),
                _ => None,
            };
        }

        let files: Vec<FileDetail> = raw
            .files
            .into_iter()
            .map(|f| {
                let audio_format = match file_format.get(f.id.as_str()) {
                    Some(af) => {
                        // Lossy codecs store no bit depth; show the average bitrate
                        // (file bytes over the file's audio duration) when the full
                        // duration is known. When it isn't, the label drops the
                        // bitrate part — log that legitimate skip rather than hiding
                        // the missing duration.
                        let bitrate_kbps = if af.bits_per_sample.is_none() {
                            match file_audio_duration_ms.get(f.id.as_str()).copied().flatten() {
                                Some(dur) if dur > 0 => Some(f.file_size * 8 / dur),
                                _ => {
                                    debug!(
                                        "lossy file {} ({}) has no known positive audio \
                                         duration; omitting bitrate from its format label",
                                        f.id, f.original_filename
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        };
                        Some(AudioFormat {
                            codec: af.content_type.display_name().to_string(),
                            sample_rate_hz: af.sample_rate,
                            bits_per_sample: af.bits_per_sample,
                            bitrate_kbps,
                            channels: af.channels,
                        })
                    }
                    None => {
                        // A non-audio file (image, cue) legitimately has no format
                        // row; an audio file without one is a data gap worth noting.
                        if f.content_type.is_audio() {
                            warn!(
                                "release file {} ({}) is audio but has no audio_format row",
                                f.id, f.original_filename
                            );
                        }
                        None
                    }
                };
                FileDetail {
                    is_image: f.content_type.is_image(),
                    content_type: f.content_type.to_string(),
                    audio_format,
                    id: f.id,
                    original_filename: f.original_filename,
                    file_size: f.file_size,
                }
            })
            .collect();
        let image_files: Vec<FileDetail> = files.iter().filter(|f| f.is_image).cloned().collect();

        let file_count = files.len() as i64;
        let total_size: i64 = files.iter().map(|f| f.file_size).sum();
        let total_duration_ms: i64 = tracks.iter().filter_map(|t| t.duration_ms).sum();

        let mut gallery = Vec::new();
        // The release's own cover, resolved once from the `covers` row: the gallery's
        // "Cover" slot and the summary's `cover` field both read it. The lightbox
        // fetches its bytes by image id (`read_image_blob`) and caches them under
        // `(id, version)`. coven owns the bytes' on-disk location (its local store
        // while Local, its cache while Remote).
        if let Some(cover_ref) = &ctx.cover {
            gallery.push(GalleryItem {
                id: "cover".to_string(),
                label: "Cover".to_string(),
                source: GallerySource::Cover(cover_ref.clone()),
            });
        }
        // Every image file the release has. coven owns the locality-aware read, so the
        // lightbox fetches an image file's bytes on demand by file id through
        // `read_gallery_bytes` (the user's own file when Local, the cache/cloud when
        // Remote) — there is no stable bae path for it.
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

        // The summary is the slim projection of this same release: build that row
        // (with the file totals computed above) and route it through the shared
        // `ReleaseSummary::from_raw` so `storage_actions` derivation stays in one
        // place.
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

        ReleaseDetail {
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
            image_files,
            gallery_items: gallery,
        }
    }
}
