//! The Storage Manager list, read through one live query whose sort, filter,
//! search text and windows change in place.

use bae_store::LibraryBrowseWindow;

/// Every window of the list read under one sort, filter and search text —
/// the ones named here, which a UI that has since moved on drops — with the
/// filtered set's row count and total size, the row of each selected release
/// the filtered set holds, in a window or not, by release id, and the storage
/// actions every one of those allows: a selected release the list no longer
/// holds is absent, so the inspector never shows, and the storage actions
/// never act on, a release the list doesn't. Beside the list, the transfer
/// queues that hold work.
#[derive(Debug, Clone)]
pub struct StorageBrowseSnapshot {
    pub sort: crate::StorageSortCriterion,
    pub filter: crate::StorageFilter,
    pub search: String,
    /// Whether the search text narrows the list: it is a library search,
    /// not blank.
    pub searching: bool,
    pub windows: Vec<LibraryBrowseWindow<crate::album_detail::StorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected: Vec<crate::album_detail::StorageRow>,
    pub selection_actions: Vec<bae_types::release_storage::ReleaseStorageAction>,
    pub queues: StorageQueues,
}

/// The transfer queues holding files waiting or on the move, each with what
/// holds it back: this device's uploads, its pins (downloads to keep releases
/// here), and its exports and saves. A queue holding no work, or only failed
/// entries, is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageQueues {
    pub uploads: Option<UploadQueueRun>,
    pub pins: Option<ReleaseQueueRun>,
    pub outputs: Option<ReleaseQueueRun>,
}

/// The upload queue while it holds files, and what holds it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadQueueRun {
    pub wait: Option<bae_records::UploadQueueWait>,
}

/// A queue that works through one release at a time, while it holds work:
/// what holds it back, and the release it runs, if any; how fast that goes,
/// or how far, arrives apart, keyed by the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseQueueRun {
    pub wait: Option<bae_transfers::QueueWait>,
    pub active: Option<String>,
}
