//! `AppHandle`'s calls that open a screen's live read of the library: each
//! hands back a pull object built in [`crate::screens`], over the services'
//! subscription.

use std::sync::Arc;

use super::*;

#[uniffi::export]
impl AppHandle {
    /// The album list under `sort_criteria`, grouped by artist or not,
    /// reading no windows until `set_windows`.
    pub fn subscribe_album_browse(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
        group_by_artist: bool,
    ) -> Arc<crate::AlbumBrowseSubscription> {
        let sort: Vec<_> = sort_criteria.into_iter().map(BridgeSortCriterion::into_core).collect();
        crate::screens::album_browse(
            self.services.subscribe_album_browse(&sort, group_by_artist),
            self.runtime.clone(),
        )
    }

    /// The artist list under `sort_criteria`, reading no windows until
    /// `set_windows`.
    pub fn subscribe_artist_browse(
        &self,
        sort_criteria: Vec<BridgeArtistSortCriterion>,
    ) -> Arc<crate::ArtistBrowseSubscription> {
        let sort: Vec<_> =
            sort_criteria.into_iter().map(BridgeArtistSortCriterion::into_core).collect();
        crate::screens::artist_browse(
            self.services.subscribe_artist_browse(&sort),
            self.runtime.clone(),
        )
    }

    /// The composer list under `sort_criteria`, reading no windows until
    /// `set_windows`.
    pub fn subscribe_composer_browse(
        &self,
        sort_criteria: Vec<BridgeComposerSortCriterion>,
    ) -> Arc<crate::ComposerBrowseSubscription> {
        let sort: Vec<_> =
            sort_criteria.into_iter().map(BridgeComposerSortCriterion::into_core).collect();
        crate::screens::composer_browse(
            self.services.subscribe_composer_browse(&sort),
            self.runtime.clone(),
        )
    }

    /// The rows of `artist_id`'s composer page, reading no windows until
    /// `set_windows`.
    pub fn subscribe_composer_detail_rows(
        &self,
        artist_id: String,
    ) -> Arc<crate::ComposerDetailRowsSubscription> {
        crate::screens::composer_detail_rows(
            self.services.subscribe_composer_detail_rows(&artist_id),
            self.runtime.clone(),
        )
    }

    /// A read of the download queue that reads no windows until
    /// `set_windows`.
    pub fn subscribe_download_browse(&self) -> Arc<crate::DownloadBrowseSubscription> {
        crate::screens::download_browse(
            self.services.subscribe_download_browse(),
            self.runtime.clone(),
        )
    }

    /// A read of the album grid's selection that reads no albums until
    /// `set_albums`.
    pub fn subscribe_album_selection(&self) -> Arc<crate::AlbumSelectionSubscription> {
        crate::screens::album_selection(
            self.services.subscribe_album_selection(),
            self.runtime.clone(),
        )
    }

    /// A live library search with no query until `set_query`.
    pub fn subscribe_library_search(&self) -> Arc<crate::LibrarySearchSubscription> {
        crate::screens::library_search(
            self.services.subscribe_library_search(),
            self.runtime.clone(),
        )
    }

    /// A live artist search with no text and no windows until `set_view`
    /// names them.
    pub fn subscribe_artist_search(&self) -> Arc<crate::ArtistSearchSubscription> {
        crate::screens::artist_search(self.services.subscribe_artist_search(), self.runtime.clone())
    }

    /// The album detail view's read, showing nothing until `set_id`.
    pub fn subscribe_album_detail(&self) -> Arc<crate::AlbumDetailSubscription> {
        crate::screens::album_detail(self.services.subscribe_album_detail(), self.runtime.clone())
    }

    /// The release detail view's read, showing nothing until `set_id`.
    pub fn subscribe_release_detail(&self) -> Arc<crate::ReleaseDetailSubscription> {
        crate::screens::release_detail(
            self.services.subscribe_release_detail(),
            self.runtime.clone(),
        )
    }

