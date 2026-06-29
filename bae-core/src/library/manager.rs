//! Orchestrator layer: the one place where raw DB aggregates meet util
//! formatters and filesystem paths to produce resolved types.
//!
//! `LibraryManager` owns `library_dir` and the database handle. The
//! `resolve_*` helpers near the top of this file each take a `Db*` input
//! (from `crate::db::models`) and produce its resolved counterpart (in
//! `crate::album_detail`). Public methods return the resolved shapes —
//! `AlbumSummary`, `ReleaseStorageSummary`, `SearchResults`, `AlbumDetail`,
//! `ReleaseDetail` — never the raw `Db*` aggregates.
//!
//! Rule for additions: new DB-backed data flows through this layer. If you
//! need a new resolved shape, add the raw type to `crate::db::models`, the
//! resolved type to `crate::album_detail` (or a sibling like
//! `crate::queue`), and a `resolve_*` helper here.

use std::collections::HashMap;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use thiserror::Error;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::album_detail::{
    AlbumDetail, AlbumSearchResult, AlbumSummary, FileDetail, GalleryItem, GallerySource, ImageRef,
    ReleaseDetail, ReleaseStorageAction, ReleaseStorageSummary, ReleaseSummary, SearchResults,
    StorageFilter, StoragePage, StorageRow, StorageSort, StorageSortDirection, StorageSortField,
    TrackDetail, TrackGroup, TrackSearchResult,
};
use crate::config::{CloudProvider, ConfigHandle};
use crate::db::{
    Database, DbAlbum, DbAlbumArtist, DbAlbumSearchResult, DbAlbumSummary, DbArtist, DbAudioFormat,
    DbFile, DbImport, DbLibraryImage, DbLibrarySearchResults, DbRelease, DbReleaseStorageSummary,
    DbReleaseSummary, DbStorageRow, DbTrack, DbTrackArtist, DbTrackSearchResult,
    ImportOperationStatus, LibraryImageType, Pressing, SortDirection as DbSortDirection,
    StorageFilter as DbStorageFilter, StorageSortCriterion as DbStorageSortCriterion,
    StorageSortField as DbStorageSortField,
};
use crate::keys::BaeKeyServiceExt;
use crate::keys::KeyService;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::library::export::ExportService;
use crate::playback::QueueEntry;
use crate::queue::QueueItem;
use crate::storage::local::cleanup::{append_pending_deletions, PendingDeletion};
use crate::storage::local::ReleaseStorageImpl;
use crate::sync::sync_manager::S3ConfigData;
use coven::ClockRef;
#[cfg(any(test, feature = "test-utils"))]
use coven::CloudHome;
use coven::CovenHandle;
use coven::EncryptionService;
use coven::IdRef;
use coven::LibraryDir;
/// Comma-join artist names for display.
pub(crate) fn join_artist_names(artists: &[DbArtist]) -> String {
    artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build `PlaybackTrackInfo` given a track + release that have already been
/// loaded. Queries the album title + artists but reuses the track/release
/// passed in.
pub(crate) async fn playback_info_from_track_release(
    database: &Database,
    track: &DbTrack,
    release: &DbRelease,
) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
    // Cover comes from the track's own release so playing a non-primary
    // release shows that release's art, not the album-level primary.
    let cover_image_id = Some(track.release_id.clone());
    let album_id = release.album_id.clone();
    let album_title = match database.find_album_by_id(&album_id).await? {
        Some(album) => album.title,
        None => {
            warn!(
                "Album not found for track {} (album_id {})",
                track.id, album_id
            );
            String::new()
        }
    };

    let track_artists = database.get_artists_for_track(&track.id).await?;
    let (artist_id, artist_names) = if !track_artists.is_empty() {
        let id = track_artists[0].id.clone();
        let names = join_artist_names(&track_artists);
        (id, names)
    } else {
        let album_artists = database.get_artists_for_album(&album_id).await?;
        if album_artists.is_empty() {
            return Err(LibraryError::TrackMapping(format!(
                "no artist found for track {} album {}",
                track.id, album_id
            )));
        }
        let id = album_artists[0].id.clone();
        let names = join_artist_names(&album_artists);
        (id, names)
    };

    let side = crate::util::format::physical_side_medium(release.pressing.format.as_deref()).map(
        |medium| crate::playback::PlaybackTrackSide {
            medium,
            side_letter: crate::util::format::side_letter(track.side),
        },
    );

    Ok(crate::playback::PlaybackTrackInfo {
        track_id: track.id.clone(),
        track_title: track.title.clone(),
        artist_names,
        artist_id,
        album_id,
        album_title,
        cover_image_id,
        release_id: release.id.clone(),
        side,
    })
}

/// Produces a resolved `ReleaseStorageSummary` from a raw
/// `DbReleaseStorageSummary`: derives `storage_state` from `remote` and `pinned`
/// (the caller asks coven's cache whether the release's blobs are pinned). The raw
/// `primary_release_id` comes from SQL's `COALESCE(a.primary_release_id,
/// <first release id>)` and is non-null by construction: every album has at
/// least one release (enforced by `delete_release`).
fn resolve_release_storage_summary(
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
fn resolve_album_summary(
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

/// Resolve a raw release-summary aggregate: derives `storage_state` from
/// `remote` + this device's `release_local_copy` row.
/// `has_cloud_home` is read once by the caller and passed down (DI) so
/// `storage_actions` reflects whether remote storage exists at all.
/// `resolve_cover` maps the release's own id to its cover reference.
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
fn resolve_storage_row(
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

/// Translate a UI-facing `StorageSort` to the DB-layer sort criterion.
fn to_db_storage_sort(sort: &StorageSort) -> DbStorageSortCriterion {
    DbStorageSortCriterion {
        field: match sort.field {
            StorageSortField::AlbumTitle => DbStorageSortField::AlbumTitle,
            StorageSortField::ArtistNames => DbStorageSortField::ArtistNames,
            StorageSortField::Format => DbStorageSortField::Format,
            StorageSortField::FileCount => DbStorageSortField::FileCount,
            StorageSortField::TotalSize => DbStorageSortField::TotalSize,
        },
        direction: match sort.direction {
            StorageSortDirection::Ascending => DbSortDirection::Ascending,
            StorageSortDirection::Descending => DbSortDirection::Descending,
        },
    }
}

/// Translate a UI-facing `StorageFilter` to the DB-layer filter.
fn to_db_storage_filter(filter: StorageFilter) -> DbStorageFilter {
    match filter {
        StorageFilter::All => DbStorageFilter::All,
        StorageFilter::Remote => DbStorageFilter::Remote,
        StorageFilter::Local => DbStorageFilter::Local,
        StorageFilter::Uploading => DbStorageFilter::Uploading,
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

/// Outcome of `resolve_identity_target_album` — where a release should
/// land after a `set_identity` call. `new_album` carries the album row
/// to insert when the target is brand-new; otherwise the target is an
/// existing album and `new_album` is `None`.
struct IdentityTargetAlbum {
    album_id: String,
    new_album: Option<DbAlbum>,
}

/// Per-source agreement check: do `new_identities` fit alongside
/// `other_release_identities` (the identity rows of every *other*
/// release in the candidate album)?
///
/// Two releases can share an album as long as they don't disagree on
/// any source they both claim. `new_id.source == other.source` requires
/// matching `source_group_id`; differing sources are independent.
fn identities_fit_album(
    new_identities: &[crate::import::ReleaseIdentity],
    other_release_identities: &[Vec<crate::import::ReleaseIdentity>],
) -> bool {
    for new_id in new_identities {
        for other_release in other_release_identities {
            for existing in other_release {
                if existing.source == new_id.source
                    && existing.source_group_id != new_id.source_group_id
                {
                    return false;
                }
            }
        }
    }
    true
}

/// Project `MetadataPointer` to the two `releases` columns it sets:
/// `metadata_source` (always present) and `metadata_source_release_id`
/// (NULL when source is `file_tags`).
fn metadata_pointer_to_columns(
    pointer: crate::import::MetadataPointer,
) -> (crate::db::ReleaseMetadataSource, Option<String>) {
    use crate::db::ReleaseMetadataSource;
    use crate::import::{MetadataPointer, MetadataSource};
    match pointer {
        MetadataPointer::External { source, release_id } => {
            let column_source = match source {
                MetadataSource::MusicBrainz => ReleaseMetadataSource::MusicBrainz,
                MetadataSource::Discogs => ReleaseMetadataSource::Discogs,
            };
            (column_source, Some(release_id))
        }
        MetadataPointer::FileTags => (ReleaseMetadataSource::FileTags, None),
    }
}

/// Project cached MusicBrainz `release_metadata` rows back into a
/// `ParsedAlbum`. Replays what `commit_mb_release` did at import,
/// minus the network calls — uses whatever the importer archived in
/// `release_metadata` (the MB release JSON, optional cross-linked
/// Discogs release JSON).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_musicbrainz_from_cache(
    database: &Database,
    release_id: &str,
    source_release_id: &str,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let pairs = database.get_release_metadata_by_source(release_id).await?;
    let mb_json = pairs.get("musicbrainz").ok_or_else(|| {
        LibraryError::Import(format!(
            "no cached MusicBrainz payload for release '{release_id}' (source release {source_release_id})"
        ))
    })?;
    let response: crate::musicbrainz::MbReleaseResponse =
        serde_json::from_str(mb_json).map_err(|e| {
            LibraryError::Import(format!("failed to parse cached MusicBrainz JSON: {e}"))
        })?;

    // The cached payload may belong to an earlier pressing if `set_identity`
    // redirected `metadata_source_release_id` without re-fetching. Refuse to
    // project stale data — caller must re-fetch (e.g. via Re-identify) first.
    if response.id != source_release_id {
        return Err(LibraryError::Import(format!(
            "cached MusicBrainz payload (release '{}') doesn't match current pointer '{}'; re-fetch via Re-identify first",
            response.id, source_release_id
        )));
    }

    let discogs_release = match pairs.get("discogs") {
        Some(json) => Some(
            crate::discogs::client::parse_discogs_release_json(json).map_err(|e| {
                LibraryError::Import(format!(
                    "failed to parse cached Discogs cross-ref JSON: {e}"
                ))
            })?,
        ),
        None => None,
    };

    crate::import::musicbrainz_mapper::map_mb_response_to_db(
        &response,
        None,
        discogs_release,
        clock,
        ids,
    )
    .map_err(LibraryError::Import)
}

/// Project cached Discogs `release_metadata` rows back into a
/// `ParsedAlbum`. Replays the import-time projection from the archived
/// raw JSON (Discogs release + optional master + optional MB cross-ref).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_discogs_from_cache(
    database: &Database,
    release_id: &str,
    source_release_id: &str,
    clock: &dyn coven::Clock,
    ids: &dyn coven::IdProvider,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let pairs = database.get_release_metadata_by_source(release_id).await?;
    let discogs_json = pairs.get("discogs").ok_or_else(|| {
        LibraryError::Import(format!(
            "no cached Discogs payload for release '{release_id}' (source release {source_release_id})"
        ))
    })?;
    let release = crate::discogs::client::parse_discogs_release_json(discogs_json)
        .map_err(|e| LibraryError::Import(format!("failed to parse cached Discogs JSON: {e}")))?;

    // The cached payload may belong to an earlier pressing if `set_identity`
    // redirected `metadata_source_release_id` without re-fetching. Refuse to
    // project stale data — caller must re-fetch (e.g. via Re-identify) first.
    if release.id != source_release_id {
        return Err(LibraryError::Import(format!(
            "cached Discogs payload (release '{}') doesn't match current pointer '{}'; re-fetch via Re-identify first",
            release.id, source_release_id
        )));
    }

    let master_year = match pairs.get("discogs_master") {
        Some(json) => crate::discogs::client::parse_discogs_master_year(json).map_err(|e| {
            LibraryError::Import(format!("failed to parse cached Discogs master JSON: {e}"))
        })?,
        None => release.year,
    };

    let mb_xref = match pairs.get("musicbrainz") {
        Some(json) => Some(
            serde_json::from_str::<crate::musicbrainz::MbReleaseResponse>(json).map_err(|e| {
                LibraryError::Import(format!(
                    "failed to parse cached MusicBrainz cross-ref JSON: {e}"
                ))
            })?,
        ),
        None => None,
    };

    crate::import::discogs_mapper::map_discogs_to_db(
        &release,
        master_year,
        mb_xref.as_ref(),
        clock,
        ids,
    )
    .map_err(LibraryError::Import)
}

/// Project the embedded tags of a release's local audio files into a
/// `ParsedAlbum`. Mirrors the Unknown import path's call to
/// `map_file_tags_to_db`. Errors out if any audio file is unreachable on
/// disk (cloud-only release without a local copy).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn project_file_tags(
    database: &Database,
    release: &DbRelease,
    clock: ClockRef,
    ids: IdRef,
) -> Result<crate::import::ParsedAlbum, LibraryError> {
    let files = database.get_files_for_release(&release.id).await?;
    let mut audio_paths = Vec::new();
    for file in &files {
        if !file.content_type.is_audio() {
            continue;
        }
        // The file's bytes must be the user's own file in place (a Local
        // user-provided blob coven holds an external ref for); a Remote release
        // has no on-disk original to re-read tags from.
        let path = database
            .external_blob(&file.id)
            .await?
            .map(|ext| ext.path)
            .ok_or_else(|| {
            LibraryError::Import(format!(
                "audio file '{}' is remote — make the release local before resetting from file tags",
                file.original_filename
            ))
        })?;
        audio_paths.push(path);
    }
    if audio_paths.is_empty() {
        return Err(LibraryError::Import(format!(
            "release '{}' has no audio files to read tags from",
            release.id
        )));
    }
    // Album-title fallback when no file carries an ALBUM tag: the folder the
    // release was originally imported from.
    let folder_name = release.source_folder_name.clone();
    tokio::task::spawn_blocking(move || {
        crate::import::file_tag_mapper::map_file_tags_to_db(
            &audio_paths,
            folder_name.as_deref(),
            clock.as_ref(),
            ids.as_ref(),
        )
    })
    .await
    .map_err(|e| LibraryError::Import(format!("file-tag mapping task failed: {e}")))?
    .map_err(LibraryError::Import)
}

/// Resolve the raw search-result container by mapping each inner list.
/// `covers` maps an album's primary release id to its cover reference.
fn resolve_search_results(
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

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] coven::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Import error: {0}")]
    Import(String),
    #[error("Track mapping error: {0}")]
    TrackMapping(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] coven::EncryptionError),
    #[error("Storage error: {0}")]
    Storage(String),
}

/// All DB data needed to play or serve a track.
///
/// Internal aggregate used by `resolve_track_audio` and carried inside
/// `ExportTrackPlan` for the export decoder. Callers that only need resolved
/// playback data should use `ResolvedTrackAudio` instead — the export path
/// still needs raw audio-format fields (byte ranges, seektable, CUE sample
/// bounds) for whole-file decode, so the raw shape stays here as `pub(crate)`.
pub(crate) struct TrackAudioMeta {
    pub track: DbTrack,
    pub release: DbRelease,
    pub audio_format: DbAudioFormat,
    pub audio_file: DbFile,
    /// File id backing this track's audio. Always `Some` coming out of `resolve`
    /// — the `audio_format.file_id` invariant is checked there, so downstream
    /// code can read this directly without re-validating.
    pub file_id: String,
}

impl TrackAudioMeta {
    /// Resolve track metadata from the Database.
    pub(crate) async fn resolve(database: &Database, track_id: &str) -> Result<Self, LibraryError> {
        let track = database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;

        let audio_format = database
            .find_audio_format_by_track_id(track_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("No audio format for track: {}", track_id))
            })?;

        let release = database.get_release_for_track(&track).await?;

        let file_id = audio_format.file_id.clone().ok_or_else(|| {
            LibraryError::TrackMapping(format!(
                "No file_id in audio_format for track: {}",
                track_id
            ))
        })?;

        let audio_file = database.find_file_by_id(&file_id).await?.ok_or_else(|| {
            LibraryError::TrackMapping(format!("Audio file not found: {}", file_id))
        })?;

        Ok(Self {
            track,
            release,
            audio_format,
            audio_file,
            file_id,
        })
    }
}

/// All resolved data needed to set up a playback reader for a track.
///
/// Returned by `LibraryManager::resolve_track_audio` — no raw `Db*` types
/// exposed. The track's sample window is resolved from the stored bounds, so the
/// playback service never reads raw audio format fields.
pub struct ResolvedTrackAudio {
    pub track_id: String,
    pub release_id: String,
    /// File id backing this track's audio data. The blob id coven addresses this
    /// file by (in the `release_files` namespace), and the playback cache key.
    pub file_id: String,
    /// The readable cloud-relative key for this file's blob (`release_files.cloud_path`):
    /// a readable path on a browsable home, `None` for the hashed-by-id default on
    /// an opaque one. Carried into the `BlobRef` the playback reader streams through
    /// coven; coven resolves the locality (external ref / local store / cache / cloud).
    pub cloud_path: Option<String>,
    pub file_size: u64,
    pub duration_ms: Option<i64>,
    pub pregap_ms: Option<i64>,
    pub sample_rate: u32,
    pub channels: u32,
    /// This track's sample window in its backing file: `start_sample` is 0 for a
    /// whole-file track, `end_sample` is `None` when the track runs to EOF.
    pub start_sample: u64,
    pub end_sample: Option<u64>,
    /// One past this track's last byte in its backing file (frame-granular, from
    /// import; `None` when the track runs to EOF / a whole-file track). Playback
    /// buffers the rest of the current track up to here instead of a fixed window.
    pub end_byte: Option<u64>,
    /// Raw loudness/peak measurements (LUFS + linear peak) for this track and its
    /// album, as stored at import. `None` = not measured. Playback derives the
    /// replay gain from these against a constant target; nothing here is a gain.
    pub track_loudness_lufs: Option<f64>,
    pub track_peak_linear: Option<f64>,
    pub album_loudness_lufs: Option<f64>,
    pub album_peak_linear: Option<f64>,
}

impl ResolvedTrackAudio {
    /// Build a resolved view of a track's audio from raw DB records. coven owns the
    /// locality resolution at read time (external ref / local store / cache /
    /// cloud), so this carries only the blob's identity (`file_id` + `cloud_path`)
    /// and the playback parameters — not a resolved read source.
    pub(crate) fn from_meta(meta: &TrackAudioMeta) -> Self {
        Self {
            track_id: meta.track.id.clone(),
            release_id: meta.track.release_id.clone(),
            file_id: meta.file_id.clone(),
            cloud_path: meta.audio_file.cloud_path.clone(),
            file_size: meta.audio_file.file_size as u64,
            duration_ms: meta.track.duration_ms,
            pregap_ms: meta.audio_format.pregap_ms,
            sample_rate: meta.audio_format.sample_rate as u32,
            channels: meta.audio_format.channels as u32,
            start_sample: meta.audio_format.start_sample as u64,
            end_sample: meta.audio_format.end_sample.map(|s| s as u64),
            end_byte: meta.audio_format.end_byte.map(|b| b as u64),
            track_loudness_lufs: meta.audio_format.track_loudness_lufs,
            track_peak_linear: meta.audio_format.track_peak_linear,
            album_loudness_lufs: meta.release.album_loudness_lufs,
            album_peak_linear: meta.release.album_peak_linear,
        }
    }

    /// Linear playback gain for this track under `mode`. `1.0` = no change.
    ///
    /// The gain is a view of (stored measurements, mode, target) — never stored.
    /// `Off` plays at unity. `Track`/`Album` pick that level's `(loudness, peak)`,
    /// falling back to the other level when the preferred one wasn't measured,
    /// and to unity when neither was (NULL measurements, or a silent track). For
    /// the chosen level the gain brings the measured loudness to the target, then
    /// is capped at `1.0/peak` so a boosted track can't clip.
    pub fn replay_gain_linear(&self, mode: crate::config::ReplayGainMode) -> f32 {
        use crate::config::ReplayGainMode;

        let track = self.track_loudness_lufs.zip(self.track_peak_linear);
        let album = self.album_loudness_lufs.zip(self.album_peak_linear);

        let chosen = match mode {
            ReplayGainMode::Off => None,
            ReplayGainMode::Track => track.or(album),
            ReplayGainMode::Album => album.or(track),
        };

        let Some((loudness_lufs, peak_linear)) = chosen else {
            return 1.0;
        };

        let gain = 10f64.powf((REPLAY_GAIN_TARGET_LUFS - loudness_lufs) / 20.0);
        // Cap the gain so the loudest true-peak sample can't exceed full scale.
        // A non-positive peak (no usable peak) imposes no cap.
        let max_safe = if peak_linear > 0.0 {
            1.0 / peak_linear
        } else {
            f64::INFINITY
        };
        gain.min(max_safe) as f32
    }
}

/// Target playback loudness the replay gain aims each track/album at, in LUFS.
/// A constant in this change; becomes a user setting alongside the picker.
/// -18 LUFS is a common reference for quiet-listening normalization.
const REPLAY_GAIN_TARGET_LUFS: f64 = -18.0;

/// Resolved context for starting playback of a track: everything the
/// playback service needs to set up the queue without chasing back into
/// the library for neighbouring track IDs.
///
/// The release a directly-selected track plays from: the full track order (by
/// `side, track_number, id`) and the selected track's index into it. The
/// playback service seeds a context from this.
pub struct PlayContext {
    pub release_id: String,
    pub track_ids: Vec<String>,
    pub index: usize,
}

