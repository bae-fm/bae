//! Resolved release types (`ReleaseSummary`, `ReleaseDetail`) and the pure
//! projections that produce them from raw DB aggregates.

use tracing::{debug, warn};

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
    /// Whether the sync loop is running — not just whether a home is connected.
    /// `MakeRemote` needs it; see [`crate::album_detail::available_storage_actions`].
    pub(crate) sync_ready: bool,
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
    /// "FLAC", "MP3", … `None` if unknown.
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
                ctx.sync_ready,
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
    pub image_files: Vec<FileDetail>,
    /// The cover, then every image file the release has — including cloud-only ones,
    /// which the lightbox fetches on demand.
    pub gallery_items: Vec<GalleryItem>,
}

impl ReleaseDetail {
    /// Joins per-track artist names (falling back to the album's), describes each
    /// file's audio format, groups tracks by side, builds the gallery, derives
    /// `display_name`, and composes the slim [`ReleaseSummary`].
    ///
    /// Both the manager and the upload observer route through here, so the resolve
    /// logic stays in one place. (The observer holds the same `Database` and a
    /// `CovenHandle`, so it can emit `ReleaseUpdated` without owning a manager.)
    /// Resolve a raw release-detail row into the projection, plus a count of
    /// audio-format orphans found (a segment naming a missing format, a format
    /// naming an absent track, an audio file with no format row). This pure layer
    /// only detects and logs each; the count lets the diagnostics-holding manager
    /// caller ship the `audio_format_orphaned` anomaly without this layer growing
    /// a diagnostics handle.
    pub(crate) fn from_raw(
        raw: DbReleaseDetail,
        album_artists: &[DbArtist],
        release_index: usize,
        ctx: &ReleaseResolveCtx,
    ) -> (ReleaseDetail, u32) {
        let release = raw.release;
        let mut audio_format_orphans = 0u32;

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

        // Each audio segment names its owning file, and a single-file CUE rip has
        // many track formats sharing one file through their segments. So group by
        // file id, first row winning — every row for a file agrees on the
        // file-level codec/rate/depth/channels.
        //
        // A file's audio duration (which a lossy file's average bitrate is derived
        // from) is the sum of its tracks' durations, and stays `Some` only while
        // every contributing track has a known duration: one unknown collapses the
        // total to unknown, so no bitrate is ever derived from a partial sum.
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
                audio_format_orphans += 1;
                continue;
            };
            let file_id = segment.file_id.as_str();
            file_format.entry(file_id).or_insert(af);
            // `af` is joined from this release's own tracks, so its track must be in
            // `track_durations`. A miss means that join invariant broke — log it
            // rather than quietly fold it in as unknown.
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
                        audio_format_orphans += 1;
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
                        // A lossy codec stores no bit depth, so the label shows an
                        // average bitrate instead — file bytes over the file's audio
                        // duration, and only when that duration is fully known.
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
                        // An image or cue legitimately has no format row; an *audio*
                        // file without one is a data gap worth saying so about.
                        if f.content_type.is_audio() {
                            warn!(
                                "release file {} ({}) is audio but has no audio_format row",
                                f.id, f.original_filename
                            );
                            audio_format_orphans += 1;
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
            image_files,
            gallery_items: gallery,
        };
        (detail, audio_format_orphans)
    }
}
