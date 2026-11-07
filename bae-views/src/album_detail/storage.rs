//! Resolved Storage Manager types (`StorageRow`, `StoragePage`) plus the
//! sort/filter inputs they're paged by, and the pure projections that
//! produce them.

use super::*;
use crate::DbStorageRow;
use bae_transfers::{DownloadEntry, QueueWait};

/// Where a release's files are, as the Storage Manager sorts its rows by
/// storage, in that order: its listing works it out from the upload queue,
/// the download queue, and its stored and pinned files, in the write that
/// changes any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoragePlace {
    /// This device's download queue holds it failed, or coven lists a file of
    /// it in a failed move.
    Failed,
    /// Files of it wait in this device's upload queue.
    Uploading,
    /// Its files are coming to this device: this device's download queue
    /// holds it, waiting or downloading.
    Pinning,
    OnOneDevice,
    /// Uploaded, every file pinned on this device.
    Pinned,
    /// Uploaded, and not wholly pinned on this device.
    InStorage,
}

/// Where a release stands in the download queue while its files are coming to
/// this device, and what holds the queue back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadEntryStep {
    /// Waiting, with `ahead` entries before it that have not failed.
    Queued { ahead: u32, wait: Option<QueueWait> },
    /// Downloading.
    Running { wait: Option<QueueWait> },
}

/// What a Storage Manager row's storage state shows: where its files are,
/// and while they rest, a transition this process runs for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageRowStatus {
    /// The download queue holds it failed, with the error.
    DownloadFailed {
        error: String,
    },
    /// Its move to the cloud failed because a file's original changed since
    /// import; the move stays until cancelled.
    MoveFailed {
        file_name: String,
    },
    Uploading,
    Pinning(DownloadEntryStep),
    /// A transition under way while the files rest where they are: unpinning,
    /// making the release local, or admitting it for upload.
    Transferring(ReleaseStorageAction),
    OnOneDevice,
    Pinned,
    InStorage,
}

impl StorageRowStatus {
    /// The status of a release at `place`, in the transition `transfer`
    /// (when it rests), with the download queue entry `download` and, when
    /// its move failed, the name of the file whose original changed. A failed
    /// download reads first when both failed. A place that names a failure
    /// without the entry or file that puts it there is a row the queue and
    /// the listing disagree on: the reason it fails.
    pub(crate) fn of(
        place: StoragePlace,
        transfer: Option<ReleaseStorageAction>,
        download: Option<DownloadEntry>,
        failed_move_file: Option<String>,
    ) -> Result<Self, String> {
        Ok(match (place, transfer, download, failed_move_file) {
            (StoragePlace::Failed, _, Some(DownloadEntry::Failed { error }), _) => {
                Self::DownloadFailed { error }
            }
            (StoragePlace::Failed, _, _, Some(file_name)) => Self::MoveFailed { file_name },
            (StoragePlace::Failed, ..) => {
                return Err("the listing says a download or move failed but neither is held".into())
            }
            (StoragePlace::Uploading, ..) => Self::Uploading,
            (StoragePlace::Pinning, _, Some(DownloadEntry::Queued { ahead, wait }), _) => {
                Self::Pinning(DownloadEntryStep::Queued { ahead, wait })
            }
            (StoragePlace::Pinning, _, Some(DownloadEntry::Running { wait }), _) => {
                Self::Pinning(DownloadEntryStep::Running { wait })
            }
            (StoragePlace::Pinning, ..) => {
                return Err("the listing says a download waits but the queue holds none".into())
            }
            (_, Some(action), ..) => Self::Transferring(action),
            (StoragePlace::OnOneDevice, None, ..) => Self::OnOneDevice,
            (StoragePlace::Pinned, None, ..) => Self::Pinned,
            (StoragePlace::InStorage, None, ..) => Self::InStorage,
        })
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
        .filter(|action| rest.iter().all(|row| row.release.storage_actions.contains(action)))
        .collect()
}

impl StorageRow {
    /// The release and its album arrive pre-joined from SQL, each with its
    /// cover's version; each half maps to its own summary projection, so the
    /// release row carries its own art and the album its primary release's.
    pub(crate) fn from_raw(raw: DbStorageRow, syncs: bool) -> StorageRow {
        let cover = cover_ref(&raw.release.id, raw.release_cover_version);
        let ctx = ReleaseResolveCtx {
            syncs,
            cover,
            // A storage row projects a `ReleaseSummary`, which has no tracks, so
            // the per-track compilation flag is never read on this path.
            is_compilation: false,
        };
        StorageRow {
            release: ReleaseSummary::from_raw(raw.release, &ctx),
            album: AlbumSummary::from_raw(raw.album),
            status: raw.status,
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
#[path = "storage_tests.rs"]
mod tests;
