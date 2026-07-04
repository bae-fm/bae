//! Resolved Storage Manager types (`ReleaseStorageSummary`, `StorageRow`,
//! `StoragePage`) plus the sort/filter inputs they're paged by, and the pure
//! projections that produce them.

use super::*;
use crate::db::{DbReleaseStorageSummary, DbStorageRow};

/// Resolved per-release storage summary for the Storage Manager view.
/// Produced by `LibraryManager` from `DbReleaseStorageSummary`: derives
/// `storage_state` from `releases.remote` and this device's
/// `release_local_copy` row. The UI formats `total_size` for the locale.
#[derive(Debug, Clone)]
pub struct ReleaseStorageSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_names: String,
    pub format: Option<String>,
    /// The album's primary release (for "am I the primary release"
    /// comparisons and cover art lookup). Always set: every album has at
    /// least one release.
    pub primary_release_id: String,
    /// The release's storage state — Local (local) or Remote (cloud) —
    /// derived from the shared `releases.remote` fact. Orthogonal to `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the orthogonal coven-cache property. Meaningful only when
    /// `storage_state` is `Remote`. Kept SEPARATE from `storage_state`.
    pub pinned: bool,
    /// The storage transitions this release allows now, gated on cloud-home
    /// only. The in-flight-uploads gate lives in the UI: it suppresses these
    /// actions when the outbox snapshot has a group for this release.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
}

impl ReleaseStorageSummary {
    /// Produces a resolved `ReleaseStorageSummary` from a raw
    /// `DbReleaseStorageSummary`: derives `storage_state` from `remote` and `pinned`
    /// (the caller asks coven's cache whether the release's blobs are pinned). The raw
    /// `primary_release_id` comes from SQL's `COALESCE(a.primary_release_id,
    /// <first release id>)` and is non-null by construction: every album has at
    /// least one release (enforced by `delete_release`).
    pub(crate) fn from_raw(
        raw: DbReleaseStorageSummary,
        has_cloud_home: bool,
        pinned: bool,
    ) -> ReleaseStorageSummary {
        let storage_state = storage_state(raw.remote);
        let storage_actions = available_storage_actions(storage_state, pinned, has_cloud_home);
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
}

/// One row on the Storage Manager view: a release paired with its parent
/// album. The UI normalizes the shape into two slices (releases +
/// summaries) at ingest — rendering joins from the release to the album
/// at render time.
#[derive(Debug, Clone)]
pub struct StorageRow {
    pub release: ReleaseSummary,
    pub album: AlbumSummary,
}

impl StorageRow {
    /// Resolve a raw storage-page row: releases and their parent albums
    /// arrive pre-joined from SQL; each half maps to its summary projection.
    /// `resolve_cover` maps an image id (the release's own id, and the album's
    /// primary release id) to its cover reference; it serves both halves so the
    /// release row carries its own art and the album carries the primary's.
    pub(crate) fn from_raw(
        raw: DbStorageRow,
        has_cloud_home: bool,
        pinned: bool,
        resolve_cover: impl Fn(&str) -> Option<ImageRef>,
    ) -> StorageRow {
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            pinned,
            cover: resolve_cover(&raw.release.id),
        };
        StorageRow {
            release: ReleaseSummary::from_raw(raw.release, &ctx),
            album: AlbumSummary::from_raw(raw.album, &resolve_cover),
        }
    }
}

/// One page of storage rows with the total count so paginated list
/// machinery knows where the end is. `total_count` reflects the filtered
/// subset, not the full library.
#[derive(Debug, Clone)]
pub struct StoragePage {
    pub rows: Vec<StorageRow>,
    pub total_count: u64,
}

/// Field the Storage Manager can sort on. Mirrors the columns
/// `StorageManagerView` renders today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSortDirection {
    Ascending,
    Descending,
}

/// A single sort criterion for `get_storage_page`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSort {
    pub field: StorageSortField,
    pub direction: StorageSortDirection,
}

/// Filter applied to the Storage Manager list. Mirrors the four
/// mutually-exclusive filter chips the UI shows today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    /// Only local releases (files live outside the library directory).
    Local,
    /// Only remote releases (files stored by the library).
    Remote,
    /// Only releases with at least one file pending cloud upload.
    Uploading,
}
