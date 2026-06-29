//! Resolver helpers: turn `Db*` aggregates from `crate::db::models` into the
//! resolved UI shapes in `crate::album_detail`.

use super::*;

/// Produces a resolved `ReleaseStorageSummary` from a raw
/// `DbReleaseStorageSummary`: derives `storage_state` from `remote` and `pinned`
/// (the caller asks coven's cache whether the release's blobs are pinned). The raw
/// `primary_release_id` comes from SQL's `COALESCE(a.primary_release_id,
/// <first release id>)` and is non-null by construction: every album has at
/// least one release (enforced by `delete_release`).
pub(super) fn resolve_release_storage_summary(
    raw: DbReleaseStorageSummary,
    has_cloud_home: bool,
    pinned: bool,
) -> ReleaseStorageSummary {
    let storage_state = crate::album_detail::storage_state(raw.remote);
    let storage_actions =
        crate::album_detail::available_storage_actions(storage_state, pinned, has_cloud_home);
    ReleaseStorageSummary {
        storage_state,
        pinned,
        storage_actions,
        release_id: raw.release_id,
        album_id: raw.album_id,
        album_title: raw.album_title,
        artist_names: raw.artist_names,
        format: raw.format,
        primary_release_id: raw
            .primary_release_id
            .expect("album has at least one release"),
        file_count: raw.file_count,
        total_size: raw.total_size,
    }
}

/// Apply the `primary_release_id` fallback: stored value, or the first release
/// if unset. Produces a resolved `AlbumSummary` from a raw `DbAlbumSummary`,
/// with the album's cover resolved to its cache-bustable identifier via
/// `resolve_cover`. The fallback always succeeds: every album has at least one
/// release (enforced by `delete_release`).
///
/// `resolve_cover` maps the primary release id to its cover reference (image id +
/// version) from the `covers` row; passed in rather than reaching for `&self` so
/// the same resolver serves the SQL-page, search, and event paths without
/// duplicating the existence-and-version logic.
pub(super) fn resolve_album_summary(
    raw: DbAlbumSummary,
    resolve_cover: impl Fn(&str) -> Option<ImageRef>,
) -> AlbumSummary {
    let primary_release_id = raw
        .primary_release_id
        .clone()
        .or_else(|| raw.release_ids.first().cloned())
        .expect("album has at least one release");
    let cover = resolve_cover(&primary_release_id);
    AlbumSummary {
        id: raw.id,
        title: raw.title,
        year: raw.year,
        is_compilation: raw.is_compilation,
        artist_names: raw.artist_names,
        release_ids: raw.release_ids,
        primary_release_id,
        cover,
    }
}

/// Resolve a raw release-summary aggregate: derives `storage_state` from the
/// `remote` gate alone. `pinned` and `has_cloud_home` are read by the caller
/// and passed down (DI) — `pinned` from coven's offline-cache check,
/// `has_cloud_home` so `storage_actions` reflects whether remote storage exists
/// at all. `resolve_cover` maps the release's own id to its cover reference.
fn resolve_release_summary(
    raw: DbReleaseSummary,
    has_cloud_home: bool,
    pinned: bool,
    resolve_cover: impl Fn(&str) -> Option<ImageRef>,
) -> ReleaseSummary {
    let cover = resolve_cover(&raw.id);
    build_release_summary(
        raw.id,
        raw.album_id,
        raw.format,
        crate::album_detail::storage_state(raw.remote),
        pinned,
        raw.file_count,
        raw.total_size,
        has_cloud_home,
        cover,
    )
}

/// Single source of truth for `ReleaseSummary` construction. Both
/// `resolve_release_summary` (from a SQL-aggregated `DbReleaseSummary`
/// row) and `resolve_release` (from the fat `DbReleaseDetail` with its
/// own `files` vec) route through here so the `storage_actions`
/// derivation stays in one place.
/// `cover` is the release's own cover reference (image id + version), resolved by
/// the caller from the `covers` row.
#[allow(clippy::too_many_arguments)]
fn build_release_summary(
    id: String,
    album_id: String,
    format: Option<String>,
    storage_state: crate::album_detail::ReleaseStorageState,
    pinned: bool,
    file_count: i64,
    total_size: i64,
    has_cloud_home: bool,
    cover: Option<ImageRef>,
) -> ReleaseSummary {
    ReleaseSummary {
        id,
        album_id,
        format,
        storage_state,
        pinned,
        storage_actions: crate::album_detail::available_storage_actions(
            storage_state,
            pinned,
            has_cloud_home,
        ),
        file_count,
        total_size,
        cover,
    }
}

/// Resolve a raw storage-page row: releases and their parent albums
/// arrive pre-joined from SQL; each half maps to its summary resolver.
/// `resolve_cover` maps an image id (the release's own id, and the album's
/// primary release id) to its cover reference; it serves both halves so the
/// release row carries its own art and the album carries the primary's.
pub(super) fn resolve_storage_row(
    raw: DbStorageRow,
    has_cloud_home: bool,
    pinned: bool,
    resolve_cover: impl Fn(&str) -> Option<ImageRef>,
) -> StorageRow {
    StorageRow {
        release: resolve_release_summary(raw.release, has_cloud_home, pinned, &resolve_cover),
        album: resolve_album_summary(raw.album, &resolve_cover),
    }
}

