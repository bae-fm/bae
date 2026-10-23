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
/// row count and total size, and which of the selected releases the filtered
/// set holds, in a window or not: a selected release it no longer holds is
/// absent, so the inspector never shows a release the list doesn't.
#[derive(Debug, Clone)]
pub struct StorageBrowseSnapshot {
    pub sort: crate::db::StorageSortCriterion,
    pub filter: crate::db::StorageFilter,
    pub windows: Vec<LibraryBrowseWindow<crate::album_detail::StorageRow>>,
    pub total_count: u64,
    pub total_size: u64,
    pub selected_in_list: std::collections::BTreeSet<String>,
}

/// A live read of the Storage Manager list. The view changes in place
/// through [`LiveRead::set`](super::LiveRead::set); the upload queue, pin
/// markers, and sync state the rows are resolved against move the same read.
pub type StorageBrowseSubscription = super::LiveRead<StorageBrowseView, StorageBrowseSnapshot>;
