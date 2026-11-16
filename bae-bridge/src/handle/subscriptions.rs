//! `AppHandle`'s calls that open a screen's live read of the library: each
//! hands back a pull object built in [`crate::screens`], over the services'
//! subscription.

use std::sync::Arc;

use super::*;

#[uniffi::export]
impl AppHandle {
    /// Read the rows of `list` in `window`, in the list's order, and keep
    /// reading them live.
    pub fn subscribe_list_window(
        &self,
        list: BridgeList,
        window: BridgeListWindow,
    ) -> Arc<crate::ListWindowSubscription> {
        let window = window.into_core();
        let live = match list {
            BridgeList::AlbumGrid { sort, group_by_artist } => self
                .services
                .subscribe_album_window(&BridgeList::album_sort(sort), group_by_artist, window),
            BridgeList::Artists { sort } => {
                self.services.subscribe_artist_window(&BridgeList::artist_sort(sort), window)
            }
            BridgeList::Composers { sort } => {
                self.services.subscribe_composer_window(&BridgeList::composer_sort(sort), window)
            }
            BridgeList::ComposerPage { artist_id } => {
                self.services.subscribe_composer_page_window(&artist_id, window)
            }
            BridgeList::AlbumTracks { album_id } => {
                self.services.subscribe_album_track_window(&album_id, window)
            }
            BridgeList::WorkChildren { work_id } => {
                self.services.subscribe_work_child_window(&work_id, window)
            }
            BridgeList::Downloads => self.services.subscribe_download_window(window),
            BridgeList::Upcoming { shuffled } => {
                self.services.subscribe_upcoming_window(shuffled, window)
            }
            BridgeList::Storage { sort, filter, search } => self.services.subscribe_storage_window(
                &BridgeList::storage_sort(sort),
                filter.into_core(),
                &search,
                window,
            ),
            BridgeList::ArtistSearch { text } => self
                .services
                .subscribe_artist_search_window(BridgeList::search_query(&text), window),
            BridgeList::SearchAlbums { query } => self
                .services
                .subscribe_search_album_window(BridgeList::search_query(&query), window),
        };
        crate::screens::list_window(live, self.runtime.clone())
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

    /// How many rows of `list` come before the row at `key`, read live.
    pub fn subscribe_list_position(
        &self,
        list: BridgeList,
        key: Vec<BridgeSortValue>,
    ) -> Arc<crate::ListPositionSubscription> {
        let key = crate::screens::order_key(key);
        let live = match list {
            BridgeList::AlbumGrid { sort, group_by_artist } => self
                .services
                .subscribe_album_position(&BridgeList::album_sort(sort), group_by_artist, key),
            BridgeList::Artists { sort } => {
                self.services.subscribe_artist_position(&BridgeList::artist_sort(sort), key)
            }
            BridgeList::Composers { sort } => {
                self.services.subscribe_composer_position(&BridgeList::composer_sort(sort), key)
            }
            BridgeList::ComposerPage { artist_id } => {
                self.services.subscribe_composer_page_position(&artist_id, key)
            }
            BridgeList::AlbumTracks { album_id } => {
                self.services.subscribe_album_track_position(&album_id, key)
            }
            BridgeList::WorkChildren { work_id } => {
                self.services.subscribe_work_child_position(&work_id, key)
            }
            BridgeList::Downloads => self.services.subscribe_download_position(key),
            BridgeList::Upcoming { shuffled } => {
                self.services.subscribe_upcoming_position(shuffled, key)
            }
            BridgeList::Storage { sort, filter, search } => {
                self.services.subscribe_storage_position(
                    &BridgeList::storage_sort(sort),
                    filter.into_core(),
                    &search,
                    key,
                )
            }
            BridgeList::ArtistSearch { text } => {
                self.services.subscribe_artist_search_position(BridgeList::search_query(&text), key)
            }
            BridgeList::SearchAlbums { query } => {
                self.services.subscribe_search_album_position(BridgeList::search_query(&query), key)
            }
        };
        crate::screens::list_position(live, self.runtime.clone())
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