/// Human-readable name for picker UI. Tries in order:
/// 1. The stored `release_name` (e.g. "1974 Vinyl", "Deluxe Edition").
/// 2. "$year $format" (e.g. "1974 Vinyl").
/// 3. "Release $N" (1-based) when neither is present.
fn build_release_display_name(
    release_name: Option<&str>,
    year: Option<i32>,
    format: Option<&str>,
    index: usize,
) -> String {
    if let Some(name) = release_name {
        return name.to_string();
    }
    let mut parts = Vec::new();
    if let Some(y) = year {
        parts.push(y.to_string());
    }
    if let Some(f) = format {
        parts.push(f.to_string());
    }
    if parts.is_empty() {
        format!("Release {}", index + 1)
    } else {
        parts.join(" ")
    }
}

/// Resolve a raw album search result. Field-by-field copy. The raw
/// `primary_release_id` comes from SQL's `COALESCE(a.primary_release_id,
/// <first release id>)` and is non-null by construction: every album has
/// at least one release (enforced by `delete_release`).
fn resolve_album_search_result(
    raw: DbAlbumSearchResult,
    resolve_cover: impl Fn(&str) -> Option<ImageRef>,
) -> AlbumSearchResult {
    // The primary release id resolves the cover but isn't surfaced — the search
    // UI navigates by album id.
    let primary_release_id = raw
        .primary_release_id
        .expect("album has at least one release");
    let cover = resolve_cover(&primary_release_id);
    AlbumSearchResult {
        id: raw.id,
        title: raw.title,
        year: raw.year,
        artist_name: raw.artist_name,
        cover,
    }
}

/// Resolve a raw track search result into its display-ready shape.
fn resolve_track_search_result(raw: DbTrackSearchResult) -> TrackSearchResult {
    TrackSearchResult {
        id: raw.id,
        title: raw.title,
        duration_ms: raw.duration_ms,
        album_id: raw.album_id,
        album_title: raw.album_title,
        artist_name: raw.artist_name,
    }
}

/// Resolve the raw search-result container by mapping each inner list.
/// `covers` maps an album's primary release id to its cover reference.
pub(super) fn resolve_search_results(
    raw: DbLibrarySearchResults,
    covers: &HashMap<String, ImageRef>,
) -> SearchResults {
    SearchResults {
        albums: raw
            .albums
            .into_iter()
            .map(|a| resolve_album_search_result(a, |rid| covers.get(rid).cloned()))
            .collect(),
        tracks: raw
            .tracks
            .into_iter()
            .map(resolve_track_search_result)
            .collect(),
    }
}

/// The cover [`ImageRef`] for one release from its `covers` row's `_updated_at`,
/// or `None` when it has no cover row. Free function so the manager's `cover_ref`
/// and the observer's `find_release_detail_with` share one construction.
pub(crate) async fn cover_ref_for(
    database: &Database,
    release_id: &str,
) -> Result<Option<ImageRef>, LibraryError> {
    Ok(database
        .cover_version(release_id)
        .await?
        .map(|version| ImageRef {
            id: release_id.to_string(),
            version,
        }))
}

/// Free-function variant of `LibraryManager::find_release_detail`.
///
/// Used by the manager and by the upload observer (which holds the same
/// `Database` and a `CovenHandle` so it can emit `ReleaseUpdated` events for a
/// release whose `local_path` just got cleared at the end of an upload run,
/// without owning a manager). The pin-state is answered through `handle`, the
/// same door the manager uses. `has_cloud_home` is supplied by the caller; the
/// observer fires inside a running sync cycle so it can pass `true`.
pub(crate) async fn find_release_detail_with(
    database: &Database,
    handle: &CovenHandle,
    has_cloud_home: bool,
    release_id: &str,
) -> Result<Option<ReleaseDetail>, LibraryError> {
    let Some(raw) = database.find_release_detail(release_id).await? else {
        return Ok(None);
    };
    let album_id = raw.release.album_id.clone();
    let album_artists = database.get_artists_for_album(&album_id).await?;
    let releases = database.get_releases_for_album(&album_id).await?;
    let release_index = releases
        .iter()
        .position(|r| r.id == release_id)
        .expect("release belongs to its album");
    let pinned = match raw.files.first() {
        Some(file) => release_file_pinned(handle, &file.id).await?,
        None => false,
    };
    let cover = cover_ref_for(database, release_id).await?;
    Ok(Some(resolve_release(
        raw,
        &album_artists,
        release_index,
        has_cloud_home,
        pinned,
        cover,
    )))
}

