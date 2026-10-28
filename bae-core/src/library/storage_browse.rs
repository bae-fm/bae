//! The Storage Manager list, read through one live query whose sort, filter,
//! and windows change in place.

use super::{LibraryBrowseWindow, LibraryPageWindows};

/// What the Storage Manager shows: the list under one sort and filter, the
/// windows of it on screen, and the releases selected in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseView {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: LibraryPageWindows,
    pub selected: std::collections::BTreeSet<String>,
}

/// Every window of the list read under one sort and filter — the ones named
/// here, which a UI that has since moved on drops — with the filtered set's
/// row count and total size, the row of each selected release the filtered
/// set holds, in a window or not, by release id, and the storage actions every
/// one of those allows: a selected release the list no longer holds is
/// absent, so the inspector never shows, and the storage actions never act
/// on, a release the list doesn't. Beside the list, the transfer queues that
/// hold work.
#[derive(Debug, Clone)]
pub struct StorageBrowseSnapshot {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: Vec<LibraryBrowseWindow<crate::album_detail::StorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected: Vec<crate::album_detail::StorageRow>,
    pub selection_actions: Vec<crate::album_detail::ReleaseStorageAction>,
    pub queues: StorageQueues,
}

/// The transfer queues holding files waiting or on the move, each with
/// whether the person paused it: this device's uploads, its pins (downloads
/// to keep releases here), and its exports and saves. A queue holding no
/// work, or only failed entries, is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageQueues {
    pub uploads: Option<UploadQueueRun>,
    pub pins: Option<ReleaseQueueRun>,
    pub outputs: Option<ReleaseQueueRun>,
}

/// The upload queue while it holds files: whether it is paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UploadQueueRun {
    pub paused: bool,
}

/// A queue that works through one release at a time, while it holds work:
/// whether it is paused, and the release it runs, if any; how fast that goes,
/// or how far, arrives apart, keyed by the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseQueueRun {
    pub paused: bool,
    pub active: Option<String>,
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`](super::LiveRead::set); the upload queue, pin
/// markers, and sync state the rows are resolved against move the same read.
pub type StorageBrowseSubscription = super::LiveRead<StorageBrowseView, StorageBrowseSnapshot>;
