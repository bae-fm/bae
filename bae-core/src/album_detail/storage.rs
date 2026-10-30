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
    /// originals; none while any of its files waits to upload.
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
        // A release with files waiting to upload offers only cancelling.
        let storage_actions = if raw.upload.is_some() {
            Vec::new()
        } else {
            available_storage_actions(
                storage_state,
                release_playability(&raw.unstored_audio, originals),
                raw.pinned,
                has_cloud_home,
            )
        };
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

/// Where a release's files are, as the Storage Manager sorts its rows by
/// storage, in that order: its listing works it out from the upload queue,
/// the download queue, and its stored, pinned and waiting pinned files, in
/// the write that changes any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePlace {
    /// Files of it wait in this device's upload queue.
    Uploading,
    /// Its files are coming to this device: this device's download queue
    /// holds it, waiting or downloading, or pins of its files wait for coven
    /// to fetch their copies.
    Pinning,
    Local,
    /// Uploaded, every file pinned on this device.
    Pinned,
    /// Uploaded, and not wholly pinned on this device.
    Cloud,
}

/// What a Storage Manager row's storage state shows: where its files are,
/// and while they rest, a transition this process runs for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageRowStatus {
    Uploading,
    Pinning,
    /// A transition under way while the files rest where they are: unpinning,
    /// making the release local, or admitting it for upload.
    Transferring(ReleaseStorageAction),
    Local,
    Pinned,
    Cloud,
}

impl StorageRowStatus {
    fn of(place: StoragePlace, transfer: Option<ReleaseStorageAction>) -> Self {
        match (place, transfer) {
            (StoragePlace::Uploading, _) => Self::Uploading,
            (StoragePlace::Pinning, _) => Self::Pinning,
            (_, Some(action)) => Self::Transferring(action),
            (StoragePlace::Local, None) => Self::Local,
            (StoragePlace::Pinned, None) => Self::Pinned,
            (StoragePlace::Cloud, None) => Self::Cloud,
        }
    }
}

/// One row on the Storage Manager view: a release with its parent album,
/// what its storage state shows, and when it was added.
#[derive(Debug, Clone)]
pub struct StorageRow {
    pub release: ReleaseSummary,
    pub album: AlbumSummary,
    pub status: StorageRowStatus,
    /// When the release was added to the library.
    pub date_added: chrono::DateTime<chrono::Utc>,
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
    /// The release and its album arrive pre-joined from SQL, each with its
    /// cover's version; each half maps to its own summary projection, so the
    /// release row carries its own art and the album its primary release's.
    pub(crate) fn from_raw(
        raw: DbStorageRow,
        has_cloud_home: bool,
        transfer_action: Option<ReleaseStorageAction>,
        originals: &OriginalsSeen,
    ) -> StorageRow {
        let cover = cover_ref(&raw.release.id, raw.release_cover_version);
        let ctx = ReleaseResolveCtx {
            has_cloud_home,
            originals,
            cover,
            transfer_action,
            // A storage row projects a `ReleaseSummary`, which has no tracks, so
            // the per-track compilation flag is never read on this path.
            is_compilation: false,
        };
        StorageRow {
            release: ReleaseSummary::from_raw(raw.release, &ctx),
            album: AlbumSummary::from_raw(raw.album),
            status: StorageRowStatus::of(raw.place, transfer_action),
            date_added: raw.date_added,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A transition this process runs shows while the files rest, and the
    /// queues' places show over it.
    #[test]
    fn a_running_transition_shows_only_while_the_files_rest() {
        let detach = Some(ReleaseStorageAction::DetachAudio);
        assert_eq!(
            StorageRowStatus::of(StoragePlace::Pinned, detach),
            StorageRowStatus::Transferring(ReleaseStorageAction::DetachAudio)
        );
        assert_eq!(
            StorageRowStatus::of(StoragePlace::Pinning, Some(ReleaseStorageAction::Pin)),
            StorageRowStatus::Pinning
        );
        assert_eq!(
            StorageRowStatus::of(StoragePlace::Uploading, Some(ReleaseStorageAction::Upload)),
            StorageRowStatus::Uploading
        );
        assert_eq!(
            StorageRowStatus::of(StoragePlace::Cloud, None),
            StorageRowStatus::Cloud
        );
    }
}