/// Free-function variant of `LibraryManager::find_release_detail`. Both the
/// manager and the upload observer (which holds the same `LibraryDir` and
/// `Database` so it can emit `ReleaseUpdated` events without owning a manager)
/// route through here so the resolve logic stays in one place.
pub(crate) fn resolve_release(
    raw: crate::db::DbReleaseDetail,
    album_artists: &[crate::db::DbArtist],
    release_index: usize,
    has_cloud_home: bool,
    pinned: bool,
    cover: Option<ImageRef>,
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
            TrackDetail {
                id: entry.track.id,
                title: entry.track.title,
                side: entry.track.side,
                track_number: entry.track.track_number,
                duration_ms: entry.track.duration_ms,
                artist_names,
                position,
            }
        })
        .collect();

    let track_groups: Vec<TrackGroup> = crate::util::format::group_tracks_by_side(&tracks);

    // Describe each audio file's format. Audio-format rows key on track id and
    // carry the owning file id; a single-file CUE rip has one row per track, all
    // sharing that file id. Group by file id (first row wins — every row for one
    // file shares the file-level codec/rate/depth/channels). A file's audio
    // duration (for deriving a lossy file's average bitrate) is the sum of its
    // tracks' durations, kept as `Some(sum)` only while every contributing track
    // has a known duration; one unknown duration makes the file's total unknown
    // so no bitrate is derived from a partial sum.
    let audio_formats = raw.audio_formats;
    let track_durations: std::collections::HashMap<&str, Option<i64>> = tracks
        .iter()
        .map(|t| (t.id.as_str(), t.duration_ms))
        .collect();
    let mut file_format = std::collections::HashMap::new();
    let mut file_audio_duration_ms: std::collections::HashMap<&str, Option<i64>> =
        std::collections::HashMap::new();
    for af in &audio_formats {
        let Some(file_id) = af.file_id.as_deref() else {
            debug!(
                "audio_format {} has no file_id; skipping format attribution",
                af.id
            );
            continue;
        };
        file_format.entry(file_id).or_insert(af);
        // `af` is joined from this release's tracks, so the lookup should be
        // present; its value is the track's own (optional) duration. A missing
        // entry means that join invariant broke — log it rather than silently
        // folding it in as unknown. Fold into the file total: any unknown
        // duration collapses the file's total to unknown.
        let track_dur = match track_durations.get(af.track_id.as_str()) {
            Some(duration) => *duration,
            None => {
                warn!(
                    "audio_format {} references track {} absent from the release; \
                     treating its duration as unknown",
                    af.id, af.track_id
                );
                None
            }
        };
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
                    Some(crate::album_detail::AudioFormat {
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
    if let Some(cover_ref) = &cover {
        gallery.push(GalleryItem {
            id: "cover".to_string(),
            label: "Cover".to_string(),
            source: crate::album_detail::GallerySource::Cover(cover_ref.clone()),
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
            source: crate::album_detail::GallerySource::ReleaseFile {
                file_id: f.id.clone(),
            },
        });
    }

    let display_name = build_release_display_name(
        release.release_name.as_deref(),
        release.pressing.year,
        release.pressing.format.as_deref(),
        release_index,
    );

    let summary = build_release_summary(
        release.id.clone(),
        release.album_id.clone(),
        release.pressing.format.clone(),
        release.storage_state(),
        pinned,
        file_count,
        total_size,
        has_cloud_home,
        cover,
    );

    ReleaseDetail {
        summary,
        display_name,
        release_name: release.release_name,
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

impl LibraryManager {
    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`.
    /// Joins artist names, formats labels, groups tracks by side, builds
    /// galleries, and applies the `primary_release_id` fallback. The
    /// fallback always succeeds: every album has at least one release
    /// (enforced by `delete_release`).
    pub(super) async fn resolve_album_detail(
        &self,
        raw: crate::db::DbAlbumDetail,
    ) -> Result<AlbumDetail, LibraryError> {
        let artist_names = join_artist_names(&raw.artists);
        // The primary is the stored value when it's still present, otherwise
        // the album's first release.
        let primary_release_id = raw
            .album
            .primary_release_id
            .clone()
            .filter(|id| raw.releases.iter().any(|r| &r.release.id == id))
            .or_else(|| raw.releases.first().map(|r| r.release.id.clone()))
            .expect("album has at least one release");

        let has_cloud_home = self.has_cloud_home();
        // One cover lookup for the whole album: the album's cover is the primary
        // release's, and each release carries its own.
        let release_ids: Vec<String> = raw.releases.iter().map(|r| r.release.id.clone()).collect();
        let covers = self.cover_refs(&release_ids).await?;
        let cover = covers.get(&primary_release_id).cloned();
        let mut releases = Vec::with_capacity(raw.releases.len());
        for (i, r) in raw.releases.into_iter().enumerate() {
            // Ask coven's cache whether this release is pinned — the orthogonal
            // coven-cache property of a remote release.
            let pinned = self
                .release_pinned(r.files.first().map(|f| f.id.as_str()))
                .await?;
            let release_cover = covers.get(&r.release.id).cloned();
            releases.push(resolve_release(
                r,
                &raw.artists,
                i,
                has_cloud_home,
                pinned,
                release_cover,
            ));
        }

        Ok(AlbumDetail {
            album: raw.album,
            artist_names,
            releases,
            primary_release_id,
            cover,
        })
    }
}