/// Tag fields to embed on an exported track file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ExportTags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// Disc number for multi-disc releases. `None` when the release is
    /// single-disc — we don't write a disc tag in that case.
    pub disc: Option<i32>,
}

/// Pre-assembled data for exporting a single track. Everything
/// `ExportService::export_track` needs comes out of a single
/// `LibraryManager::get_export_track_plan` call; the export service
/// never chases back into the database to resolve tags, paths, or
/// neighbours.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct ExportTrackPlan {
    /// The track's source audio file, read at plan time: from this device's
    /// local copy when one exists, otherwise downloaded from the cloud home
    /// and decrypted with the release's item key. Length-verified against
    /// the file row either way.
    pub(crate) audio_bytes: Vec<u8>,
    pub tags: ExportTags,
    /// The cover image bytes to embed, read through coven at plan time, or `None`
    /// when the album has no primary release with a cover.
    pub cover_image_bytes: Option<Vec<u8>>,
    /// Track number within its side, straight from the DB.
    pub track_number: Option<i32>,
    /// Number of tracks in the release — used for the track-total tag.
    pub total_tracks: usize,
    /// `true` for CD / digital / unknown releases, `false` for side-based
    /// media like vinyl or cassette. Gates writing an ID3 disc-number tag:
    /// disc numbers don't map to vinyl / cassette sides.
    pub is_digital: bool,
    /// Raw audio-format aggregate. Held internally so `ExportService::export_track`
    /// can decode CUE-split byte ranges / APE sample bounds without re-resolving.
    pub(crate) audio_meta: TrackAudioMeta,
}

/// Which image to use when changing an album's cover art.
pub enum CoverSelection {
    /// Use an image file already in the library.
    ReleaseImage { file_id: String },
    /// Download from a remote URL.
    RemoteCover {
        url: String,
        source: crate::import::MetadataSource,
    },
}

/// Verb for a storage transition, used to name the operation in the "ended
/// without completion" guard.
fn verb(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Pin",
        ReleaseStorageAction::Unpin => "Unpin",
        ReleaseStorageAction::MakeRemote => "Manage",
        ReleaseStorageAction::MakeLocal => "Unmanage",
    }
}

/// Emits `ReleaseTransferEnded` on drop unless defused. `drive_transfer` holds
/// one so an aborted transfer future (its bridge wrapper is dropped mid-flight)
/// still clears the UI's transfer indicator; the normal exit defuses it after
/// emitting the event itself. The broadcast send is synchronous, so `Drop` can
/// fire it directly.
struct TransferEndedGuard {
    event_tx: broadcast::Sender<LibraryEvent>,
    release_id: String,
    armed: bool,
}

impl TransferEndedGuard {
    fn defuse(&mut self) {
        self.armed = false;
    }
}

impl Drop for TransferEndedGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Err(err) = self.event_tx.send(LibraryEvent::ReleaseTransferEnded {
            release_id: std::mem::take(&mut self.release_id),
        }) {
            warn!("ReleaseTransferEnded on abort had no subscribers: {err}");
        }
    }
}

/// Removes a release's transfer cancellation token from the registry when the
/// transfer ends — whether it completes normally or its future is dropped (a
/// view dismiss), so a dropped transfer never leaves a stale token behind.
struct TransferCancelGuard {
    registry: Arc<Mutex<HashMap<String, crate::library::CancellationToken>>>,
    release_id: String,
}

impl Drop for TransferCancelGuard {
    fn drop(&mut self) {
        self.registry.lock().unwrap().remove(&self.release_id);
    }
}

/// Events emitted by LibraryManager when data changes.
///
/// Album-level and release-level events are mutually exclusive for the same
/// album in the same mutation: each mutation site emits exactly one event per
/// affected album. `AlbumAdded` includes the first release in its payload;
/// `ReleaseAdded` only fires when the album already exists.
#[derive(Clone, Debug)]
pub enum LibraryEvent {
    // ── Album-level (fat: carry the full album payload) ───────────
    AlbumAdded {
        album: AlbumDetail,
    },
    AlbumUpdated {
        album: AlbumDetail,
    },
    AlbumRemoved {
        album_id: String,
        /// Ids of the album's child releases to drop alongside it.
        release_ids: Vec<String>,
    },

    // ── Release-level (carry the release + parent album) ─────────
    ReleaseAdded {
        album: AlbumSummary,
        release: ReleaseDetail,
    },
    ReleaseUpdated {
        album_id: String,
        release: ReleaseDetail,
    },
    ReleaseRemoved {
        album_id: String,
        release_id: String,
        /// Parent album's post-removal summary, so the reducer interns it
        /// instead of patching its `release_ids` by reading the old list.
        /// `None` when the album itself was removed with its last release.
        album: Option<AlbumSummary>,
    },

    // ── Retained (not about library data shape) ──────────────────
    TracksDeleted {
        track_ids: Vec<String>,
    },
    Error {
        error: crate::ui::UiError,
    },
    /// Sync loop's latest error state. `None` clears a prior failure (sync
    /// recovered). Emitted on transitions so the UI banner appears and
    /// disappears in step with sync health. When set, it's a
    /// `UiError::Diagnostic` whose category keys the generic line and whose
    /// detail is the opaque, log-only error chain.
    SyncError {
        error: Option<crate::ui::UiError>,
    },
    /// Wall-clock time the last sync cycle completed successfully, as Unix
    /// epoch milliseconds. Emitted on transitions so the sidebar's "Last
    /// synced …" subtitle updates after every cycle and stays stable in
    /// between.
    SyncTimeChanged {
        time: Option<i64>,
    },
    /// Whether the sync loop is currently mid-cycle. Emitted on transitions so
    /// the sidebar can show a spinner over the active library row from when a
    /// "Sync Now" kicks the loop to when it idles again.
    SyncingChanged {
        syncing: bool,
    },
    /// The cloud outbox changed — carries the full processing snapshot so the
    /// Storage Manager re-renders its queue panel.
    OutboxChanged {
        snapshot: crate::library::OutboxSnapshot,
    },
    /// A pin/unpin/manage/unmanage transition advanced. `percent` is the
    /// overall release progress (combined across files, computed in core);
    /// `label` is a ready-to-render line. The UI shows a determinate bar until
    /// the matching `ReleaseTransferEnded` arrives.
    ReleaseTransferProgress {
        release_id: String,
        action: ReleaseStorageAction,
        file_no: Option<u32>,
        total: Option<u32>,
        percent: u8,
    },
    /// A transition finished (success OR failure) — the UI clears its transfer
    /// indicator. On failure the user-facing reason still arrives via the
    /// thrown error from the transfer call.
    ReleaseTransferEnded {
        release_id: String,
    },
    /// The in-memory download (pin) queue changed — carries the full snapshot
    /// so the Storage Manager re-renders its Downloads pane and storage rows
    /// re-read their per-release "Downloading" badge.
    DownloadQueueChanged {
        snapshot: crate::library::DownloadSnapshot,
    },
}
/// The main library manager for database operations and entity persistence
///
/// Handles:
/// - Album/track/file persistence
/// - State transitions (importing -> complete/failed)
/// - Query methods for library browsing
/// - Deletion with cloud storage cleanup
pub struct LibraryManager {
    database: Database,
    library_dir: LibraryDir,
    config_handle: Arc<ConfigHandle>,
    key_service: KeyService,
    clock: ClockRef,
    ids: IdRef,
    runtime_handle: tokio::runtime::Handle,
    /// The one coven data handle: it owns the database connection, the library
    /// directory, the keys, the cloud storage, and — once a provider is connected
    /// — the sync manager. Every blob read/write, every locality transition, and
    /// the whole sync lifecycle route through it; bae passes only descriptors
    /// (a `BlobRef`, a root id) and never assembles coven's internals by hand.
    handle: CovenHandle,
    event_tx: broadcast::Sender<LibraryEvent>,
    /// `file_id`s whose upload is in flight right now, mapped to the live count
    /// of encrypted bytes that have reached the cloud for that file. Shared with
    /// the sync loop's `ReleaseUploadObserver`, which inserts on upload start,
    /// advances the byte count from coven's mid-upload progress callback, and
    /// removes on completion/failure; read by `outbox_snapshot` to mark the
    /// "uploading" state and drive the per-file bar. Transient (empty after a
    /// restart).
    outbox_in_flight: Arc<Mutex<HashMap<String, u64>>>,
    /// Rolling-window upload-throughput tracker. The observer records bytes
    /// on every successful upload; the snapshot builder reads the rate.
    /// Transient (zeroed after a restart, like `outbox_in_flight`).
    upload_throughput: Arc<crate::library::UploadThroughput>,
    /// User-driven pause flag for the cloud-upload pipeline. When true,
    /// coven's `drain_uploads` short-circuits before each entry, so the
    /// queue stops draining but in-flight uploads (and new enqueues) keep
    /// flowing. Transient (Running after a restart).
    sync_paused: Arc<std::sync::atomic::AtomicBool>,
    /// Whether a sync manager is installed in the coven handle. Public coven no
    /// longer exposes the manager itself, so bae records the connection fact at
    /// the points where it calls connect/disconnect.
    sync_connected: Arc<std::sync::atomic::AtomicBool>,
    /// The active encryption service for opaque homes, kept for UI state and
    /// tests that seed encrypted fixtures. Browsable homes leave this empty.
    encryption_service: Arc<RwLock<Option<EncryptionService>>>,
    /// Cancellation tokens for in-progress foreground transfers (unmanage),
    /// keyed by release id. `cancel_release_transition` fires the token; the
    /// transfer observes it between files, deletes the partial copies it wrote,
    /// and leaves the release remote (no orphans). Registered for the transfer's
    /// duration; transient.
    transfer_cancels: Arc<Mutex<HashMap<String, crate::library::CancellationToken>>>,
    /// In-memory queue for "Pin for offline". A single serial worker drains it
    /// one release at a time. Shared across manager clones; transient (empty
    /// after a restart — a release that wasn't fully pinned stays cloud-only).
    download_queue: Arc<crate::library::DownloadQueue>,
    /// Test-only injection points for the cloud read/write paths, so tests
    /// resolve the cloud home and the sync-ready gate without standing up a live
    /// `SyncManager`.
    #[cfg(any(test, feature = "test-utils"))]
    test_overrides: TestOverrides,
}

/// Test-only overrides for state that production reads from a live
/// `SyncManager`. The cloud read/write paths run against a real manager a test
/// connects via [`LibraryManager::connect_test_cloud_home`], so this holds only
/// the cleanup-delay knob.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Clone, Default)]
struct TestOverrides {
    /// Delay before the deferred storage-cleanup drain runs. Production uses
    /// the fixed `CLEANUP_DELAY`; tests set a near-zero delay so a delete's
    /// scheduled drain is observable without waiting the production interval.
    cleanup_delay: Option<std::time::Duration>,
}

impl std::fmt::Debug for LibraryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryManager")
            .field("database", &self.database)
            .finish_non_exhaustive()
    }
}

