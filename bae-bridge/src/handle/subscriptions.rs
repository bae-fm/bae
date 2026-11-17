//! `AppHandle`'s calls that open a screen's live read of the library: each
//! hands back a pull object built in [`crate::screens`], over the services'
//! subscription.

use std::sync::Arc;

use super::*;

#[uniffi::export]
impl AppHandle {
    /// Read `list` through its view, which keeps one live window of the list
    /// around the rows from position `first` to position `last` on screen,
    /// and moves it as the screen reports other rows.
    pub fn subscribe_list_view(
        &self,
        list: BridgeList,
        first: u64,
        last: u64,
    ) -> Arc<crate::ListViewSubscription> {
        let services = &self.services;
        let reads = match list {
            BridgeList::AlbumGrid { sort, group_by_artist } => {
                services.album_list(BridgeList::album_sort(sort), group_by_artist)
            }
            BridgeList::Artists { sort } => services.artist_list(BridgeList::artist_sort(sort)),
            BridgeList::Composers { sort } => {
                services.composer_list(BridgeList::composer_sort(sort))
            }
            BridgeList::ComposerPage { artist_id } => services.composer_page_list(artist_id),
            BridgeList::AlbumTracks { album_id } => services.album_track_list(album_id),
            BridgeList::WorkChildren { work_id } => services.work_child_list(work_id),
            BridgeList::Downloads => services.download_list(),
            BridgeList::Upcoming { shuffled } => services.upcoming_list(shuffled),
            BridgeList::Storage { sort, filter, search } => {
                services.storage_list(sort.into_core(), filter.into_core(), search)
            }
            BridgeList::ArtistSearch { text } => {
                services.artist_search_list(BridgeList::search_query(&text))
            }
            BridgeList::SearchAlbums { query } => {
                services.search_album_list(BridgeList::search_query(&query))
            }
        };
        crate::screens::list_view(
            bae_views::list_view::ListView::new(
                reads,
                bae_views::list_view::Visible::new(first, last),
            ),
            self.runtime.clone(),
        )
    }

    /// Read live how many rows `list` has, with the values its count carries
    /// (`BridgeListCountExtra`).
    pub fn subscribe_list_count(&self, list: BridgeList) -> Arc<crate::ListCountSubscription> {
        let live = match list {
            BridgeList::AlbumGrid { group_by_artist, .. } => {
                self.services.subscribe_album_count(group_by_artist)
            }
            BridgeList::Artists { .. } => self.services.subscribe_artist_count(),
            BridgeList::Composers { .. } => self.services.subscribe_composer_count(),
            BridgeList::ComposerPage { artist_id } => {
                self.services.subscribe_composer_page_count(&artist_id)
            }
            BridgeList::AlbumTracks { album_id } => {
                self.services.subscribe_album_track_count(&album_id)
            }
            BridgeList::WorkChildren { work_id } => {
                self.services.subscribe_work_child_count(&work_id)
            }
            BridgeList::Downloads => self.services.subscribe_download_count(),
            BridgeList::Upcoming { shuffled } => self.services.subscribe_upcoming_count(shuffled),
            BridgeList::Storage { filter, search, .. } => {
                self.services.subscribe_storage_count(filter.into_core(), &search)
            }
            BridgeList::ArtistSearch { text } => {
                self.services.subscribe_artist_search_count(BridgeList::search_query(&text))
            }
            BridgeList::SearchAlbums { query } => {
                self.services.subscribe_search_album_count(BridgeList::search_query(&query))
            }
        };
        crate::screens::list_count(live, self.runtime.clone())
    }

    /// The artist sections of the album grid grouped by artist under
    /// `sort_criteria`.
    pub fn subscribe_album_sections(
        &self,
        sort_criteria: Vec<BridgeSortCriterion>,
    ) -> Arc<crate::AlbumSectionsSubscription> {
        let sort: Vec<_> = sort_criteria.into_iter().map(BridgeSortCriterion::into_core).collect();
        crate::screens::album_sections(
            self.services.subscribe_album_sections(&sort),
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
    pub fn subscribe_playlist_detail(&self) -> Arc<crate::PlaylistDetailSubscription> {
        crate::screens::playlist_detail(
            self.services.subscribe_playlist_detail(),
            self.runtime.clone(),
        )
    }

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

    /// Read live the rows of the selected releases that the Storage Manager
    /// list holds under `filter` and `search`, and the storage actions all of
    /// them allow. The read has no selection until `set_selected` names one.
    pub fn subscribe_storage_selection(
        &self,
        filter: BridgeStorageFilter,
        search: String,
    ) -> Arc<crate::StorageSelectionSubscription> {
        crate::screens::storage_selection(
            self.services.subscribe_storage_selection(filter.into_core(), &search),
            self.runtime.clone(),
        )
    }

    /// The transfer queues holding work, as the Storage Manager shows them.
    pub fn subscribe_storage_queues(&self) -> Arc<crate::StorageQueuesSubscription> {
        crate::screens::storage_queues(
            self.services.subscribe_storage_queues(),
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

    /// The Storage Manager inspector's read of one release file, for its
    /// file details, showing nothing until `set_id`.
    pub fn subscribe_storage_file(&self) -> Arc<crate::StorageFileSubscription> {
        crate::screens::storage_file(self.services.subscribe_storage_file(), self.runtime.clone())
    }

    /// The covers a library release's picker shows, showing nothing until
    /// `set_request` names a release.
    pub fn subscribe_cover_gallery(&self) -> Arc<crate::CoverGallerySubscription> {
        crate::screens::cover_gallery(self.services.subscribe_cover_gallery(), self.runtime.clone())
    }

    /// The images the lightbox of a candidate's folder shows, showing
    /// nothing until `set_request` names a candidate.
    pub fn subscribe_folder_images(&self) -> Arc<crate::FolderImagesSubscription> {
        crate::screens::folder_images(self.services.subscribe_folder_images(), self.runtime.clone())
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
