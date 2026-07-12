//! Resolved Storage Manager types (`ReleaseStorageSummary`, `StorageRow`,
//! `StoragePage`) plus the sort/filter inputs they're paged by, and the pure
//! projections that produce them.

use super::*;
use crate::db::{DbReleaseStorageSummary, DbStorageRow};

/// Resolved per-release storage summary for the Storage Manager view. The UI
/// formats `total_size` for the locale.
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
    /// `storage_state` derives from `remote` alone; `pinned` is the orthogonal
    /// coven-cache property the caller reads separately. The raw
    /// `primary_release_id` comes from SQL's `COALESCE(a.primary_release_id, <first
    /// release id>)`, non-null by construction: every album has at least one
    /// release, enforced by `delete_release`.
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

/// One row on the Storage Manager view: a release with its parent album. The UI
/// splits it into two slices at ingest and re-joins at render time.
#[derive(Debug, Clone)]
pub struct StorageRow {
    pub release: ReleaseSummary,
    pub album: AlbumSummary,
}

impl StorageRow {
    /// The release and its album arrive pre-joined from SQL; each half maps to its
    /// own summary projection. `resolve_cover` serves both, so the release row
    /// carries its own art and the album carries its primary release's.
    pub(crate) fn from_raw(
        raw: DbStorageRow,
        has_cloud_home: bool,
        pinned: bool,
        transfer_action: Option<ReleaseStorageAction>,
        resolve_cover: impl Fn(&str) -> Option<ImageRef>,
    ) -> StorageRow {
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            pinned,
            cover: resolve_cover(&raw.release.id),
            transfer_action,
        };
        StorageRow {
            release: ReleaseSummary::from_raw(raw.release, &ctx),
            album: AlbumSummary::from_raw(raw.album, &resolve_cover),
        }
    }
}

/// One page of storage rows. `total_count` is of the *filtered* subset, not the
/// whole library — it's what tells the paginated list where the end is.
#[derive(Debug, Clone)]
pub struct StoragePage {
    pub rows: Vec<StorageRow>,
    pub total_count: u64,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSort {
    pub field: StorageSortField,
    pub direction: StorageSortDirection,
}

/// The Storage Manager's filter chips, which are mutually exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageFilter {
    All,
    /// Files live outside the library directory.
    Local,
    /// Files are stored by the library.
    Remote,
    /// At least one file is pending cloud upload.
    Uploading,
}
