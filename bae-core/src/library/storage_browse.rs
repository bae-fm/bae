//! The Storage Manager list, read through one live query whose sort, filter,
//! and windows change in place.

use super::{LibraryBrowseWindow, LibraryPageWindows};

/// What the Storage Manager shows: the list under one sort and filter, and
/// the windows of it on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBrowseView {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: LibraryPageWindows,
}

/// Every window of the list read under one sort and filter — the ones named
/// here, which a UI that has since moved on drops — with the filtered set's
/// row count and total size.
#[derive(Debug, Clone)]
pub struct StorageBrowseSnapshot {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: Vec<LibraryBrowseWindow<crate::album_detail::StorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`](super::LiveRead::set); the upload queue, pin
/// markers, and sync state the rows are resolved against move the same read.
pub type StorageBrowseSubscription = super::LiveRead<StorageBrowseView, StorageBrowseSnapshot>;