impl Clone for LibraryManager {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            library_dir: self.library_dir.clone(),
            config_handle: self.config_handle.clone(),
            key_service: self.key_service.clone(),
            clock: self.clock.clone(),
            ids: self.ids.clone(),
            runtime_handle: self.runtime_handle.clone(),
            handle: self.handle.clone(),
            event_tx: self.event_tx.clone(),
            outbox_in_flight: self.outbox_in_flight.clone(),
            upload_throughput: self.upload_throughput.clone(),
            sync_paused: self.sync_paused.clone(),
            sync_connected: self.sync_connected.clone(),
            encryption_service: self.encryption_service.clone(),
            transfer_cancels: self.transfer_cancels.clone(),
            download_queue: self.download_queue.clone(),
            #[cfg(any(test, feature = "test-utils"))]
            test_overrides: self.test_overrides.clone(),
        }
    }
}
impl LibraryManager {
    /// Open coven through the top-level builder and create the library manager
    /// over the resulting handle.
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        config_handle: Arc<ConfigHandle>,
        key_service: KeyService,
        clock: ClockRef,
        ids: IdRef,
        runtime_handle: tokio::runtime::Handle,
    ) -> Result<Self, coven::DbError> {
        let (event_tx, _) = broadcast::channel(16);
        let outbox_in_flight = Arc::new(Mutex::new(HashMap::new()));
        let upload_throughput = Arc::new(crate::library::UploadThroughput::new());
        let sync_paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sync_connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let encryption_service = Arc::new(RwLock::new(None));

        let observer = Arc::new(crate::sync::upload_observer::ReleaseUploadObserver::new(
            outbox_in_flight.clone(),
            upload_throughput.clone(),
            sync_paused.clone(),
            event_tx.clone(),
        ));
        let ch = Arc::clone(&config_handle);
        let config_provider = move || ch.config().to_coven();
        let handle = coven::Coven::builder(config_provider)
            .synced_tables(crate::sync::synced_tables())
            .clock(clock.clone())
            .key_service(key_service.clone())
            .observer(observer.clone() as Arc<dyn coven::BlobTransitionObserver>)
            .migrations(crate::migrations::all())
            .open()
            .map_err(|e| match e {
                coven::CovenError::Database(error) => error,
                other => coven::DbError(other.to_string()),
            })?;
        let database = Database::from_handle(handle.clone(), clock.clone());
        observer.set_database(Arc::new(database.clone()));
        observer.set_handle(handle.clone());
        let library_dir = config_handle.config().library_dir.clone();

        Ok(LibraryManager {
            database,
            library_dir,
            config_handle,
            key_service,
            clock,
            ids,
            runtime_handle,
            handle,
            event_tx,
            outbox_in_flight,
            upload_throughput,
            sync_paused,
            sync_connected,
            encryption_service,
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(any(test, feature = "test-utils"))]
            test_overrides: TestOverrides::default(),
        })
    }

    /// Create a library manager over an already-open database handle. Production
    /// uses [`Self::open`] so the upload observer is installed into coven before
    /// sync starts; this constructor remains for tests that exercise database-only
    /// manager behavior.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        database: Database,
        library_dir: LibraryDir,
        config_handle: Arc<ConfigHandle>,
        key_service: KeyService,
        clock: ClockRef,
        ids: IdRef,
        runtime_handle: tokio::runtime::Handle,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(16);
        let outbox_in_flight = Arc::new(Mutex::new(HashMap::new()));
        let upload_throughput = Arc::new(crate::library::UploadThroughput::new());
        let sync_paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = database.handle().clone();

        LibraryManager {
            database,
            library_dir,
            config_handle,
            key_service,
            clock,
            ids,
            runtime_handle,
            handle,
            event_tx,
            outbox_in_flight,
            upload_throughput,
            sync_paused,
            sync_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            encryption_service: Arc::new(RwLock::new(None)),
            transfer_cancels: Arc::new(Mutex::new(HashMap::new())),
            download_queue: Arc::new(crate::library::DownloadQueue::new()),
            #[cfg(any(test, feature = "test-utils"))]
            test_overrides: TestOverrides::default(),
        }
    }

    /// Connect a real `SyncManager` over an injected cloud home for tests, so the
    /// handle's make-Remote / make-Local / upload-drain / read paths all run
    /// against a mock cloud with no live provider — the test counterpart of
    /// `attach_and_start_sync`. `cipher` is the home's at-rest protection:
    /// `Plaintext` for a browsable mock, `Encrypted(service)` for an opaque one.
    /// After this, `has_cloud_home`, `get_encryption_service`, and `is_sync_ready`
    /// all resolve off the connected manager, no override needed.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn connect_test_cloud_home(
        &self,
        cloud_home: Arc<dyn CloudHome>,
        cipher: crate::sync::CloudCipher,
    ) -> Result<(), String> {
        let active_encryption = match &cipher {
            crate::sync::CloudCipher::Encrypted(service) => Some(service.clone()),
            crate::sync::CloudCipher::Plaintext => None,
        };
        self.handle
            .connect_sync_with_test_home(cloud_home, cipher)
            .await?;
        *self.encryption_service.write().unwrap() = active_encryption;
        self.sync_connected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Shorten the deferred storage-cleanup delay so a scheduled drain runs
    /// promptly under test (production waits `CLEANUP_DELAY`).
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_cleanup_delay(&mut self, delay: std::time::Duration) {
        self.test_overrides.cleanup_delay = Some(delay);
    }

    /// Set the cloud home's storage mode in config, so a test can exercise the
    /// browsable read/write paths against an injected cloud home (production sets
    /// this through the cloud-setup wizard). The connected home's cipher must match
    /// — `Plaintext` for browsable, `Encrypted` for opaque.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn set_home_storage(&self, storage: crate::config::HomeStorage) {
        self.config_handle
            .update(|c| c.cloud_home.storage = storage)
            .expect("set test home storage mode");
    }

    /// The cloud object key the read path resolves for a remote file: the row's
    /// stored `cloud_path` (readable on a browsable home) or the hashed
    /// `storage_path(id)` default. Mirrors what `ResolvedTrackAudio` threads into
    /// the cloud reader, exposed so a test can assert the read key matches the
    /// stored upload key without setting up full playback.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn resolve_track_cloud_key_for_test(&self, file_id: &str) -> String {
        let file = self
            .database
            .find_file_by_id(file_id)
            .await
            .expect("file lookup")
            .expect("file exists");
        self.release_file_cloud_key(&file).expect("cloud key")
    }

    /// The readable cover `cloud_path` the current home would store for a
    /// release: `Some({artist}/{album}/cover.{ext})` on a browsable home, `None`
    /// on an opaque one. Exposes the same computation `change_cover` performs.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn cover_cloud_path_for_test(
        &self,
        release_id: &str,
        content_type: &crate::util::content_type::ContentType,
    ) -> Option<String> {
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .cover_cloud_path_for_storage(storage, release_id, content_type)
            .await
            .expect("compute cover cloud path")
    }

    /// Spawn the deferred drain of the pending-deletions manifest. Every
    /// deletion entry point (delete_release/delete_album/unpin/unmanage) routes
    /// through here so the queued local `storage/` copies are actually removed.
    fn spawn_cleanup(&self) {
        #[cfg(any(test, feature = "test-utils"))]
        if let Some(delay) = self.test_overrides.cleanup_delay {
            crate::storage::local::cleanup::schedule_cleanup_after(&self.library_dir, delay);
            return;
        }
        crate::storage::local::cleanup::schedule_cleanup(&self.library_dir);
    }

    // =========================================================================
    // Internal accessors (pub(crate) — being phased out in favour of domain methods)
    // =========================================================================

    fn sync_connected(&self) -> bool {
        self.sync_connected
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The injected wall clock. The import layer and the mappers read "now"
    /// through this so the whole import shares one clock under test.
    pub(crate) fn clock(&self) -> &ClockRef {
        &self.clock
    }

    /// The injected id source. The import layer and the mappers mint row ids
    /// through this so tests get a deterministic-but-unique sequence; the
    /// playback queue mints per-instance `QueueEntryId`s through it on every
    /// platform.
    pub(crate) fn ids(&self) -> &IdRef {
        &self.ids
    }

    /// The library's on-disk directory. The import layer reads/writes its
    /// sibling appdata files (e.g. the watched-folder registry) under it.
    /// Desktop-only: the import module that uses it is gated off iOS/Android,
    /// and playback reads blobs through coven's handle rather than this path.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn library_dir(&self) -> &LibraryDir {
        &self.library_dir
    }

    fn encryption_service_inner(&self) -> Option<EncryptionService> {
        self.encryption_service.read().unwrap().clone()
    }

    /// Start background listeners (sync status → library events).
    ///
    /// Call once after construction when a tokio runtime is available.
    /// Subscribes to sync loop status and emits granular library events
    /// for any entity changes from applied changesets.
    pub fn start(&self) {
        if let Ok(mut rx) = self.handle.subscribe_sync_status() {
            let lm = self.clone();
            self.runtime_handle.spawn(async move {
                let mut last_error: Option<String> = None;
                let mut last_sync_time: Option<String> = None;
                let mut last_syncing: bool = false;
                while let Ok(status) = rx.recv().await {
                    if let Some(row_changes) = status.row_changes {
                        let changes =
                            crate::library::sync_events::changes_from_row_changes(&row_changes);
                        lm.emit_sync_entity_changes(changes).await;
                    }
                    if status.error != last_error {
                        last_error = status.error.clone();
                        // Coven hands back an opaque error string (connectivity,
                        // auth, storage); the UI shows a generic line plus this
                        // as copyable, log-only detail. `None` clears the banner.
                        lm.emit(LibraryEvent::SyncError {
                            error: status.error.map(crate::ui::UiError::internal),
                        });
                    }
                    if status.syncing != last_syncing {
                        last_syncing = status.syncing;
                        lm.emit(LibraryEvent::SyncingChanged {
                            syncing: status.syncing,
                        });
                    }
                    if status.last_sync_time != last_sync_time {
                        last_sync_time = status.last_sync_time.clone();
                        // coven reports the time as an RFC 3339 string;
                        // the UI only needs an instant, so emit epoch
                        // millis. A value that won't parse is a bug (coven
                        // writes valid RFC 3339), so log it and emit `None`
                        // rather than masking it as "never synced".
                        let time = status.last_sync_time.as_deref().and_then(|s| {
                            match crate::config::rfc3339_to_epoch_millis(s) {
                                Ok(ms) => Some(ms),
                                Err(e) => {
                                    warn!("unparseable last_sync_time {s:?}: {e}");
                                    None
                                }
                            }
                        });
                        lm.emit(LibraryEvent::SyncTimeChanged { time });
                    }
                    // coven gives no per-item drain signal in the status,
                    // so re-derive the outbox snapshot each cycle to catch
                    // entries it uploaded or failed.
                    lm.emit_outbox_changed().await;
                }
            });
        }
    }

    /// Subscribe to library events (albums changed, etc.)
    pub fn subscribe_events(&self) -> broadcast::Receiver<LibraryEvent> {
        self.event_tx.subscribe()
    }

    /// Emit a library event to all subscribers. Logs at warn-level when no
    /// subscribers remain — the bus is alive for the lifetime of the library
    /// so empty subscribers is unusual and worth a trace.
    fn emit(&self, event: LibraryEvent) {
        if let Err(err) = self.event_tx.send(event) {
            warn!("library event broadcast had no subscribers: {err}");
        }
    }

    /// Build the current outbox snapshot and emit it as `OutboxChanged`. Called
    /// at every outbox mutation, once per sync cycle, and on each upload
    /// lifecycle callback so the Storage Manager's queue panel stays current.
    pub(crate) async fn emit_outbox_changed(&self) {
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        match crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.database,
            &in_flight,
            &self.upload_throughput,
            paused,
        )
        .await
        {
            Ok(snapshot) => self.emit(LibraryEvent::OutboxChanged { snapshot }),
            Err(e) => warn!("Failed to build outbox snapshot: {e}"),
        }
    }

    /// Build the current download-queue snapshot and emit it as
    /// `DownloadQueueChanged`. Called at every queue mutation (enqueue,
    /// worker pick-up, per-file progress, success, failure, cancel, retry,
    /// pause/resume) so the Storage Manager's Downloads pane stays current.
    pub(crate) fn emit_download_queue_changed(&self) {
        self.emit(LibraryEvent::DownloadQueueChanged {
            snapshot: self.download_snapshot(),
        });
    }

    /// The current download-queue snapshot — per-release state and a
    /// pre-formatted summary, built from the in-memory queue. Seeds the
    /// Downloads pane before the first `DownloadQueueChanged` event arrives.
    pub fn download_snapshot(&self) -> crate::library::DownloadSnapshot {
        crate::library::download_snapshot::build_download_snapshot(
            &self.download_queue.ops(),
            self.download_queue.is_paused(),
        )
    }

    /// The current outbox processing snapshot — queue depth, per-item state, and
    /// a pre-formatted summary. Seeds the Storage Manager panel before the first
    /// `OutboxChanged` event arrives.
    pub async fn outbox_snapshot(&self) -> Result<crate::library::OutboxSnapshot, LibraryError> {
        let in_flight = { self.outbox_in_flight.lock().unwrap().clone() };
        let paused = self.is_sync_paused();
        Ok(crate::library::outbox_snapshot::build_outbox_snapshot(
            &self.database,
            &in_flight,
            &self.upload_throughput,
            paused,
        )
        .await?)
    }

    // ── Fat-event emit helpers ───────────────────────────────────────
    // Each reads the current state of the entity post-mutation from the DB
    // and packs it into the event payload.

    pub async fn emit_album_added(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => {
                info!("emit_album_added: emitting AlbumAdded for album {album_id}");
                self.emit(LibraryEvent::AlbumAdded { album });
            }
            Ok(None) => {
                warn!("emit_album_added: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_added: DB error for album {album_id}: {e}");
            }
        }
    }

    pub async fn emit_album_updated(&self, album_id: &str) {
        match self.find_album_detail(album_id).await {
            Ok(Some(album)) => self.emit(LibraryEvent::AlbumUpdated { album }),
            Ok(None) => {
                warn!("emit_album_updated: album {album_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_album_updated: DB error for album {album_id}: {e}");
            }
        }
    }

    /// Re-emit `AlbumUpdated` for every album. Each release's available storage
    /// actions are computed at resolve time from whether a cloud home exists
    /// (`available_storage_actions`), then baked into the cached `ReleaseDetail`.
    /// Connecting or disconnecting a cloud home flips that, but the already-
    /// resolved releases keep their stale actions — so a UI that cached them
    /// (e.g. an open release's storage panel) shows the wrong actions until a
    /// restart. Re-resolving every album reads `has_cloud_home()` fresh, so the
    /// actions recompute and consumers update live. Called on each cloud-home
    /// transition; a no-op-feeling burst that's fine because connect/disconnect
    /// is rare.
    pub async fn emit_all_albums_updated(&self) {
        let albums = match self.get_albums(&[]).await {
            Ok(albums) => albums,
            Err(e) => {
                warn!("emit_all_albums_updated: failed to list albums: {e}");
                return;
            }
        };
        for album in albums {
            self.emit_album_updated(&album.id).await;
        }
    }

    pub fn emit_album_removed(&self, album_id: &str, release_ids: Vec<String>) {
        self.emit(LibraryEvent::AlbumRemoved {
            album_id: album_id.to_string(),
            release_ids,
        });
    }

    pub async fn emit_release_added(&self, album_id: &str, release_id: &str) {
        let release = match self.find_release_detail(release_id).await {
            Ok(Some(release)) => release,
            Ok(None) => {
                warn!("emit_release_added: release {release_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for release {release_id}: {e}");
                return;
            }
        };
        let raw_album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => raw,
            Ok(None) => {
                warn!("emit_release_added: album {album_id} not found in DB, skipping event");
                return;
            }
            Err(e) => {
                warn!("emit_release_added: DB error for album {album_id}: {e}");
                return;
            }
        };
        // The release/album rows are already committed, so the event must fire for
        // them. A cover lookup failure degrades to no covers (the UI lazily fetches
        // them by id) — never drops the event for committed state.
        let covers = self
            .cover_refs(&raw_album.release_ids)
            .await
            .unwrap_or_else(|e| {
                warn!(
                    "emit_release_added: cover lookup failed for album {album_id}: {e}; \
                 emitting without covers"
                );
                HashMap::new()
            });
        let album = resolve_album_summary(raw_album, |rid| covers.get(rid).cloned());
        self.emit(LibraryEvent::ReleaseAdded { album, release });
    }

    pub async fn emit_release_updated(&self, album_id: &str, release_id: &str) {
        match self.find_release_detail(release_id).await {
            Ok(Some(release)) => {
                self.emit(LibraryEvent::ReleaseUpdated {
                    album_id: album_id.to_string(),
                    release,
                });
            }
            Ok(None) => {
                warn!("emit_release_updated: release {release_id} not found in DB, skipping event");
            }
            Err(e) => {
                warn!("emit_release_updated: DB error for release {release_id}: {e}");
            }
        }
    }

    pub async fn emit_release_removed(&self, album_id: &str, release_id: &str) {
        // Ship the parent album's post-removal summary so the reducer interns it
        // rather than reading the old release list to patch it — a read-to-write
        // that goes stale on the sync path, where no AlbumUpdated co-fires. `None`
        // means strictly "the album itself was removed with its last release"; a
        // transient cover-lookup failure must NOT misreport the album as gone, so
        // it degrades to a summary with no covers (the UI lazily fetches them).
        let album = match self.database.find_album_summary(album_id).await {
            Ok(Some(raw)) => {
                let covers = self.cover_refs(&raw.release_ids).await.unwrap_or_else(|e| {
                    warn!(
                        "emit_release_removed: cover lookup failed for album {album_id}: {e}; \
                         emitting without covers"
                    );
                    HashMap::new()
                });
                Some(resolve_album_summary(raw, |rid| covers.get(rid).cloned()))
            }
            Ok(None) => None,
            Err(e) => {
                warn!("emit_release_removed: DB error for album {album_id}: {e}");
                None
            }
        };
        self.emit(LibraryEvent::ReleaseRemoved {
            album_id: album_id.to_string(),
            release_id: release_id.to_string(),
            album,
        });
    }

    /// Emit granular library events for entity changes collected from sync changesets.
    pub async fn emit_sync_entity_changes(
        &self,
        mut changes: crate::library::sync_events::ChangesetEntityChanges,
    ) {
        use crate::library::sync_events::{AlbumChangeEvent, ReleaseChangeEvent};

        // Resolve any unresolved track IDs to album IDs via DB.
        if !changes.unresolved_track_ids.is_empty() {
            match self
                .database
                .get_album_ids_for_tracks(&changes.unresolved_track_ids)
                .await
            {
                Ok(resolved) => {
                    let mut seen = std::collections::HashSet::new();
                    for album_id in resolved.values() {
                        if seen.insert(album_id.clone()) {
                            changes
                                .album_events
                                .push(AlbumChangeEvent::Updated(album_id.clone()));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to resolve track IDs to album IDs: {e}");
                }
            }
        }

        for event in changes.album_events {
            match event {
                AlbumChangeEvent::Added(id) => self.emit_album_added(&id).await,
                AlbumChangeEvent::Updated(id) => self.emit_album_updated(&id).await,
                AlbumChangeEvent::Removed {
                    album_id,
                    release_ids,
                } => self.emit_album_removed(&album_id, release_ids),
            }
        }
        for event in changes.release_events {
            match event {
                ReleaseChangeEvent::Added {
                    album_id,
                    release_id,
                } => self.emit_release_added(&album_id, &release_id).await,
                ReleaseChangeEvent::Updated {
                    album_id,
                    release_id,
                } => self.emit_release_updated(&album_id, &release_id).await,
                ReleaseChangeEvent::Removed {
                    album_id,
                    release_id,
                } => self.emit_release_removed(&album_id, &release_id).await,
            }
        }
    }

    /// Emit a general error to the UI through the event bus.
    pub fn emit_error(&self, error: crate::ui::UiError) {
        self.emit(LibraryEvent::Error { error });
    }

    // =========================================================================
    // Config access
    // =========================================================================

    /// Get a snapshot of the current config.
    pub fn get_config(&self) -> crate::config::Config {
        self.config_handle.config().clone()
    }

    /// Rename a library by id. If the id matches the active library, the
    /// rename goes through the reactive `ConfigState` so all current
    /// subscribers see it. Otherwise the library's `config.yaml` on disk
    /// is edited in place — the inactive library isn't loaded into
    /// memory.
    pub fn rename_library(&self, library_id: &str, name: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Library name cannot be empty".to_string());
        }
        if library_id == self.config_handle.config().library_id {
            return self
                .config_handle
                .rename_library(name)
                .map_err(|e| format!("{e}"));
        }
        let bae_dir = crate::config::bae_dir().map_err(|e| format!("{e}"))?;
        crate::config::rename_inactive_library(&bae_dir, library_id, name)
            .map_err(|e| format!("{e}"))
    }

    /// Forget the active library's encryption key. The running
    /// `sync_manager` still holds the key in memory so this session
    /// keeps working; the next launch lands on `UnlockView` because
    /// the keyring is empty. Used by the sidebar's "Lock Library" action.
    pub fn forget_encryption_key(&self) -> Result<(), String> {
        self.key_service
            .forget_encryption_key()
            .map_err(|e| format!("{e}"))
    }

    /// Subscribe to the config-state stream; each change yields the whole latest
    /// `Config`.
    pub fn subscribe_config_changes(&self) -> tokio::sync::watch::Receiver<crate::config::Config> {
        self.config_handle.subscribe()
    }

    /// Set whether playback pauses between vinyl/cassette sides.
    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.pause_between_sides = enabled)
    }

    /// Forget this (local) library on this device: delete its master encryption
    /// key, clear the active-library pointer, and remove its data directory. The
    /// owner's cloud copy (if any) is untouched — this only drops the device's
    /// local presence.
    ///
    /// The caller must drop this handle immediately afterward: the database
    /// lives in the directory being removed, so this must be the handle's last
    /// operation. The next launch re-discovers and opens another library (or
    /// onboards) since the active pointer is gone.
    pub fn forget_library(&self) -> Result<(), String> {
        if let Err(e) = self.key_service.delete_encryption_key() {
            warn!("Failed to delete encryption key while forgetting library: {e}");
        }

        let library_id = self.config_handle.config().library_id.clone();
        let bae_dir = crate::config::bae_dir().map_err(|e| e.to_string())?;

        let active_pointer = bae_dir.join("active-library");
        if active_pointer.exists() {
            if let Err(e) = std::fs::remove_file(&active_pointer) {
                warn!("Failed to clear active-library pointer: {e}");
            }
        }

        if let Some(dir) = crate::config::library_data_dir(&bae_dir, &library_id) {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                warn!("Failed to remove library data at {}: {e}", dir.display());
            }
        }

        Ok(())
    }

    // =========================================================================
    // Discogs token management
    // =========================================================================

    pub fn has_discogs_token(&self) -> bool {
        self.config_handle.has_discogs_key()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, String> {
        self.key_service
            .get_discogs_key()
            .map_err(|e| e.to_string())
    }

    pub fn save_discogs_key(&self, token: &str) -> Result<(), String> {
        self.key_service
            .set_discogs_key(token)
            .map_err(|e| e.to_string())
    }

    pub fn delete_discogs_key(&self) -> Result<(), String> {
        self.key_service
            .delete_discogs_key()
            .map_err(|e| e.to_string())
    }

    /// Record a stored key with its validation state — the single write for the
    /// save path. `Some(validation)` is both the stored-key hint and the state,
    /// so one `update` keeps them consistent and fires one watch-channel
    /// notification.
    pub fn set_discogs_key_stored(
        &self,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.discogs = Some(validation))
    }

    /// Clear the stored-key state — no key, so no validation.
    pub fn clear_discogs_key_stored(&self) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.discogs = None)
    }

    /// Update the stored key's validation state. No-op when no key is stored —
    /// validation describes a key that exists.
    pub fn set_discogs_validation(
        &self,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| {
            if c.discogs.is_some() {
                c.discogs = Some(validation);
            }
        })
    }

    pub fn discogs_validation(&self) -> Option<crate::config::DiscogsValidation> {
        self.config_handle.config().discogs
    }

    /// An observer that folds a Discogs call's outcome into the stored key's
    /// validation, so every call site updates the stored validation state
    /// without recording the outcome itself. A 401 marks it `Rejected`; a
    /// success while it was `Unvalidated` confirms it `Valid`; any other outcome
    /// (network, rate-limit, success while already settled, or no key stored)
    /// leaves it untouched. Injected into the client by [`Self::discogs_client`].
    pub(crate) fn discogs_validation_observer(
        &self,
    ) -> crate::discogs::client::DiscogsValidationObserver {
        use crate::config::DiscogsValidation;
        use crate::discogs::client::DiscogsKeySignal;
        let config_handle = self.config_handle.clone();
        std::sync::Arc::new(move |signal| {
            let Some(current) = config_handle.config().discogs else {
                // The key was removed while a Discogs call was in flight: the
                // client outlives the config entry it was built from. Nothing to
                // fold the outcome into.
                tracing::debug!("discogs validation signal ignored: no key stored");
                return;
            };
            let next = match signal {
                DiscogsKeySignal::Rejected => DiscogsValidation::Rejected,
                DiscogsKeySignal::Accepted if current == DiscogsValidation::Unvalidated => {
                    DiscogsValidation::Valid
                }
                _ => return,
            };
            if current == next {
                return;
            }
            if let Err(e) = config_handle.update(|c| c.discogs = Some(next)) {
                tracing::warn!("failed to persist discogs validation {next:?}: {e}");
            }
        })
    }

    /// A client for the stored key, unless that key is `Rejected`. A `Valid` or
    /// `Unvalidated` key is served (the latter used optimistically); a
    /// `Rejected` key is withheld so search call sites skip Discogs entirely.
    /// The client reports each call's outcome back into the validation state.
    pub fn discogs_client(&self) -> Result<Option<crate::discogs::DiscogsClient>, String> {
        if self.discogs_validation() == Some(crate::config::DiscogsValidation::Rejected) {
            return Ok(None);
        }
        let observer = self.discogs_validation_observer();
        Ok(self
            .key_service
            .get_discogs_key()
            .map_err(|e| e.to_string())?
            .map(|key| crate::discogs::DiscogsClient::with_observer(key, observer)))
    }

    // =========================================================================
    // Sync / cloud
    // =========================================================================

    /// Whether a cloud provider is connected. Reads config, not manager presence:
    /// the connected provider lives in config and is known synchronously from the
    /// first read, whereas the `SyncManager` (and its cloud client) is built lazily
    /// once connected and still absent on mobile when the first listing query runs.
    pub fn is_sync_configured(&self) -> bool {
        self.config_handle.config().cloud_home.provider.is_some()
    }

    pub fn has_cloud_home(&self) -> bool {
        self.sync_connected()
    }

    /// The coven `BlobRef` for a remote release file's audio blob — its identity
    /// in coven's cache (and the cloud on a miss). `cloud_path` is the row's value
    /// RELATIVE to the `release_files` namespace coven prepends. A release file is
    /// a coven **user-provided** blob (the user's own imported file): Local = the
    /// file at the user's path (an external ref coven holds), Remote = uploaded and
    /// `CacheLazy` (fetched into the cache on first read). coven resolves which by
    /// where the bytes are — the same `BlobRef` addresses every locality.
    pub(crate) fn release_file_blob_ref(file: &DbFile) -> coven::BlobRef {
        coven::BlobRef {
            namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
            id: file.id.clone(),
            scope: coven::BlobScope::Master,
            cloud_path: file.cloud_path.clone(),
            provenance: coven::Provenance::UserProvided,
            fill: coven::CacheFill::CacheLazy,
        }
    }

    /// The one coven data handle. The playback reader clones it to stream blob
    /// ranges; callers route every blob read/write and the sync lifecycle through
    /// it rather than reaching into coven's internals.
    pub(crate) fn handle(&self) -> &CovenHandle {
        &self.handle
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn database_for_test(&self) -> Database {
        self.database.clone()
    }

    /// Configure coven's per-namespace cache budgets for this device: the bulk for
    /// `release_files` (audio), a small reserved slice each for `covers` and
    /// `artist_images`, so each namespace evicts against its own budget and audio
    /// pressure never wipes the cover cache. Device-local; set once at startup.
    pub(crate) async fn configure_cache_budgets(&self) -> Result<(), LibraryError> {
        self.handle
            .set_cache_budget(
                crate::sync::RELEASE_FILES_NAMESPACE,
                crate::sync::RELEASE_FILES_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::COVERS_NAMESPACE,
                crate::sync::COVERS_CACHE_BUDGET,
            )
            .await?;
        self.handle
            .set_cache_budget(
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                crate::sync::ARTIST_IMAGES_CACHE_BUDGET,
            )
            .await?;
        Ok(())
    }

    /// Store a bae-produced host-provided image and its row in one coven batch.
    pub async fn store_library_image_blob(
        &self,
        image: &DbLibraryImage,
        bytes: &[u8],
    ) -> Result<(), LibraryError> {
        self.database.write_library_image_blob(image, bytes).await?;
        Ok(())
    }

    /// Read a release file's whole plaintext through coven's locality-aware read:
    /// served from the user's file (Local user-provided), coven's local store
    /// (Local host-provided), `storage/pinned`/`storage/cache` on a Remote hit, or
    /// fetched from the cloud (into `cache/`) on a Remote miss. For the
    /// non-streaming readers (export, gallery images); playback streams ranges via
    /// `open_blob_stream` instead. A vanished/changed external file maps to a
    /// storage error so the caller surfaces a "files missing / moved" state.
    pub(crate) async fn read_release_blob(&self, file: &DbFile) -> Result<Vec<u8>, LibraryError> {
        let blob = Self::release_file_blob_ref(file);
        self.handle
            .read_blob(&blob)
            .await
            .map_err(|e| LibraryError::Storage(format!("read of {}: {e}", file.id)))
    }

    /// The coven `BlobRef` for a host-provided library image (a cover or an artist
    /// image) — its identity in coven's local store while Local and its cache while
    /// Remote. `namespace` is `covers` or `artist_images`; `id` is the release id
    /// (a cover) or artist id (an artist image). A host-provided `CacheEager` blob:
    /// the bytes are produced by bae and kept by coven, fetched into the cache on
    /// pull so a grid renders from local bytes. `cloud_path` is the row's readable
    /// path on a browsable home (`None` on an opaque one).
    pub(crate) fn image_blob_ref(
        namespace: &str,
        id: &str,
        cloud_path: Option<String>,
    ) -> coven::BlobRef {
        coven::BlobRef {
            namespace: namespace.to_string(),
            id: id.to_string(),
            scope: coven::BlobScope::Master,
            cloud_path,
            provenance: coven::Provenance::HostProvided,
            fill: coven::CacheFill::CacheEager,
        }
    }

    /// The cover [`ImageRef`] for one release — its image id paired with the
    /// `covers` row's `_updated_at` — or `None` when the release has no cover row.
    async fn cover_ref(&self, release_id: &str) -> Result<Option<ImageRef>, LibraryError> {
        cover_ref_for(&self.database, release_id).await
    }

    /// The cover [`ImageRef`] for each of `release_ids` that has a `covers` row,
    /// in one query. The batch source for the list/grid resolvers, which build a
    /// `Fn(&str) -> Option<ImageRef>` over the returned map.
    async fn cover_refs(
        &self,
        release_ids: &[String],
    ) -> Result<HashMap<String, ImageRef>, LibraryError> {
        Ok(self
            .database
            .cover_versions(release_ids)
            .await?
            .into_iter()
            .map(|(id, version)| (id.clone(), ImageRef { id, version }))
            .collect())
    }

    /// Read a host-provided library image's whole bytes through coven's
    /// locality-aware read: coven's local store while Local, the pinned/evictable
    /// cache or the cloud while Remote. `id` is a release id (a cover) or an artist
    /// id (an artist image); the `covers` row is probed first (the common grid
    /// case), then `artist_images`. `None` when no such image row exists (no cover
    /// produced); a read error surfaces rather than being masked.
    pub async fn read_image_blob(&self, id: &str) -> Result<Option<Vec<u8>>, LibraryError> {
        for (namespace, image_type) in [
            (crate::sync::COVERS_NAMESPACE, LibraryImageType::Cover),
            (
                crate::sync::ARTIST_IMAGES_NAMESPACE,
                LibraryImageType::Artist,
            ),
        ] {
            let Some(row) = self.database.find_library_image(id, &image_type).await? else {
                continue;
            };
            let blob = Self::image_blob_ref(namespace, id, row.cloud_path.clone());
            let bytes = self
                .handle
                .read_blob(&blob)
                .await
                .map_err(|e| LibraryError::Storage(format!("read image {id}: {e}")))?;
            return Ok(Some(bytes));
        }
        Ok(None)
    }

    /// Whether coven holds this release pinned on this device — true iff its
    /// representative blob (any one of the release's files; pin/unpin act on all a
    /// release's blobs together) is kept in coven's `storage/pinned/`. `None` (a
    /// release with no files) reads as not pinned. Pinned-ness is coven cache
    /// state, never a bae column — answered through the handle, not by stat-ing
    /// coven's cache layout.
    pub(crate) async fn release_pinned(
        &self,
        any_file_id: Option<&str>,
    ) -> Result<bool, LibraryError> {
        match any_file_id {
            Some(file_id) => release_file_pinned(&self.handle, file_id).await,
            None => Ok(false),
        }
    }

    /// Pin a remote release's blobs for offline: coven fetches every blob into
    /// `storage/pinned/` (from the evictable cache if already there, else the
    /// cloud). Idempotent. Pinned-ness is coven cache state — there is no bae flag.
    /// The low-level cache op behind the "Pin" transition.
    pub(crate) async fn pin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .pin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("pin release {release_id}: {e}")))
    }

    /// Unpin a remote release's blobs: coven moves every blob from
    /// `storage/pinned/` to the evictable `storage/cache/` (still readable, now
    /// droppable). No cloud read, no bae flag. The low-level cache op behind the
    /// "Unpin" transition.
    pub(crate) async fn unpin_release_blobs(&self, release_id: &str) -> Result<(), LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        let blobs: Vec<_> = files.iter().map(Self::release_file_blob_ref).collect();
        self.handle
            .unpin(&blobs)
            .await
            .map_err(|e| LibraryError::Storage(format!("unpin release {release_id}: {e}")))
    }

    // =========================================================================
    // coven-owned locality transitions (make-Remote / make-Local / cancel)
    // =========================================================================

    /// The make-Local destination map: each release file's blob id → the user path
    /// (`new_path/original_filename`) its bytes go back to. Host-provided blobs (the
    /// cover) take no dest — coven restores them to its local store. The single
    /// place the dest shape is built, shared by the production transition and the
    /// test driver.
    async fn make_local_dest(
        &self,
        release_id: &str,
        new_path: &str,
    ) -> Result<HashMap<String, PathBuf>, LibraryError> {
        let files = self.database.get_files_for_release(release_id).await?;
        Ok(files
            .iter()
            .map(|f| {
                (
                    f.id.clone(),
                    std::path::Path::new(new_path).join(&f.original_filename),
                )
            })
            .collect())
    }

    /// Bridge bae's `CancellationToken` to the `watch::Receiver<bool>` coven's
    /// make_local polls between blobs: when the token fires, flip the watch. Returns
    /// the receiver and the bridge task's handle (abort it once make_local returns).
    /// The single place the bridge lives.
    fn cancel_token_to_watch(
        cancel: &crate::library::CancellationToken,
    ) -> (
        tokio::sync::watch::Receiver<bool>,
        tokio::task::JoinHandle<()>,
    ) {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let token = cancel.clone();
        let handle = tokio::spawn(async move {
            token.cancelled().await;
            let _ = cancel_tx.send(true);
        });
        (cancel_rx, handle)
    }

    /// Map a coven `make_local` result to bae's: a cancel before the commit is a
    /// clean stop (coven rolled back the partial copies and left the release
    /// Remote), every other error is surfaced. Shared by the production transition
    /// and the test driver.
    fn map_make_local_result(
        release_id: &str,
        result: Result<(), coven::MakeLocalError>,
    ) -> Result<(), LibraryError> {
        match result {
            Ok(()) => Ok(()),
            Err(coven::MakeLocalError::Cancelled) => {
                debug!(
                    release_id,
                    "make-local cancelled before commit; release stays Remote"
                );
                Ok(())
            }
            Err(e) => Err(LibraryError::Storage(format!(
                "make release {release_id} local: {e}"
            ))),
        }
    }

    /// Make a release Remote (Local → Remote) through coven: coven enqueues an
    /// upload per user-provided blob from its external file, uploads each, and on
    /// the last flips the `remote` gate true, drops the external refs, deletes the
    /// source files, and re-emits the subtree (the host-provided cover then rides
    /// along). Returns once enqueued; completion fires `on_root_made_remote`.
    pub async fn coven_make_remote(&self, release_id: &str, pin: bool) -> Result<(), LibraryError> {
        self.handle
            .make_remote("releases", release_id, pin)
            .await
            .map_err(|e| LibraryError::Storage(format!("make release {release_id} remote: {e}")))
    }

    /// Cancel an in-flight make-Remote of `release_id` through coven: clears the
    /// intent and pending uploads and tombstones any blob already in the cloud.
    /// The gate never flips, so the release stays Local.
    pub(crate) async fn coven_cancel_make_remote(
        &self,
        release_id: &str,
    ) -> Result<(), LibraryError> {
        self.handle
            .cancel_make_remote("releases", release_id)
            .await
            .map_err(|e| {
                LibraryError::Storage(format!("cancel make release {release_id} remote: {e}"))
            })
    }

    /// Make a release Local (Remote → Local) through coven: coven materializes each
    /// blob back to a local file durability-first — every release file (a
    /// user-provided blob) to `new_path/{original_filename}`, the host-provided
    /// cover to coven's local store (no dest) — then flips the `remote` gate false,
    /// registers the external refs, and enqueues the cloud deletes in one atomic
    /// commit. `cancel` aborts before the commit (the release stays Remote).
    pub async fn coven_make_local(
        &self,
        release_id: &str,
        new_path: &str,
        cancel: &crate::library::CancellationToken,
    ) -> Result<(), LibraryError> {
        let dest = self.make_local_dest(release_id, new_path).await?;
        let (cancel_rx, bridge) = Self::cancel_token_to_watch(cancel);
        let result = self
            .handle
            .make_local("releases", release_id, &dest, &cancel_rx)
            .await;
        bridge.abort();
        Self::map_make_local_result(release_id, result)
    }

    pub fn generate_restore_code(&self) -> Result<String, String> {
        self.handle.generate_restore_code()
    }

    /// The library's membership: its devices (with this device flagged, each
    /// member's fingerprint, and whether it can be removed) and whether the
    /// running device is an owner.
    pub async fn get_members(&self) -> Result<crate::sync::sync_manager::Membership, String> {
        let members = self.handle.get_members().await?;
        Ok(crate::sync::sync_manager::Membership::from_members(members))
    }

    /// Approve a device into the library by its public key, wrapping the library
    /// key to it and signing a membership entry. Returns the invite code to hand
    /// back to the joining device. bae adds every device as a `Member`; the
    /// founding device is the `Owner`.
    pub async fn invite_member(&self, public_key_hex: &str) -> Result<String, String> {
        self.handle
            .invite_member(
                public_key_hex,
                crate::sync::sync_manager::MemberRole::Member,
            )
            .await
    }

    /// Remove a device from the library and rotate the library key so the removed
    /// device can no longer read new data. Records the rotated key's fingerprint
    /// in this device's config.
    pub async fn remove_member(&self, public_key_hex: &str) -> Result<(), String> {
        let fingerprint = self.handle.remove_member(public_key_hex).await?;
        self.config_handle
            .record_encryption_key_fingerprint(fingerprint)
            .map_err(|e| e.to_string())
    }

    // =========================================================================
    // File paths / storage
    // =========================================================================

    /// The on-disk path of a release file that lives at the user's own path — a
    /// coven **user-provided Local** blob's external ref. `Ok(Some(path))` only
    /// for a Local release file coven holds an external ref for (the user's file
    /// in place); `Ok(None)` for a Remote file (its bytes are in coven's cache,
    /// keyed by id, with no stable bae path) or an unknown file. DB errors
    /// propagate so callers distinguish "no in-place file" from "library broken".
    ///
    /// Used where a caller needs the actual user file (the DiscID re-read of the
    /// rip's artifacts), not coven's locality-aware byte read.
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self
            .database
            .external_blob(file_id)
            .await?
            .map(|ext| ext.path))
    }

    pub fn create_release_storage(&self) -> ReleaseStorageImpl {
        ReleaseStorageImpl::new_local(self.library_dir.clone())
    }

    pub async fn append_pending_deletions(
        &self,
        deletions: &[PendingDeletion],
    ) -> Result<(), String> {
        append_pending_deletions(self.library_dir.as_ref(), deletions)
            .await
            .map_err(|e| format!("{e}"))
    }

    // =========================================================================
    // Encryption
    // =========================================================================

    pub fn has_encryption(&self) -> bool {
        self.encryption_service_inner().is_some()
    }

    pub fn get_encryption_service(&self) -> Option<EncryptionService> {
        self.encryption_service_inner()
    }

    // =========================================================================
    // Playback state persistence
    // =========================================================================

    /// Write the device-local playback-state row. Propagates the DB error so the
    /// caller can distinguish a write failure from a stored absence; the resume
    /// row is a device-local cache, so the call site logs and continues rather
    /// than treating a failed write as fatal to playback.
    pub async fn save_playback_state(
        &self,
        state: &crate::db::DbPlaybackState,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_playback_state(state).await?)
    }

    /// Read the device-local playback-state row (kept, not deleted — it's
    /// overwritten on the next playback change and cleared on stop). `Ok(None)`
    /// means no row is stored; an `Err` is a read failure, kept distinct from
    /// absence so the caller doesn't silently start fresh on a DB error.
    pub async fn load_playback_state(
        &self,
    ) -> Result<Option<crate::db::DbPlaybackState>, LibraryError> {
        Ok(self.database.load_playback_state().await?)
    }

    /// Delete the device-local playback-state row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_playback_state().await?)
    }

    // =========================================================================
    // Database domain operations
    // =========================================================================

    pub async fn get_release_by_id(
        &self,
        release_id: &str,
    ) -> Result<Option<DbRelease>, LibraryError> {
        Ok(self.database.find_release_by_id(release_id).await?)
    }

    /// Whether a release whose stored content hash equals `hash` is in the
    /// library. The import watcher stamps each scanned candidate with this so an
    /// already-imported folder surfaces under the "Added" tab even after a
    /// restart (it matches by file structure, not by name).
    pub async fn is_content_hash_imported(&self, hash: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_content_hash_imported(hash).await?)
    }

    /// Count outbox upload entries still pending for a release's files.
    /// Zero means the cloud copy is confirmed durable. Used by the unpin
    /// guard in `make_release_local` to refuse a transition mid-upload — the
    /// UI side of "no actions mid-upload" reads the `OutboxSnapshot.per_release`
    /// map instead.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, LibraryError> {
        Ok(self
            .database
            .count_pending_uploads_for_release(release_id)
            .await?)
    }

    /// Seed an upload outbox row + refresh the snapshot. coven owns enqueueing in
    /// `make_remote`, so this is only a test helper for exercising the
    /// outbox-snapshot / drain machinery directly.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn add_cloud_outbox_upload(
        &self,
        file_id: &str,
        cloud_key: &str,
        source_path: Option<&str>,
        retain_pinned: bool,
    ) -> Result<(), LibraryError> {
        self.database
            .add_cloud_outbox_upload(file_id, cloud_key, source_path, retain_pinned)
            .await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// The full cloud object key a release file's blob lives at, derived through
    /// coven for the configured home's scheme (`Hashed` → `{ns}/{ab}/{cd}/{id}`,
    /// `Plain` → `{ns}/{cloud_path}`). Used by the bae-side delete path, which
    /// stays bae's responsibility (the transitions are coven's).
    fn release_file_cloud_key(&self, file: &DbFile) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::release_file_blob_ref(file))
            .map_err(|e| LibraryError::Storage(format!("cloud key for file {}: {e}", file.id)))
    }

    /// The full cloud object key a cover blob lives at (namespace `covers`),
    /// derived through coven for the configured home's scheme. Used by the
    /// bae-side cover delete path.
    fn cover_cloud_key(
        &self,
        release_id: &str,
        cloud_path: Option<&str>,
    ) -> Result<String, LibraryError> {
        self.handle
            .blob_cloud_key(&Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                release_id,
                cloud_path.map(str::to_string),
            ))
            .map_err(|e| LibraryError::Storage(format!("cloud key for cover {release_id}: {e}")))
    }

    /// Retry failed uploads now: clear their backoff so the next cycle picks
    /// them up immediately, then kick the sync loop.
    pub async fn retry_outbox_now(&self) -> Result<(), LibraryError> {
        self.database.reset_cloud_outbox_backoff().await?;
        self.trigger_sync();
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Cancel one queued outbox entry by id. Removes the queue row only; the
    /// local file is untouched, so the release just stops syncing this entry.
    pub async fn cancel_outbox_item(&self, id: i64) -> Result<(), LibraryError> {
        self.database.remove_cloud_outbox_entry(id).await?;
        self.emit_outbox_changed().await;
        Ok(())
    }

    /// Stop uploading a release that's mid-make-Remote and keep it Local.
    ///
    /// coven owns the cancel: it clears the durable make-Remote intent and the
    /// release's pending upload rows, and tombstones any blob that already reached
    /// the cloud, in one transaction. The gate never flips, so the release stays
    /// Local — its files are still the external refs coven holds, untouched.
    pub async fn cancel_release_upload(&self, release_id: &str) -> Result<(), LibraryError> {
        self.coven_cancel_make_remote(release_id).await?;
        self.emit_outbox_changed().await;
        // Refresh the release row (it no longer reads as "uploading"). A
        // best-effort UI nudge — the cancel itself already succeeded above.
        match self.get_release_by_id(release_id).await {
            Ok(Some(release)) => {
                self.emit_release_updated(&release.album_id, release_id)
                    .await
            }
            Ok(None) => {
                warn!("cancel_release_upload: release {release_id} missing; skipped UI refresh")
            }
            Err(e) => {
                warn!("cancel_release_upload: loading release {release_id} for refresh failed: {e}")
            }
        }
        Ok(())
    }

    /// Pause or resume the cloud-upload pipeline. Paused means new enqueues
    /// still land in the outbox but the sync cycle won't drain them; in-flight
    /// uploads finish (coven's `drain_uploads` checks the flag between
    /// entries, not mid-write). Re-emits the outbox snapshot so the UI's
    /// paused indicator and the bottom-panel summary update.
    pub async fn set_sync_paused(&self, paused: bool) {
        self.sync_paused
            .store(paused, std::sync::atomic::Ordering::SeqCst);
        if !paused {
            // Kick the loop so the queue starts draining immediately on resume
            // rather than waiting for the next idle tick.
            self.trigger_sync();
        }
        self.emit_outbox_changed().await;
    }

    /// Current paused state of the upload pipeline. The snapshot builder
    /// reads this so the UI can render its paused indicator.
    pub fn is_sync_paused(&self) -> bool {
        self.sync_paused.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Drive coven's upload drain once through the handle's connected sync
    /// manager, for tests that connected an injected cloud home via
    /// [`connect_test_cloud_home`](Self::connect_test_cloud_home). Returns the
    /// number of blobs uploaded. Production drains from the running sync loop, so
    /// this stays out of release builds.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn drain_uploads_for_test(&self) -> Result<usize, String> {
        self.handle
            .drain_uploads()
            .await
            .map(|outcome| outcome.uploaded)
    }

    pub async fn get_tracks_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }

    /// All `release_identities` rows for a release. Empty for Unknown.
    pub async fn get_release_identities(
        &self,
        release_id: &str,
    ) -> Result<Vec<crate::import::ReleaseIdentity>, LibraryError> {
        Ok(self.database.get_release_identities(release_id).await?)
    }

    /// Insert identity rows for an existing release.
    pub async fn insert_release_identities(
        &self,
        release_id: &str,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .insert_release_identities(release_id, identities)
            .await?)
    }

    /// Find an existing album the new import should attach to.
    ///
    /// Two-pass identity dedup against `release_identities`:
    ///
    /// 1. **Per-pressing rejection.** If any release already in the library
    ///    carries an identity row matching one of the new release's
    ///    `(source, source_release_id)` pairs (Exact identities only —
    ///    Approximate skips this), that's a duplicate import. Surface the
    ///    existing album's title so the user sees what they already have.
    ///
    /// 2. **Cross-source merge.** If any release in the library carries an
    ///    identity row matching one of the new release's
    ///    `(source, source_group_id)` pairs, return that release's
    ///    `album_id` so the new release attaches to the same album.
    ///    Identities can pair across sources — an MB-rooted import that
    ///    carried a cross-link Discogs row will be reachable from a later
    ///    Discogs-rooted import of the same master.
    ///
    /// Empty `identities` (Unknown) skips both lookups — Unknown imports
    /// always get a fresh album.
    pub async fn find_existing_album_for_import(
        &self,
        identities: &[crate::import::ReleaseIdentity],
    ) -> Result<Option<String>, String> {
        if identities.is_empty() {
            return Ok(None);
        }

        // 1. Per-pressing rejection: any Exact identity matching a row
        //    already in `release_identities`.
        if let Some(existing) = self
            .database
            .find_album_by_identity_release(identities)
            .await
            .map_err(|e| format!("Database error: {e}"))?
        {
            return Err(format!(
                "This release is already in your library as \"{}\"",
                existing.title,
            ));
        }

        // 2. Cross-source merge: any group identity matching a row already
        //    in `release_identities`.
        let album_id = self
            .database
            .find_album_by_identity_group(identities)
            .await
            .map_err(|e| format!("Database error: {e}"))?;

        Ok(album_id)
    }

    /// Re-run the seeding projection from `metadata_source` /
    /// `metadata_source_release_id`, returning the projected
    /// `ReleaseUserEdit`. Read-only: no DB writes happen here. The caller
    /// (the editor) populates its form with the returned values; the
    /// user then re-edits or saves via `apply_release_metadata_user_edit`.
    ///
    /// Source dispatch:
    ///
    /// - `MusicBrainz` / `Discogs` — pull cached `release_metadata` rows
    ///   for the release and re-project per the same rules import uses.
    ///   The Exact-vs-Approximate decision comes from the matching
    ///   `release_identities` row's `source_release_id`: present = Exact
    ///   (full pressing data), NULL = Approximate (album-group fields
    ///   only; pressing fields cleared).
    /// - `FileTags` — re-read embedded tags from the release's local
    ///   audio files via `map_file_tags_to_db`. Errors out if the files
    ///   aren't reachable on disk (cloud-only without a local copy).
    ///
    /// Identity rows and the `metadata_source` columns are not touched —
    /// reset replays from the existing pointer rather than changing it.
    /// Identity changes go through `set_identity`.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn reset_metadata_to_source(
        &self,
        release_id: &str,
    ) -> Result<crate::import::ReleaseUserEdit, LibraryError> {
        use crate::db::ReleaseMetadataSource;
        use crate::import::{parsed_album_to_user_edit, MetadataSource};

        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;

        let identities = self.database.get_release_identities(release_id).await?;

        let parsed =
            match release.metadata_source {
                ReleaseMetadataSource::MusicBrainz => {
                    let source_release_id = release
                        .metadata_source_release_id
                        .as_deref()
                        .ok_or_else(|| {
                            LibraryError::Import(
                            "metadata_source = 'musicbrainz' but metadata_source_release_id is NULL"
                                .to_string(),
                        )
                        })?;
                    project_musicbrainz_from_cache(
                        &self.database,
                        release_id,
                        source_release_id,
                        self.clock.as_ref(),
                        self.ids.as_ref(),
                    )
                    .await?
                }
                ReleaseMetadataSource::Discogs => {
                    let source_release_id = release
                        .metadata_source_release_id
                        .as_deref()
                        .ok_or_else(|| {
                            LibraryError::Import(
                            "metadata_source = 'discogs' but metadata_source_release_id is NULL"
                                .to_string(),
                        )
                        })?;
                    project_discogs_from_cache(
                        &self.database,
                        release_id,
                        source_release_id,
                        self.clock.as_ref(),
                        self.ids.as_ref(),
                    )
                    .await?
                }
                ReleaseMetadataSource::FileTags => {
                    project_file_tags(
                        &self.database,
                        &release,
                        self.clock.clone(),
                        self.ids.clone(),
                    )
                    .await?
                }
            };

        // Approximate clearing. The matching identity row drives the
        // Exact-vs-Approximate decision per source. file_tags has no
        // identity row to inspect — its pressing fields come straight
        // from the tags and stay as projected.
        let approximate = match release.metadata_source {
            ReleaseMetadataSource::MusicBrainz => identities
                .iter()
                .find(|id| id.source == MetadataSource::MusicBrainz)
                .is_some_and(|id| id.source_release_id.is_none()),
            ReleaseMetadataSource::Discogs => identities
                .iter()
                .find(|id| id.source == MetadataSource::Discogs)
                .is_some_and(|id| id.source_release_id.is_none()),
            ReleaseMetadataSource::FileTags => false,
        };
        let mut user_edit = parsed_album_to_user_edit(&parsed);
        if approximate {
            user_edit.pressing = crate::import::PressingEdit::blank();
        }
        Ok(user_edit)
    }

    /// Replace a release's identity rows, metadata-source pointer, and
    /// cached source payload in one shot, moving the release between
    /// albums when the new identity shape doesn't fit the current one.
    ///
    /// `new_identities` may be empty (Unknown), or carry one or more
    /// `(source, source_group_id, source_release_id)` rows that the
    /// caller has already cross-linked. `metadata_pointer` updates the
    /// `metadata_source` / `metadata_source_release_id` columns; a later
    /// re-projection reads these to replay the seed.
    ///
    /// `metadata_pairs` is the freshly-fetched cached payload that
    /// pairs with `metadata_pointer`. Pass an empty slice for Unknown
    /// (no source payload to cache); for Exact/Approximate pass the
    /// `metadata_pairs` returned alongside the parsed release. The
    /// cache replacement is atomic with the identity / pointer write —
    /// there's no in-between state where a re-projection would observe a
    /// stale payload pointing at the prior source.
    ///
    /// **Album side effects.** Empty `new_identities` always moves the
    /// release to a fresh album holding only it. Otherwise, target
    /// resolution prefers a cross-source merge: if any *other* release
    /// in the library has an identity row matching one of
    /// `new_identities` on `(source, source_group_id)`, that release's
    /// album is the destination (the per-source agreement invariant
    /// makes the candidate unique). With no merge candidate the release
    /// stays in its current album when no sibling disagrees on any
    /// shared source, or moves to a fresh album when one does. Vacated
    /// albums with no remaining releases are deleted.
    ///
    /// **Album/release/track row data is not touched.** Pressing fields,
    /// album fields, and tracks stay as-is. Only `release_metadata`
    /// cache rows are replaced. Caller decides whether to also reseed
    /// the metadata.
    ///
    /// Emits one of `AlbumAdded` / `AlbumUpdated` for the destination
    /// album, plus `AlbumRemoved` or `AlbumUpdated` for the vacated
    /// source album when the release actually moved.
    pub async fn set_identity(
        &self,
        release_id: &str,
        new_identities: Vec<crate::import::ReleaseIdentity>,
        metadata_pointer: crate::import::MetadataPointer,
        metadata_pairs: &[(String, String)],
    ) -> Result<(), LibraryError> {
        use crate::db::DbReleaseMetadata;

        let current_album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;

        let target = self
            .resolve_identity_target_album(release_id, &current_album_id, &new_identities)
            .await?;

        let (new_metadata_source, new_metadata_source_release_id) =
            metadata_pointer_to_columns(metadata_pointer);

        let now = self.clock.now();
        let new_metadata: Vec<DbReleaseMetadata> = metadata_pairs
            .iter()
            .map(|(source, json)| {
                DbReleaseMetadata::new(release_id, source, json.clone(), self.ids.new_id(), now)
            })
            .collect();

        // The atomic call handles all source-album bookkeeping inside
        // its transaction (empty-check, primary_release_id repair,
        // album_artists copy) plus the `release_metadata` cache
        // replacement. Empty/repair decisions live there to avoid
        // TOCTOU between a separate read and the write.
        let outcome = self
            .database
            .set_identity_atomic(
                release_id,
                &new_identities,
                new_metadata_source,
                new_metadata_source_release_id.as_deref(),
                &current_album_id,
                &target.album_id,
                target.new_album.as_ref(),
                &new_metadata,
            )
            .await?;

        let release_moved = target.album_id != current_album_id;

        // Event emission. The destination album event is fat: AlbumAdded
        // when we created it just now, AlbumUpdated otherwise (its
        // release set changed). The source-album event covers the move
        // itself: AlbumRemoved when the vacated album is now empty,
        // AlbumUpdated when it still has releases.
        if target.new_album.is_some() {
            self.emit_album_added(&target.album_id).await;
        } else {
            self.emit_album_updated(&target.album_id).await;
        }
        if release_moved {
            if outcome.source_album_deleted {
                // The release moved to the destination album; the destination
                // event above already re-homed it. No child releases remain
                // under the vacated source album.
                self.emit_album_removed(&current_album_id, Vec::new());
            } else {
                self.emit_album_updated(&current_album_id).await;
            }
        }

        Ok(())
    }

    /// Re-identify commit. Translates the user's `IdentityChoice` from
    /// the re-identify result list into a fully cross-linked identity vec
    /// plus metadata pointer, then calls `set_identity`. Mirrors the
    /// import commit pipeline so a re-identified release lands with the
    /// same identity-row shape an initial import would produce.
    ///
    /// - **Exact / Approximate** — fetches the picked release through
    ///   `prepare_release` (which composes MB↔Discogs cross-linking via
    ///   `commit_mb_release` / `commit_discogs_release`) and projects the
    ///   mapper's identity vec via `apply_identity_choice`. The
    ///   `metadata_pointer` points at the picked release. The fetched
    ///   `metadata_pairs` flow into `set_identity` so the cached source
    ///   payload aligns with the new pointer — reset-to-source can
    ///   replay the seed without divergence. Track count is checked
    ///   against the release's existing track row count; a mismatch
    ///   errors before the identity write so a 12-track release can't
    ///   replace a 10-track rip.
    /// - **Unknown** — empty identities, `metadata_source = file_tags`,
    ///   `metadata_source_release_id = NULL`, no cached payload. Always
    ///   lands the release on a fresh album. The old source's
    ///   album/release/track rows are then reseeded from the local file
    ///   tags in the same call — projecting through the now-`FileTags`
    ///   pointer via [`Self::reset_metadata_to_source`] and writing the
    ///   result with [`Self::apply_release_metadata_user_edit`] — so the
    ///   release stops displaying the prior source's metadata. A
    ///   tag-sparse rip reseeds to blank-but-editable title/artist rather
    ///   than erroring.
    ///
    /// For **Exact / Approximate** the album/release/track row data is not
    /// touched: the identity pointer flips, but the existing rows stay as
    /// the user last had them.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn re_identify_release(
        &self,
        release_id: &str,
        identity_choice: crate::import::IdentityChoice,
    ) -> Result<(), LibraryError> {
        use crate::import::{IdentityChoice, MetadataPointer};

        let (new_identities, metadata_pointer, metadata_pairs) = match &identity_choice {
            IdentityChoice::Exact { release_ref } | IdentityChoice::Approximate { release_ref } => {
                let prepared = crate::import::service::prepare_release(self, release_ref)
                    .await
                    .map_err(LibraryError::Import)?;

                // Source pressing track count must match the local
                // release's row count. The folder-import path enforces
                // the same invariant via prefetch's `track_count_mismatch`
                // flag (which disables the commit button); re-identify
                // bypasses prefetch — the user picks a row directly —
                // so the check belongs here at commit time.
                let existing_track_count = self
                    .database
                    .get_tracks_for_release(release_id)
                    .await?
                    .len();
                let new_track_count = prepared.parsed.tracks.len();
                if existing_track_count != new_track_count {
                    return Err(LibraryError::Import(format!(
                        "Track count mismatch: release has {existing_track_count} tracks, \
                         picked release has {new_track_count}"
                    )));
                }

                let identities = crate::import::service::apply_identity_choice(
                    &prepared.parsed.identities,
                    &identity_choice,
                );
                let pointer = MetadataPointer::External {
                    source: release_ref.source,
                    release_id: release_ref.id.clone(),
                };
                (identities, pointer, prepared.metadata_pairs)
            }
            IdentityChoice::Unknown => (Vec::new(), MetadataPointer::FileTags, Vec::new()),
        };

        self.set_identity(
            release_id,
            new_identities,
            metadata_pointer,
            &metadata_pairs,
        )
        .await?;

        // Unknown flips the pointer to FileTags but leaves the old
        // source's rows in place — they would still display the prior
        // (e.g. MusicBrainz) metadata. Reseed atomically here: project
        // through the now-FileTags pointer and write the result. A
        // tag-sparse rip projects to a blank-but-editable title/artist,
        // which `apply_release_metadata_user_edit` accepts (it rejects
        // only a zero-length artist list, not a blank-named artist).
        if matches!(identity_choice, IdentityChoice::Unknown) {
            let edit = self.reset_metadata_to_source(release_id).await?;
            self.apply_release_metadata_user_edit(release_id, &edit)
                .await?;
        }

        Ok(())
    }

    /// Pick the album the release should land in after a `set_identity`.
    /// See `set_identity` for the policy. Lookup order:
    ///
    /// 1. **Cross-source merge first.** If any other release in the
    ///    library carries an identity row matching one of `new_identities`
    ///    on `(source, source_group_id)`, that release's album is the
    ///    target — the per-source agreement invariant guarantees a
    ///    cross-merging album is unique. Even if the current album
    ///    would also fit, the merge candidate wins because two
    ///    different albums cannot both legitimately claim the same
    ///    group.
    /// 2. **Stay in current** when no merge candidate exists and the
    ///    current album's other releases don't disagree with
    ///    `new_identities` on any shared source.
    /// 3. **Fresh album** otherwise.
    async fn resolve_identity_target_album(
        &self,
        release_id: &str,
        current_album_id: &str,
        new_identities: &[crate::import::ReleaseIdentity],
    ) -> Result<IdentityTargetAlbum, LibraryError> {
        // Unknown — always a fresh album holding only this release.
        if new_identities.is_empty() {
            let new_album = self.fresh_album_for_release(current_album_id).await?;
            return Ok(IdentityTargetAlbum {
                album_id: new_album.id.clone(),
                new_album: Some(new_album),
            });
        }

        // Cross-source merge: any album already holding a release that
        // matches the new identity on at least one source.
        // `find_album_by_identity_group_excluding` ignores rows belonging
        // to `release_id` so the lookup never matches against the very
        // identities we're about to overwrite.
        if let Some(candidate_album_id) = self
            .database
            .find_album_by_identity_group_excluding(new_identities, release_id)
            .await?
        {
            return Ok(IdentityTargetAlbum {
                album_id: candidate_album_id,
                new_album: None,
            });
        }

        // No merge candidate. Stay in the current album if its other
        // releases don't disagree with the new identity on any shared
        // source. An album whose only release is this one trivially
        // agrees.
        let other_identities_in_current = self
            .other_release_identities_for_album(current_album_id, release_id)
            .await?;
        if identities_fit_album(new_identities, &other_identities_in_current) {
            return Ok(IdentityTargetAlbum {
                album_id: current_album_id.to_string(),
                new_album: None,
            });
        }

        // Doesn't fit anywhere. Spin up a fresh album.
        let new_album = self.fresh_album_for_release(current_album_id).await?;
        Ok(IdentityTargetAlbum {
            album_id: new_album.id.clone(),
            new_album: Some(new_album),
        })
    }

    /// Identity rows for every release in an album except `exclude_release_id`.
    /// Each inner Vec is one release's identity rows.
    async fn other_release_identities_for_album(
        &self,
        album_id: &str,
        exclude_release_id: &str,
    ) -> Result<Vec<Vec<crate::import::ReleaseIdentity>>, LibraryError> {
        let releases = self.database.get_releases_for_album(album_id).await?;
        let mut all = Vec::with_capacity(releases.len());
        for release in releases {
            if release.id == exclude_release_id {
                continue;
            }
            let ids = self.database.get_release_identities(&release.id).await?;
            all.push(ids);
        }
        Ok(all)
    }

    /// Build a fresh album row that mirrors `seed_album_id`'s metadata.
    /// Used when `set_identity` needs a brand-new album for the release —
    /// metadata isn't touched by `set_identity`, so the new album reflects
    /// what the release already had. Caller can reseed the metadata.
    async fn fresh_album_for_release(&self, seed_album_id: &str) -> Result<DbAlbum, LibraryError> {
        let source = self
            .database
            .find_album_by_id(seed_album_id)
            .await?
            .ok_or_else(|| {
                LibraryError::Import(format!("Source album '{seed_album_id}' not found"))
            })?;
        let now = self.clock.now();
        Ok(DbAlbum {
            id: self.ids.new_id(),
            title: source.title,
            artist_id: source.artist_id,
            year: source.year,
            // The new album holds only this release; let the move pick
            // up `primary_release_id` lazily via the existing fallback
            // ("first release in the album") rather than hard-coding it
            // here.
            primary_release_id: None,
            is_compilation: source.is_compilation,
            created_at: now,
        })
    }

    /// Insert album, release, and tracks into database in a transaction
    pub async fn insert_album_with_release_and_tracks(
        &self,
        album: &DbAlbum,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_album_with_release_and_tracks(album, release, tracks, metadata, track_artists)
            .await?;
        Ok(())
    }

    pub async fn insert_release_with_tracks(
        &self,
        release: &DbRelease,
        tracks: &[DbTrack],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
    ) -> Result<(), LibraryError> {
        self.database
            .insert_release_with_tracks(release, tracks, metadata, track_artists)
            .await?;
        Ok(())
    }

    /// Load the album id, release, album, and existing tracks for a release
    /// being edited — the shared prelude of `release_edit_seed` and
    /// `apply_release_metadata_user_edit`.
    async fn load_release_for_edit(
        &self,
        release_id: &str,
    ) -> Result<(String, DbRelease, DbAlbum, Vec<DbTrack>), LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Release '{release_id}' not found")))?;
        let album = self
            .database
            .find_album_by_id(&album_id)
            .await?
            .ok_or_else(|| LibraryError::Import(format!("Album '{album_id}' not found")))?;
        let existing_tracks = self.database.get_tracks_for_release(release_id).await?;
        Ok((album_id, release, album, existing_tracks))
    }

    /// Seed the edit form for an existing library release from its current
    /// metadata — the read counterpart to `apply_release_metadata_user_edit`.
    /// Reads the album title and artists, the release pressing fields, and the
    /// per-track titles/sides/numbers/artists, projects them into a wire
    /// `ReleaseUserEdit` describing the current state, then renders that into
    /// the raw editor form via `RawReleaseEdit::from_user_edit`. A track with
    /// no artist rows of its own seeds an empty artist field ("shares the album
    /// artist"); the album artists seed the album artist field.
    pub async fn release_edit_seed(
        &self,
        release_id: &str,
    ) -> Result<crate::import::RawReleaseEdit, LibraryError> {
        let (album_id, release, album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;

        let album_artist_names: Vec<String> = self
            .database
            .get_artists_for_album(&album_id)
            .await?
            .into_iter()
            .map(|a| a.name)
            .collect();

        let mut tracks = Vec::with_capacity(existing_tracks.len());
        for track in &existing_tracks {
            // Empty when the track has no artist rows of its own — the wire edit
            // reads that as "shares the album artist", matching how
            // `apply_release_metadata_user_edit` writes it back.
            let artist_names = self
                .database
                .get_artists_for_track(&track.id)
                .await?
                .into_iter()
                .map(|a| a.name)
                .collect();
            tracks.push(crate::import::TrackUserEdit {
                title: track.title.clone(),
                side: track.side,
                track_number: track.track_number,
                artist_names,
            });
        }

        let edit = crate::import::ReleaseUserEdit {
            album_title: album.title,
            album_artist_names,
            pressing: crate::import::PressingEdit {
                year: release.pressing.year,
                format: release.pressing.format,
                label: release.pressing.label,
                catalog_number: release.pressing.catalog_number,
                country: release.pressing.country,
                barcode: release.pressing.barcode,
            },
            tracks,
        };

        Ok(crate::import::RawReleaseEdit::from_user_edit(
            edit, release_id,
        ))
    }

    /// Apply a user-supplied metadata edit to an existing release: album
    /// title and artists, release pressing fields, and per-track titles,
    /// sides, track numbers, and artists. Resolves artist names against the
    /// library (creating rows for new names), writes the album/release/track
    /// rows and replaces the `album_artists` / `track_artists` junctions, then
    /// emits an `AlbumUpdated` event.
    ///
    /// Track edits align positionally with the release's existing tracks (the
    /// edit can't add or remove tracks — `tracks.len()` must equal the
    /// release's track count). Album artists and per-track artists are
    /// positional lists — the order in `album_artist_names` /
    /// `tracks[i].artist_names` becomes the `position` column on the
    /// `album_artists` / `track_artists` rows.
    ///
    /// `release_metadata` rows, `release_identities`, and the `metadata_source`
    /// columns are deliberately not touched. Identity is orthogonal to
    /// metadata; the cached source payload stays put.
    pub async fn apply_release_metadata_user_edit(
        &self,
        release_id: &str,
        edit: &crate::import::ReleaseUserEdit,
    ) -> Result<(), LibraryError> {
        use crate::db::{DbAlbumArtist, DbArtist, DbTrackArtist};

        if edit.album_artist_names.is_empty() {
            return Err(LibraryError::Import(
                "Album must have at least one artist".to_string(),
            ));
        }

        let (album_id, release, album, existing_tracks) =
            self.load_release_for_edit(release_id).await?;
        if existing_tracks.len() != edit.tracks.len() {
            return Err(LibraryError::Import(format!(
                "Track count mismatch: release has {} tracks, edit supplies {}",
                existing_tracks.len(),
                edit.tracks.len()
            )));
        }

        // Collect every distinct artist name the edit references. The album
        // artists always appear; track-level artists only when the user
        // supplied any (an empty `artist_names` means "same as album artist",
        // no per-track row).
        let mut name_order: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push_name = |name: &str| {
            let key = name.to_lowercase();
            if seen.insert(key) {
                name_order.push(name.to_string());
            }
        };
        for name in &edit.album_artist_names {
            push_name(name);
        }
        for t in &edit.tracks {
            for name in &t.artist_names {
                push_name(name);
            }
        }

        let now = self.clock.now();
        let parsed_artists: Vec<DbArtist> = name_order
            .iter()
            .map(|name| DbArtist {
                id: self.ids.new_id(),
                name: name.clone(),
                sort_name: None,
                discogs_artist_id: None,
                musicbrainz_artist_id: None,
                created_at: now,
            })
            .collect();

        let resolved_ids = self.find_or_create_artists(&parsed_artists).await?;
        let name_to_id: HashMap<String, String> = name_order
            .iter()
            .zip(resolved_ids.iter())
            .map(|(name, id)| (name.to_lowercase(), id.clone()))
            .collect();

        let lookup_artist_id = |name: &str| -> Result<String, LibraryError> {
            name_to_id
                .get(&name.to_lowercase())
                .cloned()
                .ok_or_else(|| {
                    LibraryError::Import(format!("Artist '{name}' missing from resolved map"))
                })
        };

        // The `album.artist_id` FK is the primary album artist; additional
        // artists go in the `album_artists` junction with position >= 1
        // (mirrors the convention in {discogs,musicbrainz}_mapper.rs).
        // `get_artists_for_album` UNIONs the FK row in at sort_key = -1, so
        // including the primary in the junction too would duplicate it.
        let primary_album_artist_id = lookup_artist_id(&edit.album_artist_names[0])?;

        let updated_album = DbAlbum {
            title: edit.album_title.clone(),
            artist_id: primary_album_artist_id,
            ..album.clone()
        };

        let updated_release = DbRelease {
            pressing: Pressing {
                year: edit.pressing.year,
                format: edit.pressing.format.clone(),
                label: edit.pressing.label.clone(),
                catalog_number: edit.pressing.catalog_number.clone(),
                country: edit.pressing.country.clone(),
                barcode: edit.pressing.barcode.clone(),
            },
            ..release.clone()
        };

        let track_updates: Vec<(String, DbTrack)> = existing_tracks
            .iter()
            .zip(edit.tracks.iter())
            .map(|(existing, t)| {
                let updated = DbTrack {
                    title: t.title.clone(),
                    side: t.side,
                    track_number: t.track_number,
                    ..existing.clone()
                };
                (existing.id.clone(), updated)
            })
            .collect();

        let mut album_artists: Vec<DbAlbumArtist> = Vec::new();
        for (i, name) in edit.album_artist_names.iter().enumerate().skip(1) {
            let artist_id = lookup_artist_id(name)?;
            album_artists.push(DbAlbumArtist::new(
                &album_id,
                &artist_id,
                i as i32,
                self.ids.new_id(),
                now,
            ));
        }

        // Track artists have no FK on `tracks` — every artist (primary or
        // additional) goes in `track_artists` with positional ordering.
        let mut track_artists: Vec<DbTrackArtist> = Vec::new();
        for (existing, t) in existing_tracks.iter().zip(edit.tracks.iter()) {
            for (i, name) in t.artist_names.iter().enumerate() {
                let artist_id = lookup_artist_id(name)?;
                track_artists.push(DbTrackArtist::new(
                    &existing.id,
                    &artist_id,
                    i as i32,
                    self.ids.new_id(),
                    now,
                ));
            }
        }

        self.database
            .update_release_metadata_user_edit(
                &album_id,
                release_id,
                &updated_album,
                &updated_release,
                &track_updates,
                &album_artists,
                &track_artists,
            )
            .await?;

        self.emit_album_updated(&album_id).await;

        Ok(())
    }

    /// Add a file to the library
    pub async fn add_file(&self, file: &DbFile) -> Result<(), LibraryError> {
        self.database.insert_file(file).await?;
        Ok(())
    }

    /// Atomically insert all import data in a single transaction.
    /// Nothing is in the DB yet (except the import record and artists).
    /// The release either exists complete or doesn't exist at all.
    ///
    /// Track rows are read straight off `tracks_to_files` — each `TrackFile`
    /// owns the `DbTrack` (with its populated `duration_ms`) that gets
    /// inserted. There is no parallel list of tracks or durations.
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_import_atomic(
        &self,
        album: Option<&DbAlbum>,
        release: &DbRelease,
        tracks_to_files: &[crate::import::TrackFile],
        metadata: &[crate::db::DbReleaseMetadata],
        track_artists: &[crate::db::DbTrackArtist],
        album_artists: &[crate::db::DbAlbumArtist],
        files: &[DbFile],
        audio_formats: &[DbAudioFormat],
        library_image: Option<(&DbLibraryImage, &[u8])>,
        primary_release_id: Option<(&str, &str)>,
        import_id: &str,
        identities: &[crate::import::ReleaseIdentity],
        local_path: &str,
    ) -> Result<(), LibraryError> {
        // The home's storage mode decides the blob layout (opaque hashed-by-id vs.
        // browsable readable paths); the manager owns config, so it reads the mode
        // here rather than threading it from the importer.
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .finalize_import_atomic(
                album,
                release,
                tracks_to_files,
                metadata,
                track_artists,
                album_artists,
                files,
                audio_formats,
                library_image,
                primary_release_id,
                import_id,
                identities,
                local_path,
                storage,
            )
            .await?;
        Ok(())
    }

    /// Get all albums in the library, sorted by the given criteria.
    ///
    /// Pass an empty slice for default sort (newest first).
    pub async fn get_albums(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
    ) -> Result<Vec<DbAlbum>, LibraryError> {
        Ok(self.database.get_albums(sort).await?)
    }

    /// Get a page of albums for lazy loading.
    pub async fn get_album_page(
        &self,
        sort: &[crate::db::AlbumSortCriterion],
        offset: u64,
        limit: u64,
    ) -> Result<Vec<AlbumSummary>, LibraryError> {
        let raws = self.database.get_album_page(sort, offset, limit).await?;
        let release_ids: Vec<String> = raws
            .iter()
            .flat_map(|r| r.release_ids.iter().cloned())
            .collect();
        let covers = self.cover_refs(&release_ids).await?;
        Ok(raws
            .into_iter()
            .map(|raw| resolve_album_summary(raw, |rid| covers.get(rid).cloned()))
            .collect())
    }

    /// Count total albums.
    pub async fn get_album_count(&self) -> Result<u64, LibraryError> {
        Ok(self.database.get_album_count().await?)
    }

    pub async fn get_release_storage_summaries(
        &self,
    ) -> Result<Vec<ReleaseStorageSummary>, LibraryError> {
        let raws = self.database.get_release_storage_summaries().await?;
        let has_cloud_home = self.has_cloud_home();
        let mut out = Vec::with_capacity(raws.len());
        for raw in raws {
            let pinned = self.release_pinned(raw.any_file_id.as_deref()).await?;
            out.push(resolve_release_storage_summary(raw, has_cloud_home, pinned));
        }
        Ok(out)
    }

    /// The storage summary for a single release, or `None` if it doesn't exist.
    /// The download queue reads this at enqueue time for the release's title /
    /// file count / total size and to skip an already-pinned release.
    pub async fn find_release_storage_summary(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseStorageSummary>, LibraryError> {
        let Some(raw) = self
            .database
            .find_release_storage_summary(release_id)
            .await?
        else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let pinned = self.release_pinned(raw.any_file_id.as_deref()).await?;
        Ok(Some(resolve_release_storage_summary(
            raw,
            has_cloud_home,
            pinned,
        )))
    }

    /// Get album by ID
    pub async fn get_album_by_id(&self, album_id: &str) -> Result<Option<DbAlbum>, LibraryError> {
        Ok(self.database.find_album_by_id(album_id).await?)
    }
    pub async fn find_album_detail(
        &self,
        album_id: &str,
    ) -> Result<Option<AlbumDetail>, LibraryError> {
        let Some(raw) = self.database.find_album_detail(album_id).await? else {
            return Ok(None);
        };
        Ok(Some(self.resolve_album_detail(raw).await?))
    }

    /// Resolved release detail for the album-detail view. Composes a
    /// `ReleaseSummary` with tracks/files/gallery loaded by SQL joins,
    /// then derives the release's position in its album so
    /// `display_name` can be computed without the caller supplying an
    /// index. Returns `Ok(None)` when the release doesn't exist.
    pub async fn find_release_detail(
        &self,
        release_id: &str,
    ) -> Result<Option<ReleaseDetail>, LibraryError> {
        let Some(raw) = self.database.find_release_detail(release_id).await? else {
            return Ok(None);
        };
        let has_cloud_home = self.has_cloud_home();
        let album_id = raw.release.album_id.clone();
        let album_artists = self.database.get_artists_for_album(&album_id).await?;
        let releases = self.database.get_releases_for_album(&album_id).await?;
        let release_index = releases
            .iter()
            .position(|r| r.id == release_id)
            .expect("release belongs to its album");
        let pinned = self
            .release_pinned(raw.files.first().map(|f| f.id.as_str()))
            .await?;
        let cover = self.cover_ref(release_id).await?;
        Ok(Some(resolve_release(
            raw,
            &album_artists,
            release_index,
            has_cloud_home,
            pinned,
            cover,
        )))
    }

    /// One page of the Storage Manager list. Rows are returned pre-sorted
    /// and pre-filtered; `total_count` in the returned `StoragePage`
    /// reflects the filtered subset (not the full library).
    pub async fn get_storage_page(
        &self,
        sort: &StorageSort,
        filter: StorageFilter,
        offset: u64,
        limit: u64,
    ) -> Result<StoragePage, LibraryError> {
        let db_sort = to_db_storage_sort(sort);
        let db_filter = to_db_storage_filter(filter);

        let raw_rows = self
            .database
            .get_storage_page(&db_sort, db_filter, offset, limit)
            .await?;
        let total_count = self.database.get_storage_count(db_filter).await?;

        let has_cloud_home = self.has_cloud_home();
        // The cover resolver serves both halves of each row — the release's own id
        // and the album's primary release id — so gather both for the batch lookup.
        let cover_ids: Vec<String> = raw_rows
            .iter()
            .flat_map(|r| {
                [r.release.id.clone()]
                    .into_iter()
                    .chain(r.album.primary_release_id.clone())
                    .chain(r.album.release_ids.iter().cloned())
            })
            .collect();
        let covers = self.cover_refs(&cover_ids).await?;
        let mut rows = Vec::with_capacity(raw_rows.len());
        for raw in raw_rows {
            let pinned = self
                .release_pinned(raw.release.any_file_id.as_deref())
                .await?;
            rows.push(resolve_storage_row(raw, has_cloud_home, pinned, |rid| {
                covers.get(rid).cloned()
            }));
        }
        Ok(StoragePage { rows, total_count })
    }

    /// Count storage rows matching `filter`. Matches `get_storage_page`'s
    /// `total_count` for the same filter.
    pub async fn get_storage_count(&self, filter: StorageFilter) -> Result<u64, LibraryError> {
        let db_filter = to_db_storage_filter(filter);
        Ok(self.database.get_storage_count(db_filter).await?)
    }

    /// Resolve a raw `DbAlbumDetail` into the display-ready `AlbumDetail`.
    /// Joins artist names, formats labels, groups tracks by side, builds
    /// galleries, and applies the `primary_release_id` fallback. The
    /// fallback always succeeds: every album has at least one release
    /// (enforced by `delete_release`).
    async fn resolve_album_detail(
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

/// The release-files [`BlobRef`](coven::BlobRef) addressing a representative
/// file for a coven cache-state query (the pin check via
/// [`CovenHandle::is_pinned`]), which keys only on namespace + id. The other
/// fields carry the release-files constants (see
/// [`LibraryManager::release_file_blob_ref`]); `cloud_path` is `None` because the
/// pin check reads only `storage/pinned/<namespace>/<id>`, never the cloud layout.
fn release_files_pin_ref(file_id: &str) -> coven::BlobRef {
    coven::BlobRef {
        namespace: crate::sync::RELEASE_FILES_NAMESPACE.to_string(),
        id: file_id.to_string(),
        scope: coven::BlobScope::Master,
        cloud_path: None,
        provenance: coven::Provenance::UserProvided,
        fill: coven::CacheFill::CacheLazy,
    }
}

/// Whether `file_id`'s release is pinned offline, answered through the handle's
/// cache-state query. A structurally invalid blob id (e.g. a path-traversal
/// token forged by a peer) can't name a real cached blob, so it reads as not
/// pinned — the id is rejected, never trusted; a real I/O failure still surfaces.
async fn release_file_pinned(handle: &CovenHandle, file_id: &str) -> Result<bool, LibraryError> {
    match handle.is_pinned(&[release_files_pin_ref(file_id)]).await {
        Ok(pinned) => Ok(pinned),
        Err(coven::BlobCacheError::Path(e)) => {
            warn!("pin-state check: rejecting bad blob id {file_id}: {e}");
            Ok(false)
        }
        Err(e) => Err(LibraryError::Storage(format!(
            "pin-state for {file_id}: {e}"
        ))),
    }
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
    /// Get all releases for a specific album
    pub async fn get_releases_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbRelease>, LibraryError> {
        Ok(self.database.get_releases_for_album(album_id).await?)
    }
    /// Get tracks for a specific release
    pub async fn get_tracks(&self, release_id: &str) -> Result<Vec<DbTrack>, LibraryError> {
        Ok(self.database.get_tracks_for_release(release_id).await?)
    }
    /// Get ordered track IDs for a release. Use this when the caller only
    /// needs IDs (queue building, repeat-album rebuild) — avoids pulling
    /// full `DbTrack` rows.
    pub async fn get_track_ids(&self, release_id: &str) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_track_ids_for_release(release_id).await?)
    }
    /// Every track id in the library, in a deterministic base order. Used to
    /// materialize a `ContextSource::Library` context (shuffle library, and the
    /// `Context`-repeat re-derive of a library context).
    pub async fn get_all_track_ids(&self) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.get_all_track_ids().await?)
    }
    /// Return the play context for a track: its release id, the release's full
    /// track order, and the track's index within it. Used by the playback
    /// service to build the queue around a freshly selected track without
    /// chaining library calls.
    pub async fn get_play_context(&self, track_id: &str) -> Result<PlayContext, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release_id = track.release_id;
        let track_ids = self.database.get_track_ids_for_release(&release_id).await?;
        let index = track_ids
            .iter()
            .position(|id| id == track_id)
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!(
                    "Track {} not present in its release {}",
                    track_id, release_id
                ))
            })?;
        Ok(PlayContext {
            release_id,
            track_ids,
            index,
        })
    }

    /// Return the subset of `ids` that still exist in the tracks table.
    /// Used by playback restore to validate a persisted queue in a single
    /// query instead of one round-trip per track.
    pub async fn filter_existing_track_ids(
        &self,
        ids: &[String],
    ) -> Result<Vec<String>, LibraryError> {
        Ok(self.database.filter_existing_track_ids(ids).await?)
    }

    /// Resolve a list of IDs (which may be album IDs or track IDs) into track IDs.
    /// Album IDs are expanded to the track IDs of the album's primary release —
    /// the user's chosen release when set, otherwise the earliest-imported one
    /// (the fallback `primary_release_id` already encodes).
    pub async fn resolve_to_track_ids(&self, ids: &[String]) -> Result<Vec<String>, LibraryError> {
        let mut track_ids = Vec::new();
        for id in ids {
            if let Some(detail) = self.find_album_detail(id).await? {
                if let Some(release) = detail
                    .releases
                    .iter()
                    .find(|r| r.summary.id == detail.primary_release_id)
                {
                    track_ids.extend(release.tracks.iter().map(|t| t.id.clone()));
                }
            } else {
                track_ids.push(id.clone());
            }
        }
        Ok(track_ids)
    }
    pub async fn get_queue_items(
        &self,
        entries: &[QueueEntry],
    ) -> Result<Vec<QueueItem>, LibraryError> {
        Ok(self.database.get_queue_items(entries).await?)
    }
    pub async fn is_source_folder_name_imported(&self, name: &str) -> Result<bool, LibraryError> {
        Ok(self.database.is_source_folder_name_imported(name).await?)
    }

    pub async fn check_releases_in_library(
        &self,
        checks: &[crate::db::LibraryCheck],
    ) -> Result<Vec<crate::db::LibraryStatus>, LibraryError> {
        Ok(self.database.check_releases_in_library(checks).await?)
    }
    /// Get all files for a specific release
    ///
    /// Files belong to releases (not albums or tracks). This includes both:
    /// - Audio files (linked to tracks via db_track_position)
    /// - Metadata files (cover art, CUE sheets, etc.)
    pub async fn get_files_for_release(
        &self,
        release_id: &str,
    ) -> Result<Vec<DbFile>, LibraryError> {
        Ok(self.database.get_files_for_release(release_id).await?)
    }

    /// Bytes of one gallery slot, dispatching the read on its [`GallerySource`]
    /// so no caller picks the byte source itself: a `Cover` is read by image id
    /// (`read_image_blob`), a `ReleaseFile` by file id (`load_gallery_image`).
    /// The gallery carries a cover slot only when a cover exists, so a `Cover`
    /// with no bytes here is exceptional and surfaces rather than being masked.
    pub async fn read_gallery_bytes(
        &self,
        release_id: &str,
        source: &GallerySource,
    ) -> Result<Vec<u8>, LibraryError> {
        match source {
            GallerySource::Cover(image) => {
                self.read_image_blob(&image.id).await?.ok_or_else(|| {
                    LibraryError::Storage(format!("gallery cover image {} has no bytes", image.id))
                })
            }
            GallerySource::ReleaseFile { file_id } => {
                self.load_gallery_image(release_id, file_id).await
            }
        }
    }

    /// Bytes of one of a release's image files, read from the local copy when it
    /// exists here and otherwise downloaded from the release's cloud home (and
    /// decrypted). The `ReleaseFile` arm of [`read_gallery_bytes`](Self::read_gallery_bytes).
    pub async fn load_gallery_image(
        &self,
        release_id: &str,
        file_id: &str,
    ) -> Result<Vec<u8>, LibraryError> {
        let file = self
            .get_files_for_release(release_id)
            .await?
            .into_iter()
            .find(|f| f.id == file_id)
            .ok_or_else(|| {
                LibraryError::Import(format!(
                    "Image file {file_id} is not part of release {release_id}"
                ))
            })?;
        crate::storage::local::transfer::read_release_file_bytes(&file, self)
            .await
            .map_err(|e| LibraryError::Import(e.to_string()))
    }
    /// Get a specific file by ID
    ///
    /// Used during streaming to retrieve the file record after looking up
    /// the track→file relationship via db_track_position.
    pub async fn get_file_by_id(&self, file_id: &str) -> Result<Option<DbFile>, LibraryError> {
        Ok(self.database.find_file_by_id(file_id).await?)
    }
    /// Get audio format for a track
    pub async fn get_audio_format_by_track_id(
        &self,
        track_id: &str,
    ) -> Result<Option<DbAudioFormat>, LibraryError> {
        Ok(self
            .database
            .find_audio_format_by_track_id(track_id)
            .await?)
    }

    /// Resolve a track's audio into a `ResolvedTrackAudio` with its sample window
    /// resolved and all raw `Db*` fields hidden.
    pub async fn resolve_track_audio(
        &self,
        track_id: &str,
    ) -> Result<ResolvedTrackAudio, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        Ok(ResolvedTrackAudio::from_meta(&meta))
    }

    /// Resolve display metadata (artist names, album, cover) for a track at
    /// playback-preparation time. Done here so `PlaybackService` never sees
    /// `DbTrack`.
    pub async fn get_playback_track_info(
        &self,
        track_id: &str,
    ) -> Result<crate::playback::PlaybackTrackInfo, LibraryError> {
        let track = self
            .database
            .find_track_by_id(track_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping(format!("Track not found: {}", track_id)))?;
        let release = self.database.get_release_for_track(&track).await?;
        playback_info_from_track_release(&self.database, &track, &release).await
    }

    /// Resolve both the audio aggregate and the display metadata for a track in
    /// a single pass — avoids the `resolve_track_audio` + `get_playback_track_info`
    /// double-fetch of `DbTrack`/`DbRelease` at playback prep time.
    pub(crate) async fn resolve_track_audio_and_info(
        &self,
        track_id: &str,
    ) -> Result<(ResolvedTrackAudio, crate::playback::PlaybackTrackInfo), LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;
        let audio = ResolvedTrackAudio::from_meta(&meta);
        let info =
            playback_info_from_track_release(&self.database, &meta.track, &meta.release).await?;
        Ok((audio, info))
    }

    /// Get album ID for a release
    pub async fn get_album_id_for_release(&self, release_id: &str) -> Result<String, LibraryError> {
        let album_id = self
            .database
            .find_album_id_for_release(release_id)
            .await?
            .ok_or_else(|| LibraryError::TrackMapping("Release not found".to_string()))?;
        Ok(album_id)
    }
    /// Insert an artist
    pub async fn insert_artist(&self, artist: &DbArtist) -> Result<(), LibraryError> {
        self.database.insert_artist(artist).await?;
        Ok(())
    }
    /// Get artist by Discogs ID (for deduplication)
    pub async fn get_artist_by_discogs_id(
        &self,
        discogs_artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self
            .database
            .get_artist_by_discogs_id(discogs_artist_id)
            .await?)
    }

    /// Get artist by MusicBrainz ID (for deduplication)
    pub async fn get_artist_by_mb_id(&self, mb_id: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_mb_id(mb_id).await?)
    }

    /// Get artist by name (case-insensitive, first match)
    pub async fn get_artist_by_name(&self, name: &str) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.get_artist_by_name(name).await?)
    }

    /// Fill in NULL external IDs on an existing artist (never overwrites)
    pub async fn update_artist_external_ids(
        &self,
        id: &str,
        discogs_id: Option<&str>,
        mb_id: Option<&str>,
        sort_name: Option<&str>,
    ) -> Result<(), LibraryError> {
        Ok(self
            .database
            .update_artist_external_ids(id, discogs_id, mb_id, sort_name)
            .await?)
    }

    /// Insert album-artist relationship
    pub async fn insert_album_artist(
        &self,
        album_artist: &DbAlbumArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_album_artist(album_artist).await?;
        Ok(())
    }
    /// Insert track-artist relationship
    pub async fn insert_track_artist(
        &self,
        track_artist: &DbTrackArtist,
    ) -> Result<(), LibraryError> {
        self.database.insert_track_artist(track_artist).await?;
        Ok(())
    }
    /// Get artists for an album
    pub async fn get_artists_for_album(
        &self,
        album_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_album(album_id).await?)
    }
    /// Get artists for a track
    pub async fn get_artists_for_track(
        &self,
        track_id: &str,
    ) -> Result<Vec<DbArtist>, LibraryError> {
        Ok(self.database.get_artists_for_track(track_id).await?)
    }
    /// Get artist by ID
    pub async fn get_artist_by_id(
        &self,
        artist_id: &str,
    ) -> Result<Option<DbArtist>, LibraryError> {
        Ok(self.database.find_artist_by_id(artist_id).await?)
    }

    /// Resolve each parsed artist to an existing DB row or insert a new one.
    ///
    /// Returns the DB artist ID for each input in the same order, so callers can
    /// zip with `artists` to build a parsed-ID -> DB-ID map.
    ///
    /// Lookup chain: Various Artists alias (cross-source), `discogs_artist_id`,
    /// `musicbrainz_artist_id`, name (case-insensitive) with source-ID conflict
    /// check, then insert. On a match, any new source IDs are accumulated onto
    /// the existing row via COALESCE.
    pub async fn find_or_create_artists(
        &self,
        artists: &[DbArtist],
    ) -> Result<Vec<String>, LibraryError> {
        let mut resolved = Vec::with_capacity(artists.len());

        for artist in artists {
            // 0. Various Artists: match any known VA ID across sources so that
            //    e.g. Discogs "Various" (ID 194) merges with MB "Various Artists".
            let existing = if artist.is_various_artists() {
                let va = &crate::db::VARIOUS_ARTISTS;
                let by_discogs = self.database.get_artist_by_discogs_id(va.discogs).await?;

                if by_discogs.is_some() {
                    by_discogs
                } else {
                    self.database.get_artist_by_mb_id(va.musicbrainz).await?
                }
            } else {
                None
            };

            // 1. Try discogs_artist_id
            let existing = if existing.is_some() {
                existing
            } else if let Some(ref discogs_id) = artist.discogs_artist_id {
                self.database.get_artist_by_discogs_id(discogs_id).await?
            } else {
                None
            };

            // 2. Try musicbrainz_artist_id
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    if let Some(ref mb_id) = artist.musicbrainz_artist_id {
                        self.database.get_artist_by_mb_id(mb_id).await?
                    } else {
                        None
                    }
                }
            };

            // 3. Try name (case-insensitive) with conflict check
            let existing = match existing {
                Some(e) => Some(e),
                None => {
                    let name_match = self.database.get_artist_by_name(&artist.name).await?;

                    match name_match {
                        Some(ref matched) => {
                            let discogs_conflict =
                                match (&matched.discogs_artist_id, &artist.discogs_artist_id) {
                                    (Some(a), Some(b)) => a != b,
                                    _ => false,
                                };
                            let mb_conflict = match (
                                &matched.musicbrainz_artist_id,
                                &artist.musicbrainz_artist_id,
                            ) {
                                (Some(a), Some(b)) => a != b,
                                _ => false,
                            };

                            if discogs_conflict || mb_conflict {
                                debug!(
                                    "Name match for '{}' has conflicting source IDs, inserting new artist",
                                    artist.name
                                );
                                None
                            } else {
                                name_match
                            }
                        }
                        None => None,
                    }
                }
            };

            let actual_id = if let Some(existing_artist) = existing {
                self.database
                    .update_artist_external_ids(
                        &existing_artist.id,
                        artist.discogs_artist_id.as_deref(),
                        artist.musicbrainz_artist_id.as_deref(),
                        artist.sort_name.as_deref(),
                    )
                    .await?;
                existing_artist.id
            } else {
                self.database.insert_artist(artist).await?;
                artist.id.clone()
            };

            resolved.push(actual_id);
        }

        Ok(resolved)
    }

    /// Search across albums and tracks.
    pub async fn search_library(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<SearchResults, LibraryError> {
        let raw = self.database.search_library(query, limit).await?;
        let primary_ids: Vec<String> = raw
            .albums
            .iter()
            .filter_map(|a| a.primary_release_id.clone())
            .collect();
        let covers = self.cover_refs(&primary_ids).await?;
        Ok(resolve_search_results(raw, &covers))
    }

    /// Upsert a library image record
    pub async fn upsert_library_image(&self, image: &DbLibraryImage) -> Result<(), LibraryError> {
        self.database.upsert_library_image(image).await?;
        Ok(())
    }

    /// The readable `cloud_path` for an artist image under the current home:
    /// `None` (hashed-by-id) on an opaque home, `Some({artist}/artist.{ext})`
    /// on a browsable one. The manager owns config, so it reads the storage mode.
    pub fn artist_image_cloud_path(
        &self,
        artist_id: &str,
        content_type: &crate::util::content_type::ContentType,
    ) -> Option<String> {
        let storage = self.config_handle.config().cloud_home.storage;
        self.database
            .artist_image_cloud_path_for_storage(storage, artist_id, content_type)
    }

    /// Get a library image by ID and type
    pub async fn get_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<Option<DbLibraryImage>, LibraryError> {
        Ok(self.database.find_library_image(id, image_type).await?)
    }

    /// Delete a library image by ID and type
    pub async fn delete_library_image(
        &self,
        id: &str,
        image_type: &LibraryImageType,
    ) -> Result<(), LibraryError> {
        self.database.delete_library_image(id, image_type).await?;
        Ok(())
    }

    /// Set an album's cover release (which release provides the cover art)
    pub async fn set_album_primary_release(
        &self,
        album_id: &str,
        primary_release_id: &str,
    ) -> Result<(), LibraryError> {
        self.database
            .set_album_primary_release(album_id, primary_release_id)
            .await?;

        self.emit_album_updated(album_id).await;

        Ok(())
    }

    /// Change the cover art for an album's release.
    ///
    /// `ReleaseImage`: reads an image file already in the library (by file ID),
    /// copies it to the images dir, and records it as the cover.
    /// `RemoteCover`: downloads cover art from a URL, writes it, records it.
    pub async fn change_cover(
        &self,
        album_id: &str,
        release_id: &str,
        selection: CoverSelection,
    ) -> Result<(), LibraryError> {
        let (bytes, content_type, source, source_url) = match selection {
            CoverSelection::ReleaseImage { file_id } => {
                let file = self
                    .get_file_by_id(&file_id)
                    .await?
                    .ok_or_else(|| LibraryError::Import(format!("File '{file_id}' not found")))?;

                // Read the chosen release image file through coven's locality-aware
                // read (the user's own file when Local, the cache/cloud when Remote).
                let bytes = self.read_release_blob(&file).await?;
                let source_url = format!("release://{}", file.original_filename);
                (
                    bytes,
                    file.content_type.clone(),
                    "local".to_string(),
                    Some(source_url),
                )
            }
            CoverSelection::RemoteCover { url, source } => {
                let (bytes, content_type) =
                    crate::import::cover_art::download_cover_art_bytes(&url)
                        .await
                        .map_err(|e| {
                            LibraryError::Import(format!("Failed to download cover: {e}"))
                        })?;

                (bytes, content_type, source.as_str().to_string(), Some(url))
            }
        };

        // Record the cover blob and row in one coven write. The cover's `id` IS
        // the release id. Under a
        // browsable home the cover blob lands at a readable
        // `{artist}/{album}/cover.{ext}` key, computed + stored here; an opaque
        // home leaves `cloud_path` NULL (hashed-by-id).
        let now = self.clock.now();
        let storage = self.config_handle.config().cloud_home.storage;
        let cloud_path = self
            .database
            .cover_cloud_path_for_storage(storage, release_id, &content_type)
            .await?;
        let library_image = DbLibraryImage {
            id: release_id.to_string(),
            image_type: LibraryImageType::Cover,
            content_type,
            file_size: bytes.len() as i64,
            width: None,
            height: None,
            source,
            source_url,
            cloud_path,
            created_at: now,
        };
        self.store_library_image_blob(&library_image, &bytes)
            .await?;

        // Don't touch primary_release_id here — "change cover" updates
        // the image on this release; "set primary release" is a separate
        // user action. Let the event emit so UIs refresh.
        self.emit_album_updated(album_id).await;

        Ok(())
    }

    /// Queue files for a release for deletion (local + cloud).
    ///
    /// Skips local releases -- those are the user's original files. For
    /// remote releases:
    /// - Queues local file deletion if this device pins them
    /// - Adds cloud outbox delete entries for each file
    /// - Cancels any pending uploads for the same files
    async fn queue_release_files_for_deletion(&self, release_id: &str) {
        let release = match self.database.find_release_by_id(release_id).await {
            Ok(Some(r)) => r,
            _ => return,
        };

        let files = match self.get_files_for_release(release_id).await {
            Ok(files) => files,
            Err(e) => {
                warn!("Failed to get files for release {}: {}", release_id, e);
                return;
            }
        };

        if release.remote {
            // Remote: tombstone the cloud blobs and drop coven's cache copies.
            self.queue_storage_deletions(&files).await;
        } else {
            // Local: the files are the user's own files in place — never delete
            // them. Just clear coven's external refs so no orphan ref outlives the
            // release row.
            for file in &files {
                if let Err(e) = self.database.clear_external_blob(&file.id).await {
                    warn!(
                        "Failed to clear external ref for {} on delete: {e}",
                        file.id
                    );
                }
            }
        }
    }

    /// Tombstone every file's cloud blob (cancelling any pending upload first) and
    /// drop coven's local cache copies, for a Remote release that is being deleted.
    ///
    /// SAFETY: the cloud copies are the only ones, so this is only safe when the
    /// release is genuinely being removed. Its sole caller is
    /// `queue_release_files_for_deletion` (the delete path); make-Local
    /// tombstoning is coven's (it enqueues the deletes inside `make_local`'s
    /// atomic commit). `files` are precomputed by the caller, so the cloud keys are
    /// correct.
    pub(crate) async fn queue_storage_deletions(&self, files: &[DbFile]) {
        // Queue cloud outbox deletes and cancel pending uploads. The delete key
        // must match the key the blob was uploaded under, derived through coven
        // for the home's scheme (the row's readable `cloud_path` on a browsable
        // home, the hashed-by-id default on an opaque one).
        for file in files {
            let cloud_key = match self.release_file_cloud_key(file) {
                Ok(key) => key,
                Err(e) => {
                    warn!("Failed to derive delete key for {}: {e}", file.id);
                    continue;
                }
            };

            // Cancel any pending upload for this file
            if let Err(e) = self
                .database
                .remove_cloud_outbox_uploads_for_key(&cloud_key)
                .await
            {
                warn!("Failed to cancel outbox upload for {}: {e}", cloud_key);
            }

            // Queue cloud delete
            if let Err(e) = self.database.add_cloud_outbox_delete(&cloud_key).await {
                warn!("Failed to add outbox delete for {}: {e}", cloud_key);
            }
        }

        // Drop coven's local cache copies (both pinned and evictable folders) so a
        // deleted release leaks nothing on disk. The release is Remote here, so its
        // blobs are cache copies, not external refs. Dropping the on-device cache
        // for a deleted blob is bae's delete-path responsibility. Best-effort: each
        // drop logs and continues so a cleanup hiccup never aborts the delete.
        for file in files {
            if let Err(e) = self
                .handle
                .evict_blob(&Self::release_file_blob_ref(file))
                .await
            {
                warn!(
                    "Failed to drop on-device copies of {} during deletion: {e}",
                    file.id
                );
            }
        }

        self.emit_outbox_changed().await;
    }

    /// Clean up a release's cover blob when the release is deleted. The `covers`
    /// row itself is cascade-deleted with the release (its FK to `releases`), and
    /// that DELETE changeset replicates the removal — and on peers coven's
    /// apply-side cache drop removes their cover copy. This handles the owner's
    /// blob bytes: a Remote release's cover is in the cloud + cache (tombstone the
    /// cloud blob + drop the cache copy), a Local release's cover is in coven's
    /// local store (drop it). Best-effort: each step logs and continues so a
    /// cleanup hiccup never aborts the delete.
    async fn queue_release_cover_for_deletion(&self, release_id: &str, was_remote: bool) {
        let cover = match self
            .database
            .find_library_image(release_id, &LibraryImageType::Cover)
            .await
        {
            Ok(Some(cover)) => cover,
            // No cover: nothing to clean up.
            Ok(None) => return,
            Err(e) => {
                warn!("Failed to look up cover for release {release_id}: {e}");
                return;
            }
        };

        if was_remote {
            // Remote: tombstone the cloud cover blob (its on-device cache copy is
            // dropped below, alongside the Local case).
            match self.cover_cloud_key(release_id, cover.cloud_path.as_deref()) {
                Ok(cloud_key) => {
                    if let Err(e) = self.database.add_cloud_outbox_delete(&cloud_key).await {
                        warn!("Failed to enqueue cover blob delete for {release_id}: {e}");
                    }
                }
                Err(e) => warn!("Failed to derive cover blob key for {release_id}: {e}"),
            }
        }
        // Drop every on-device copy of the cover blob — a Remote release's cache
        // copy or a Local release's local-store copy (it lived in at most one).
        if let Err(e) = self
            .handle
            .evict_blob(&Self::image_blob_ref(
                crate::sync::COVERS_NAMESPACE,
                release_id,
                cover.cloud_path.clone(),
            ))
            .await
        {
            warn!("Failed to drop on-device cover copies for {release_id}: {e}");
        }

        self.emit_outbox_changed().await;
    }

    /// Delete a release and its associated data
    ///
    /// This will:
    /// 1. Queue files for deferred deletion via the pending deletions manifest
    /// 2. Delete the release from database (cascades to tracks, files, etc.)
    /// 3. If this was the last release for the album, also delete the album
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
    pub async fn delete_release(&self, release_id: &str) -> Result<(), LibraryError> {
        let release = self
            .database
            .find_release_by_id(release_id)
            .await?
            .ok_or_else(|| {
                LibraryError::TrackMapping(format!("Release not found: {release_id}"))
            })?;
        let album_id = release.album_id.clone();

        // Collect track IDs before deletion for playback cleanup
        let track_ids: Vec<String> = self
            .get_tracks(release_id)
            .await?
            .into_iter()
            .map(|t| t.id)
            .collect();

        // Queue files for deferred deletion before removing DB records
        self.queue_release_files_for_deletion(release_id).await;
        self.queue_release_cover_for_deletion(release_id, release.remote)
            .await;

        self.database.delete_release(release_id).await?;
        let remaining_releases = self.get_releases_for_album(&album_id).await?;
        let album_deleted = remaining_releases.is_empty();
        if album_deleted {
            self.database.delete_album(&album_id).await?;
        } else if let Some(album) = self.database.find_album_by_id(&album_id).await? {
            if album.primary_release_id.as_deref() == Some(release_id) {
                self.database.clear_album_primary_release(&album_id).await?;
            }
        }

        if !track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted { track_ids });
        }

        if album_deleted {
            // This release was the album's last; it's the only child to drop.
            self.emit_album_removed(&album_id, vec![release_id.to_string()]);
        } else {
            self.emit_album_updated(&album_id).await;
            self.emit_release_removed(&album_id, release_id).await;
        }

        // Drain the local `storage/` copies this release queued for deletion.
        // Matches delete_album/unpin/unmanage; without it a single-release
        // delete of a pinned remote release leaks its remote copies on disk.
        self.spawn_cleanup();

        Ok(())
    }

    /// Remove any releases whose stored content hash equals `hash` — full
    /// remote-file cleanup, primary-release reassignment, album cascade, and
    /// removal events, via [`delete_release`](Self::delete_release) per match.
    /// The import worker calls this before inserting a re-import of the same
    /// folder tree, so the re-import overwrites the prior release(s) instead of
    /// duplicating them.
    pub async fn delete_releases_with_content_hash(&self, hash: &str) -> Result<(), LibraryError> {
        for release_id in self.database.release_ids_for_content_hash(hash).await? {
            self.delete_release(&release_id).await?;
        }
        Ok(())
    }

    /// Delete an album and all its associated data
    ///
    /// This will:
    /// 1. Get all releases for the album
    /// 2. Queue files for deferred deletion via the pending deletions manifest
    /// 3. Delete the album from database (cascades to releases and all related data)
    ///
    /// File cleanup happens asynchronously via the cleanup service, which retries
    /// on failure. This prevents orphaned cloud objects when deletion fails.
    pub async fn delete_album(&self, album_id: &str) -> Result<(), LibraryError> {
        let releases = self.get_releases_for_album(album_id).await?;

        // Collect track IDs from all releases before deletion for playback cleanup
        let mut all_track_ids = Vec::new();
        for release in &releases {
            if let Ok(tracks) = self.get_tracks(&release.id).await {
                all_track_ids.extend(tracks.into_iter().map(|t| t.id));
            }
            self.queue_release_files_for_deletion(&release.id).await;
            self.queue_release_cover_for_deletion(&release.id, release.remote)
                .await;
        }

        self.database.delete_album(album_id).await?;

        if !all_track_ids.is_empty() {
            self.emit(LibraryEvent::TracksDeleted {
                track_ids: all_track_ids,
            });
        }

        self.emit_album_removed(album_id, releases.iter().map(|r| r.id.clone()).collect());

        self.spawn_cleanup();

        Ok(())
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_release(
        &self,
        release_id: &str,
        target_dir: &Path,
    ) -> Result<(), LibraryError> {
        ExportService::export_release(release_id, target_dir, self)
            .await
            .map_err(LibraryError::Import)
    }

    /// Assemble everything `ExportService::export_track` needs for a
    /// track in one pass: source audio bytes, tag fields, cover image path,
    /// neighbour counts, and the raw audio-format aggregate for decoding.
    /// Cloud-only tracks download + decrypt here — export never requires a
    /// local copy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn get_export_track_plan(
        &self,
        track_id: &str,
    ) -> Result<ExportTrackPlan, LibraryError> {
        let meta = TrackAudioMeta::resolve(&self.database, track_id).await?;

        let audio_bytes =
            crate::storage::local::transfer::read_release_file_bytes(&meta.audio_file, self)
                .await
                .map_err(|e| {
                    LibraryError::TrackMapping(format!(
                        "Couldn't read audio for track {track_id}: {e}"
                    ))
                })?;

        let album = self.database.get_album_for_release(&meta.release).await?;

        let album_artists = self.database.get_artists_for_album(&album.id).await?;
        let artist = join_artist_names(&album_artists);

        let release_tracks = self
            .database
            .get_tracks_for_release(&meta.release.id)
            .await?;
        let total_tracks = release_tracks.len();
        let has_multiple_sides = release_tracks
            .iter()
            .map(|t| t.side)
            .collect::<std::collections::HashSet<_>>()
            .len()
            > 1;
        let disc = if has_multiple_sides {
            Some(meta.track.side)
        } else {
            None
        };

        let year = meta.release.pressing.year.or(album.year);

        let cover_image_bytes = match album.primary_release_id.as_deref() {
            Some(rid) => self.read_image_blob(rid).await?,
            None => None,
        };

        let is_digital =
            crate::util::format::is_digital_format(meta.release.pressing.format.as_deref());

        let tags = ExportTags {
            title: meta.track.title.clone(),
            artist,
            album: album.title,
            year,
            disc,
        };

        let track_number = meta.track.track_number;

        Ok(ExportTrackPlan {
            audio_bytes,
            tags,
            cover_image_bytes,
            track_number,
            total_tracks,
            is_digital,
            audio_meta: meta,
        })
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub async fn export_track(
        &self,
        track_id: &str,
        output_path: &Path,
        format: super::ExportFormat,
    ) -> Result<(), LibraryError> {
        let plan = self.get_export_track_plan(track_id).await?;
        ExportService::export_track(plan, output_path, format)
            .await
            .map_err(LibraryError::Import)
    }
    /// Insert a new import operation record
    pub async fn insert_import(&self, import: &DbImport) -> Result<(), LibraryError> {
        Ok(self.database.insert_import(import).await?)
    }
    /// Update the status of an import operation
    pub async fn update_import_status(
        &self,
        id: &str,
        status: ImportOperationStatus,
    ) -> Result<(), LibraryError> {
        Ok(self.database.update_import_status(id, status).await?)
    }
    /// Record an error for an import operation
    pub async fn update_import_error(&self, id: &str, error: &str) -> Result<(), LibraryError> {
        Ok(self.database.update_import_error(id, error).await?)
    }
    /// Get all active (non-complete, non-failed) imports
    pub async fn get_active_imports(&self) -> Result<Vec<DbImport>, LibraryError> {
        Ok(self.database.get_active_imports().await?)
    }

    /// Delete an import record (used by UI to dismiss stuck imports)
    pub async fn delete_import(&self, id: &str) -> Result<(), LibraryError> {
        Ok(self.database.delete_import(id).await?)
    }

    // ── Download (pin) queue ─────────────────────────────────────────
    //
    // Pinning routes through an in-memory serial queue instead of an ephemeral
    // per-release task: one release downloads at a time, the rest wait, and the
    // user can pause/cancel/retry. The queue is transient — on restart it's
    // empty and any release that wasn't fully pinned stays cloud-only (a pin
    // flips a release to pinned only after every file lands; see `do_pin`).

    /// Enqueue releases to pin for offline. Skips ids already in the queue (any
    /// state) or already pinned; for each new one, resolves its title /
    /// file_count / total_size from its storage summary so the Downloads pane
    /// can render the row without a re-query. Spawns the single worker on the
    /// first enqueue, then wakes it. Emits a fresh `DownloadQueueChanged`.
    pub async fn enqueue_pins(&self, release_ids: Vec<String>) {
        // One timestamp for the whole batch — read the clock once, not per row.
        let enqueued_at = self.clock.now().timestamp_millis();
        let mut added_any = false;
        for release_id in release_ids {
            if self.download_queue.contains(&release_id) {
                debug!("enqueue_pins: {release_id} already queued, skipping");
                continue;
            }
            let summary = match self.find_release_storage_summary(&release_id).await {
                Ok(Some(summary)) => summary,
                Ok(None) => {
                    warn!("enqueue_pins: release {release_id} not found, skipping");
                    continue;
                }
                Err(e) => {
                    warn!("enqueue_pins: failed to load storage summary for {release_id}: {e}");
                    continue;
                }
            };
            if summary.pinned {
                debug!("enqueue_pins: {release_id} already pinned, skipping");
                continue;
            }

            let op = crate::library::DownloadOp {
                release_id: release_id.clone(),
                title: summary.album_title,
                file_count: summary.file_count,
                total_size: summary.total_size,
                created_at: enqueued_at,
                state: crate::library::DownloadState::Queued,
            };
            if self.download_queue.enqueue(op) {
                added_any = true;
            }
        }

        if added_any {
            self.ensure_download_worker();
            self.download_queue.wake();
            self.emit_download_queue_changed();
        }
    }

    /// Pause or resume the download queue. While paused the worker parks instead
    /// of starting the next release; the in-flight one runs to completion.
    /// Resuming wakes the worker. Emits a fresh `DownloadQueueChanged`.
    pub fn set_downloads_paused(&self, paused: bool) {
        let was_paused = self.download_queue.set_paused(paused);
        if was_paused && !paused {
            // Resume: wake the parked worker so it picks up the next release
            // immediately rather than waiting for the next enqueue.
            self.download_queue.wake();
        }
        self.emit_download_queue_changed();
    }

    /// Cancel a release's download. Drops a queued/failed entry; for the active
    /// one, aborts its in-flight pin task (the `.part` temp file it was writing
    /// is left behind — a partial never renames into place, so the release stays
    /// cloud-only). Emits a fresh snapshot.
    pub fn cancel_download(&self, release_id: &str) {
        let was_active = self.download_queue.cancel(release_id);
        if was_active {
            // The aborted pin task closes its progress channel; the worker's
            // drain sees the close and emits the terminal `ReleaseTransferEnded`
            // that clears the inline storage-row bar, then sees the entry is
            // gone and leaves the queue as-is. So we don't emit it here — and
            // the worker re-parks on its own once the drain returns, so no wake
            // is needed for the active case.
        }
        self.emit_download_queue_changed();
    }

    /// Flip every failed download back to queued and wake the worker to retry
    /// them. Emits a fresh `DownloadQueueChanged`.
    pub fn retry_downloads(&self) {
        if self.download_queue.retry_failed() {
            self.download_queue.wake();
        }
        self.emit_download_queue_changed();
    }

    /// Spawn the single serial download worker if it isn't running yet. Claimed
    /// exactly once across all manager clones; safe to call on every enqueue.
    fn ensure_download_worker(&self) {
        if self.download_queue.claim_worker_spawn() {
            let manager = self.clone();
            self.runtime_handle.spawn(async move {
                manager.run_download_worker().await;
            });
        }
    }

    /// The serial download worker loop. Parks on the queue's `Notify` whenever
    /// the queue is paused or holds nothing queued; otherwise takes the next
    /// queued release, runs its pin, and repeats. Process strictly one release
    /// at a time.
    async fn run_download_worker(&self) {
        loop {
            // `next_queued_release` returns `None` while paused or empty, so this
            // one check covers both — park until an enqueue, resume, or retry
            // wakes us. `run_queued_pin` flips the picked release to Active.
            let Some(release_id) = self.download_queue.next_queued_release() else {
                self.download_queue.wait().await;
                continue;
            };
            self.run_queued_pin(&release_id).await;
        }
    }

    /// Run one queued release's pin: spawn `TransferService::pin_release_task`,
    /// flip the entry to `Active` and register its abort handle atomically, then
    /// drive its progress (folding per-file percent into the release's
    /// `Active { percent }` and re-emitting the inline `ReleaseTransferProgress`
    /// the storage row reads). On success drop the entry; on failure mark it
    /// `Failed` (it stays in the queue for retry).
    ///
    /// `cancel_download` aborts the in-flight task via the registered handle. A
    /// cancel removes the queue entry; on its way out the drain sees the channel
    /// close, and we check whether the entry is still present before recording a
    /// failure — a cancelled download isn't a failure.
    async fn run_queued_pin(&self, release_id: &str) {
        use crate::storage::local::transfer::TransferService;

        let transfer = TransferService::new(self.clone());
        let (rx, pin_task) = transfer.pin_release_task(release_id.to_string());
        let abort = pin_task.abort_handle();
        // Flip to Active and register the abort handle atomically. If a cancel
        // removed the entry in the gap since we picked it, abort the task we
        // just spawned and bail — the release stays cloud-only.
        if !self.download_queue.activate(release_id, abort.clone()) {
            abort.abort();
            debug!("Pin for {release_id} cancelled before it started; aborting");
            return;
        }
        self.emit_download_queue_changed();

        // Drive the pin through the shared transfer driver; the progress hook
        // folds each overall percent into the queue snapshot (the inline
        // `ReleaseTransferProgress` bar is emitted by `drive_transfer` itself).
        let outcome = self
            .drive_transfer(release_id, ReleaseStorageAction::Pin, rx, |overall| {
                self.download_queue.set_active_percent(release_id, overall);
                self.emit_download_queue_changed();
            })
            .await;
        self.download_queue.clear_active_abort();

        // A cancel removed the entry while the pin was in flight. The drain
        // ended with an Err (the aborted pin task closed its channel) and
        // already emitted the terminal `ReleaseTransferEnded` that clears the
        // inline bar; `cancel_download` emitted the fresh snapshot. This isn't
        // a failure — don't re-add the entry or mark it Failed.
        if !self.download_queue.contains(release_id) {
            debug!("Pin for {release_id} ended after cancel; leaving queue as-is");
            return;
        }

        match outcome {
            Ok(()) => {
                // The release is pinned. `do_pin` (via `pin_release_blobs`) already
                // emitted `ReleaseUpdated`, so its `pinned` flag flips true
                // reactively — just drop the queue entry.
                self.download_queue.remove(release_id);
                self.emit_download_queue_changed();
            }
            Err(error) => {
                error!("Pin failed for release {release_id}: {error}");
                self.download_queue.mark_failed(release_id, error);
                self.emit_download_queue_changed();
            }
        }
    }

    /// Unpin a release: delete local copies, mark as cloud-only.
    pub async fn unpin_release(&self, release_id: &str) -> Result<(), String> {
        let transfer_service = crate::storage::local::transfer::TransferService::new(self.clone());
        let rx = transfer_service.unpin_release(release_id.to_string());
        let result = self
            .drive_transfer(release_id, ReleaseStorageAction::Unpin, rx, |_| {})
            .await;
        if result.is_ok() {
            self.spawn_cleanup();
        }
        result
    }

    /// Manage a local release: upload its files to the cloud home. `pin`
    /// chooses whether coven keeps the blobs in `storage/pinned/` (offline) vs the
    /// evictable cache. The in-place source is always deleted once the upload lands
    /// (a remote release has no local path — see `transfer::do_make_remote`).
    pub async fn make_release_remote(&self, release_id: &str, pin: bool) -> Result<(), String> {
        let transfer_service = crate::storage::local::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_remote(release_id.to_string(), pin);
        self.drive_transfer(release_id, ReleaseStorageAction::MakeRemote, rx, |_| {})
            .await
    }

    /// Unmanage a remote release: copy its files back out to `new_path` and
    /// drop the remote copies. See `transfer::do_make_local` for the
    /// durability-first ordering (every copy is verified at the new path before
    /// any delete is queued).
    pub async fn make_release_local(&self, release_id: &str, new_path: &str) -> Result<(), String> {
        // Register a cancellation token so `cancel_release_transition` can stop
        // this transfer; the guard deregisters even if this future is dropped.
        let cancel = crate::library::CancellationToken::new();
        self.transfer_cancels
            .lock()
            .unwrap()
            .insert(release_id.to_string(), cancel.clone());
        let _dereg = TransferCancelGuard {
            registry: self.transfer_cancels.clone(),
            release_id: release_id.to_string(),
        };

        let transfer_service = crate::storage::local::transfer::TransferService::new(self.clone());
        let rx = transfer_service.make_release_local(
            release_id.to_string(),
            new_path.to_string(),
            cancel,
        );
        let result = self
            .drive_transfer(release_id, ReleaseStorageAction::MakeLocal, rx, |_| {})
            .await;
        if result.is_ok() {
            self.spawn_cleanup();
        }
        result
    }

    /// Cancel the in-progress transition for a release, whatever it is: a pin
    /// (download), a remote upload, or an unmanage. The UI calls this from the
    /// storage row and the queue pane without knowing which is running — a
    /// release is in at most one transition at a time. A no-op if nothing is in
    /// progress. Each branch is gated on the transition actually running:
    /// `cancel_release_upload` on a settled release would delete its blobs, so it
    /// fires only when uploads are genuinely pending.
    pub async fn cancel_release_transition(&self, release_id: &str) -> Result<(), LibraryError> {
        if self.cancel_transfer(release_id) {
            return Ok(());
        }
        if self.download_queue.contains(release_id) {
            self.cancel_download(release_id);
            return Ok(());
        }
        if self
            .database
            .has_pending_uploads_for_release(release_id)
            .await?
        {
            return self.cancel_release_upload(release_id).await;
        }
        Ok(())
    }

    /// Fire the cancellation token for a release's in-progress foreground
    /// transfer (unmanage), if one is registered; returns whether it fired. The
    /// transfer observes the token between files, deletes its partial copies, and
    /// leaves the release remote. A missing token is not an error — it means no
    /// transfer is running, so the caller falls through to the other transition
    /// kinds. The lookup and fire share one lock, so there's no check-then-act
    /// race with the deregistering drop guard.
    fn cancel_transfer(&self, release_id: &str) -> bool {
        match self.transfer_cancels.lock().unwrap().get(release_id) {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    /// Drain a transfer's progress channel, translating each non-terminal
    /// `TransferProgress` into a `ReleaseTransferProgress` UI event and emitting
    /// `ReleaseTransferEnded` on completion or failure. The overall percent is
    /// the per-file percent folded across the file count, computed here so the
    /// UI renders a single figure without re-deriving it. Returns the failure
    /// error string (also surfaced to the caller) on `Failed`.
    ///
    /// `on_overall` is called with each new overall percent before it's emitted,
    /// so the download queue worker can fold the same figure into its snapshot;
    /// the foreground unpin/manage/unmanage transitions pass a no-op.
    async fn drive_transfer(
        &self,
        release_id: &str,
        action: ReleaseStorageAction,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<
            crate::storage::local::transfer::TransferProgress,
        >,
        mut on_overall: impl FnMut(u8),
    ) -> Result<(), String> {
        use crate::storage::local::transfer::TransferProgress;

        // The bridge transfer future is abortable: a view dismiss / re-trigger
        // can drop this future between progress events, before its terminal
        // `ReleaseTransferEnded` emits. The guard fires that event on drop so a
        // cancelled transfer never freezes the progress bar on the release row;
        // the normal exit defuses it after emitting the event itself.
        let mut ended_guard = TransferEndedGuard {
            event_tx: self.event_tx.clone(),
            release_id: release_id.to_string(),
            armed: true,
        };

        let emit_progress = |percent: u8, file_no: Option<u32>, total: Option<u32>| {
            self.emit(LibraryEvent::ReleaseTransferProgress {
                release_id: release_id.to_string(),
                action,
                file_no,
                total,
                percent,
            });
        };

        let outcome = loop {
            let Some(progress) = rx.recv().await else {
                break Err(format!(
                    "{} ended without completion or failure",
                    verb(action)
                ));
            };
            match progress {
                TransferProgress::Started { .. } => {
                    // The first file count isn't known to be > 1 yet, and a bare
                    // "X of N" with file 1 reads as redundant; show the action
                    // alone until the first FileProgress reports real position.
                    emit_progress(0, None, None);
                }
                TransferProgress::FileProgress {
                    file_index,
                    total_files,
                    percent,
                    ..
                } => {
                    // `total_files` is never zero: every producer (pin / manage /
                    // unmanage) rejects an empty file list before its per-file
                    // loop, and FileProgress is only emitted from inside that loop.
                    let overall =
                        ((file_index as u32 * 100 + percent as u32) / total_files as u32) as u8;
                    on_overall(overall);
                    emit_progress(
                        overall,
                        Some(file_index as u32 + 1),
                        Some(total_files as u32),
                    );
                }
                TransferProgress::Complete { .. } => break Ok(()),
                TransferProgress::Failed { error, .. } => break Err(error),
            }
        };

        // Normal exit: emit the terminal event ourselves and defuse the guard so
        // its drop doesn't emit a second one.
        self.emit(LibraryEvent::ReleaseTransferEnded {
            release_id: release_id.to_string(),
        });
        ended_guard.defuse();
        outcome
    }

    // =========================================================================
    // Sync provider configuration
    // =========================================================================

    /// Whether the background sync loop is running and draining uploads. The
    /// manage gate requires this: managing has no inline remote flip — the
    /// release only becomes remote once the upload observer (which fires from
    /// inside the running loop) confirms the last upload landed.
    pub fn is_sync_ready(&self) -> bool {
        self.sync_connected() && self.handle.is_syncing()
    }

    pub fn trigger_sync(&self) {
        self.handle.sync_now();
    }

    pub async fn save_s3_config(&self, data: S3ConfigData) -> Result<(), String> {
        use crate::keys::CloudHomeCredentials;
        use coven::CloudHome;
        use coven::S3CloudHome;

        // Probe the bucket with the proposed credentials *before* persisting
        // anything. A typo or a missing bucket would otherwise leave the UI
        // showing "Connected" and the user discovering broken sync only via
        // the reconnect banner after the first failed cycle.
        let probe_home = S3CloudHome::new(
            data.bucket.clone(),
            data.region.clone(),
            data.endpoint.clone(),
            data.access_key.clone(),
            data.secret_key.clone(),
            data.key_prefix.clone(),
        )
        .await
        .map_err(|e| format!("Failed to build S3 client: {e}"))?;
        probe_home.probe().await.map_err(|e| format!("{e}"))?;

        let creds = CloudHomeCredentials::S3 {
            access_key: data.access_key,
            secret_key: data.secret_key,
        };
        self.key_service
            .set_cloud_home_credentials(&creds)
            .map_err(|e| format!("Failed to save credentials: {e}"))?;

        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::S3);
                c.cloud_home.s3_bucket = Some(data.bucket);
                c.cloud_home.s3_region = Some(data.region);
                c.cloud_home.s3_endpoint = data.endpoint.filter(|s| !s.is_empty());
                c.cloud_home.s3_key_prefix = data.key_prefix.filter(|s| !s.is_empty());
                c.cloud_home.storage = data.storage;
            })
            .map_err(|e| format!("Failed to save config: {e}"))?;

        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;

        info!("Saved S3 sync configuration");
        Ok(())
    }

    #[cfg(feature = "oauth-providers")]
    pub async fn sign_in_cloud_provider(
        &self,
        provider: CloudProvider,
        storage: crate::config::HomeStorage,
    ) -> Result<(), String> {
        use coven::{sign_in_dropbox, sign_in_google_drive, sign_in_onedrive};

        // Hold the sender alive across the await so cancel.wait_for inside
        // oauth::authorize never fires (this fn doesn't surface a cancel
        // signal). When this future is dropped, oauth::authorize's own
        // AbortOnDrop guard tears the listener task down.
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let library_name = self.config_handle.config().library_name.clone();
        let clock = self.clock.as_ref();
        // coven's sign-ins authorize, resolve the cloud folder, and save tokens to
        // the keyring, returning the folder identifiers; bae persists them here.
        match provider {
            CloudProvider::GoogleDrive => {
                let folder_id =
                    sign_in_google_drive(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::GoogleDrive);
                        c.cloud_home.google_drive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::Dropbox => {
                let folder_path =
                    sign_in_dropbox(&self.key_service, &library_name, cancel_rx, clock)
                        .await
                        .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::Dropbox);
                        c.cloud_home.dropbox_folder_path = Some(folder_path);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            CloudProvider::OneDrive => {
                let (drive_id, folder_id) = sign_in_onedrive(&self.key_service, cancel_rx, clock)
                    .await
                    .map_err(|e| e.to_string())?;
                self.config_handle
                    .update(move |c| {
                        c.cloud_home.provider = Some(CloudProvider::OneDrive);
                        c.cloud_home.onedrive_drive_id = Some(drive_id);
                        c.cloud_home.onedrive_folder_id = Some(folder_id);
                        c.cloud_home.storage = storage;
                    })
                    .map_err(|e| e.to_string())?;
            }
            _ => return Err("This provider does not use OAuth sign-in".to_string()),
        }
        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;
        Ok(())
    }

    pub async fn use_cloudkit(&self, storage: crate::config::HomeStorage) -> Result<(), String> {
        self.config_handle
            .update(move |c| {
                c.cloud_home.provider = Some(CloudProvider::CloudKit);
                c.cloud_home.storage = storage;
            })
            .map_err(|e| format!("Failed to save CloudKit config: {e}"))?;
        self.ensure_sync_manager_and_start().await?;
        // A cloud home now exists, so every release gains its storage actions.
        self.emit_all_albums_updated().await;

        info!("Configured CloudKit cloud provider");
        Ok(())
    }

    pub fn disconnect_cloud_provider(&self) -> Result<(), String> {
        // Stop the sync loop and drop the installed manager; the library becomes
        // home-less until the next connect.
        self.handle.disconnect_sync();
        self.sync_connected
            .store(false, std::sync::atomic::Ordering::SeqCst);
        *self.encryption_service.write().unwrap() = None;

        // Connecting fills the whole cloud home; disconnecting clears it as a unit.
        self.config_handle
            .update(|c| c.cloud_home = Default::default())
            .map_err(|e| e.to_string())?;

        // Clearing the cloud-home credentials from the keyring is coven's concern.
        if let Err(e) = self.key_service.delete_cloud_home_credentials() {
            tracing::warn!("Failed to delete cloud home credentials: {e}");
        }

        // The cloud home is gone, so releases lose their storage actions; re-emit
        // every album so cached UI details drop the now-invalid actions. Spawned
        // because this fn is sync and the re-emit re-resolves each album (async).
        let manager = self.clone();
        self.runtime_handle.spawn(async move {
            manager.emit_all_albums_updated().await;
        });
        Ok(())
    }

    /// Warning text to append to the disconnect-sync confirmation when the
    /// library has releases reachable only through cloud sync — remote and not
    /// pinned in coven's cache. Returns `None` when no releases are at risk, so the
    /// dialog just shows its base message. Asks coven's cache per remote release
    /// (a representative blob in `storage/pinned/`); pinned-ness is coven cache
    /// state, never a bae column.
    pub async fn disconnect_warning_message(&self) -> Result<Option<String>, String> {
        let remote_file_ids = self
            .database
            .get_remote_release_file_ids()
            .await
            .map_err(|e| format!("list remote releases: {e}"))?;
        let mut count: u64 = 0;
        for any_file_id in &remote_file_ids {
            if !self
                .release_pinned(any_file_id.as_deref())
                .await
                .map_err(|e| format!("pin-state check: {e}"))?
            {
                count += 1;
            }
        }
        Ok(match count {
            0 => None,
            1 => Some(
                "1 release is only stored in the cloud — it will become unplayable \
                 until you reconnect."
                    .to_string(),
            ),
            n => Some(format!(
                "{n} releases are only stored in the cloud — they will become \
                 unplayable until you reconnect."
            )),
        })
    }

    /// Build, start, and attach a sync manager. Used once at startup for a
    /// returning user with a configured cloud home: an unlocked key for an opaque
    /// home (`Some`), or no key for a browsable home (`None`). Shares this manager's
    /// outbox in-flight set and event channel with the sync loop's upload
    /// observer. Call before [`Self::start`].
    pub async fn attach_and_start_sync(
        &self,
        encryption_service: Option<EncryptionService>,
    ) -> Result<(), String> {
        self.handle.connect_sync(encryption_service.clone()).await?;
        *self.encryption_service.write().unwrap() = encryption_service;
        self.sync_connected
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    /// Ensure a SyncManager exists (creating encryption key if needed) and start sync.
    async fn ensure_sync_manager_and_start(&self) -> Result<(), String> {
        // If we already have a sync manager, just (re)start its loop.
        if self.sync_connected() {
            self.handle.start_sync().await?;
            return Ok(());
        }

        // An opaque home mints (or reuses) the library key and seals every object
        // under it; a browsable home stores in the clear and has no key at all.
        // Build the encryption service only for an opaque home, so a browsable
        // home never mints a key it would never use. `get_or_create_encryption_key`
        // is idempotent, so a retry after a failed sync init reuses the key.
        let storage = self.config_handle.config().cloud_home.storage;
        let (enc_service, fingerprint) = if storage.is_opaque() {
            let enc_key_hex = self
                .key_service
                .get_or_create_encryption_key()
                .map_err(|e| format!("Failed to create encryption key: {e}"))?;
            let enc = EncryptionService::new(&enc_key_hex)
                .map_err(|e| format!("Failed to create encryption service: {e}"))?;
            let fingerprint = enc.fingerprint();
            (Some(enc), Some(fingerprint))
        } else {
            (None, None)
        };

        // Connect the provider: build the cloud home, start the loop, and install
        // the manager. A cloud-home build or loop-start failure returns `Err` with
        // nothing installed, so it surfaces here rather than leaving a dead manager
        // — and the encryption-key fingerprint below is reached only on success, so
        // a failed setup stays a clean retry (no fingerprint telling the next
        // launch's unlock flow "encryption is set up" while sync is still broken).
        self.handle.connect_sync(enc_service.clone()).await?;
        *self.encryption_service.write().unwrap() = enc_service;
        self.sync_connected
            .store(true, std::sync::atomic::Ordering::SeqCst);

        // Sync started. For an opaque home, persist the encryption-key hint flag so
        // the next launch's unlock flow knows this library has encryption set up. A
        // browsable home records nothing — it has no key, so `encryption_key_stored`
        // stays false and the next launch builds it keyless.
        if let Some(fingerprint) = fingerprint {
            if let Err(e) = self
                .config_handle
                .record_encryption_key_fingerprint(fingerprint)
            {
                self.handle.disconnect_sync();
                *self.encryption_service.write().unwrap() = None;
                self.sync_connected
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                return Err(format!("Failed to save config: {e}"));
            }
        }

        self.trigger_sync();

        Ok(())
    }
}

#[cfg(test)]
mod tests;