    /// The artist detail view's read, showing nothing until `set_id`.
    pub fn subscribe_artist_detail(&self) -> Arc<crate::ArtistDetailSubscription> {
        crate::screens::artist_detail(self.services.subscribe_artist_detail(), self.runtime.clone())
    }

    /// The composer detail view's read, showing nothing until `set_id`.
    pub fn subscribe_composer_detail(&self) -> Arc<crate::ComposerDetailSubscription> {
        crate::screens::composer_detail(
            self.services.subscribe_composer_detail(),
            self.runtime.clone(),
        )
    }

    /// The work detail view's read, showing nothing until `set_id`.
    pub fn subscribe_work_detail(&self) -> Arc<crate::WorkDetailSubscription> {
        crate::screens::work_detail(self.services.subscribe_work_detail(), self.runtime.clone())
    }

    /// The Storage Manager list under `sort` and `filter`, searching for
    /// nothing, reading no windows and holding no selection until `set_view`
    /// names them.
    pub fn subscribe_storage_browse(
        &self,
        sort: BridgeStorageSort,
        filter: BridgeStorageFilter,
    ) -> Arc<crate::StorageBrowseSubscription> {
        crate::screens::storage_browse(
            self.services.subscribe_storage_browse(bae_views::StorageBrowseView {
                sort: sort.into_core(),
                filter: filter.into_core(),
                search: String::new(),
                windows: Default::default(),
                selected: Default::default(),
            }),
            self.runtime.clone(),
        )
    }

    /// The queue's upcoming tail, reading no windows until `set_windows`.
    pub fn subscribe_queue_upcoming(&self) -> Arc<crate::QueueUpcomingSubscription> {
        crate::screens::queue_upcoming(
            self.services.subscribe_queue_upcoming(),
            self.runtime.clone(),
        )
    }
}

#[cfg(desktop)]
#[uniffi::export]
impl AppHandle {
    /// An import candidate's detail, showing nothing until `set_id`.
    pub fn subscribe_import_candidate(&self) -> Arc<crate::ImportCandidateSubscription> {
        crate::screens::import_candidate(
            self.services.subscribe_import_candidate(),
            self.runtime.clone(),
        )
    }

    /// An invalid import folder's detail, showing nothing until `set_id`.
    pub fn subscribe_invalid_candidate(&self) -> Arc<crate::InvalidCandidateSubscription> {
        crate::screens::invalid_candidate(
            self.services.subscribe_invalid_candidate(),
            self.runtime.clone(),
        )
    }

    /// Whether the releases an import pane offers are already in the
    /// library, for as long as the pane is open; it starts with no checks.
    pub fn subscribe_library_statuses(&self) -> Arc<crate::LibraryStatusSubscription> {
        crate::screens::library_statuses(
            self.services.subscribe_library_statuses(),
            self.runtime.clone(),
        )
    }
}

// Gated on the target, as the export queue the inspector's transfers include
// is, and as the file table's read is.
#[cfg(desktop)]
#[uniffi::export]
impl AppHandle {
    /// The Storage Manager inspector's read of one release, showing nothing
    /// until `set_id`.
    pub fn subscribe_storage_inspector(&self) -> Arc<crate::StorageInspectorSubscription> {
        crate::screens::storage_inspector(
            self.services.subscribe_storage_inspector(),
            self.runtime.clone(),
        )
    }

    /// A release's files in the Storage sheet's order, showing nothing until
    /// `set_request` names a release.
    pub fn subscribe_release_files(&self) -> Arc<crate::ReleaseFilesSubscription> {
        crate::screens::release_files(self.services.subscribe_release_files(), self.runtime.clone())
    }
}

#[cfg(test)]
#[path = "artist_search_tests.rs"]
mod artist_search_tests;

#[cfg(test)]
#[path = "library_search_tests.rs"]
mod library_search_tests;
