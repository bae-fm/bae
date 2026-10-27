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
    pub media: Vec<bae_types::pressing::MediaCount>,
    /// The release's storage state — Local (local) or Remote (cloud) —
    /// derived from its files' attachment state. Orthogonal to `pinned`.
    pub storage_state: ReleaseStorageState,
    /// Whether coven keeps this release's files pinned (kept offline) on this
    /// device — the orthogonal coven-cache property. Meaningful only when
    /// `storage_state` is `Remote`. Kept SEPARATE from `storage_state`.
    pub pinned: bool,
    /// The storage transitions this release allows now, gated on the cloud
    /// home and, for an upload, on this device holding the release's
    /// originals. The in-flight-uploads gate lives in the UI: it suppresses these
    /// actions when the outbox snapshot has a group for this release.
    pub storage_actions: Vec<ReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
}

impl ReleaseStorageSummary {
    /// `storage_state` derives from `remote` alone, and `pinned` is the
    /// orthogonal pin state the row reads beside it; `originals` is what the
    /// disk has said of the originals the library records this device holding.
    pub(crate) fn from_raw(
        raw: DbReleaseStorageSummary,
        has_cloud_home: bool,
        originals: &OriginalsSeen,
    ) -> ReleaseStorageSummary {
        let storage_state = storage_state(raw.remote);
        let storage_actions = available_storage_actions(
            storage_state,
            release_playability(&raw.unstored_audio, originals),
            raw.pinned,
            has_cloud_home,
        );
        ReleaseStorageSummary {
            storage_state,
            pinned: raw.pinned,
            storage_actions,
            release_id: raw.release_id,
            album_id: raw.album_id,
            album_title: raw.album_title,
            artist_names: raw.artist_names,
            media: raw.media,
            file_count: raw.file_count,
            total_size: raw.total_size,
        }
    }
}

/// One row on the Storage Manager view: a release with its parent album.
#[derive(Debug, Clone)]
pub struct StorageRow {
    pub release: ReleaseSummary,
    pub album: AlbumSummary,
}

/// The storage actions every one of `rows` allows, in the order
/// [`available_storage_actions`] lists them; none for no rows. What the
/// Storage Manager offers a selection: an action some selected release
/// refuses is not offered for the rest.
pub fn common_storage_actions(rows: &[StorageRow]) -> Vec<ReleaseStorageAction> {
    let Some((first, rest)) = rows.split_first() else {
        return Vec::new();
    };
    first
        .release
        .storage_actions
        .iter()
        .copied()
        .filter(|action| {
            rest.iter()
                .all(|row| row.release.storage_actions.contains(action))
        })
        .collect()
}

impl StorageRow {
    /// The release and its album arrive pre-joined from SQL; each half maps
    /// to its own summary projection.
    /// `resolve_cover` serves both, so the release row carries its own art and
    /// the album carries its primary release's.
    pub(crate) fn from_raw(
        raw: DbStorageRow,
        has_cloud_home: bool,
        transfer_action: Option<ReleaseStorageAction>,
        originals: &OriginalsSeen,
        resolve_cover: impl Fn(&str) -> Option<ImageRef>,
    ) -> StorageRow {
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            originals,
            cover: resolve_cover(&raw.release.id),
            transfer_action,
            // A storage row projects a `ReleaseSummary`, which has no tracks, so
            // the per-track compilation flag is never read on this path.
            is_compilation: false,
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
