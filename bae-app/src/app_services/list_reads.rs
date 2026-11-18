//! The library views' long lists' reads on [`AppServices`], and how every
//! list's reads are built: each list's window, count and position are live
//! reads of their own, and each list's view (`bae_views::list_view`) opens
//! its window and position from the reads its builder gives it. The queue's,
//! the download queue's and the Storage Manager's builders are with their
//! domains' other reads.

use super::*;
use bae_store::list_windows::OrderKey;
use bae_store::{DbError, ListWindow};
use bae_views::list_view::ListReads;
use std::future::Future;

impl AppServices {
    /// The album grid's reads under `sort`, flat or grouped by artist.
    pub fn album_list(
        &self,
        sort: Vec<bae_views::AlbumSortCriterion>,
        group_by_artist: bool,
    ) -> ListReads {
        let (window_sort, position_sort) = (sort.clone(), sort.clone());
        list_reads(
            self,
            move |services, window| {
                services.subscribe_album_window(&window_sort, group_by_artist, window)
            },
            move |services, key| {
                services.subscribe_album_position(&position_sort, group_by_artist, key)
            },
            move |services, position| {
                let sort = sort.clone();
                async move { services.inner.views.album_key_at(&sort, group_by_artist, position).await }
            },
        )
    }

    /// The artist list's reads under `sort`.
    pub fn artist_list(&self, sort: Vec<bae_views::ArtistSortCriterion>) -> ListReads {
        let (window_sort, position_sort) = (sort.clone(), sort.clone());
        list_reads(
            self,
            move |services, window| services.subscribe_artist_window(&window_sort, window),
            move |services, key| services.subscribe_artist_position(&position_sort, key),
            move |services, position| {
                let sort = sort.clone();
                async move { services.inner.views.artist_key_at(&sort, position).await }
            },
        )
    }

    /// The composer list's reads under `sort`.
    pub fn composer_list(&self, sort: Vec<bae_views::ComposerSortCriterion>) -> ListReads {
        let (window_sort, position_sort) = (sort.clone(), sort.clone());
        list_reads(
            self,
            move |services, window| services.subscribe_composer_window(&window_sort, window),
            move |services, key| services.subscribe_composer_position(&position_sort, key),
            move |services, position| {
                let sort = sort.clone();
                async move { services.inner.views.composer_key_at(&sort, position).await }
            },
        )
    }

    /// The reads of `artist_id`'s composer page.
    pub fn composer_page_list(&self, artist_id: String) -> ListReads {
        let (window_id, position_id) = (artist_id.clone(), artist_id.clone());
        list_reads(
            self,
            move |services, window| services.subscribe_composer_page_window(&window_id, window),
            move |services, key| services.subscribe_composer_page_position(&position_id, key),
            move |services, position| {
                let artist_id = artist_id.clone();
                async move { services.inner.views.composer_detail_key_at(&artist_id, position).await }
            },
        )
    }

    /// The reads of the artists the picker's `query` finds; a blank text
    /// (`None`) finds none.
    pub fn artist_search_list(&self, query: Option<bae_views::LibrarySearchQuery>) -> ListReads {
        let (window_query, position_query) = (query.clone(), query.clone());
        list_reads(
            self,
            move |services, window| {
                services.subscribe_artist_search_window(window_query.clone(), window)
            },
            move |services, key| {
                services.subscribe_artist_search_position(position_query.clone(), key)
            },
            move |services, position| {
                let query = query.clone();
                async move { services.inner.views.artist_search_key_at(query, position).await }
            },
        )
    }

    /// The reads of the albums library search's `query` lists; no query
    /// (`None`) lists none.
    pub fn search_album_list(&self, query: Option<bae_views::LibrarySearchQuery>) -> ListReads {
        let (window_query, position_query) = (query.clone(), query.clone());
        list_reads(
            self,
            move |services, window| {
                services.subscribe_search_album_window(window_query.clone(), window)
            },
            move |services, key| {
                services.subscribe_search_album_position(position_query.clone(), key)
            },
            move |services, position| {
                let query = query.clone();
                async move { services.inner.views.search_album_key_at(query, position).await }
            },
        )
    }

    /// The reads of the album `album_id`'s tracks.
    pub fn album_track_list(&self, album_id: String) -> ListReads {
        let (window_id, position_id) = (album_id.clone(), album_id.clone());
        list_reads(
            self,
            move |services, window| services.subscribe_album_track_window(&window_id, window),
            move |services, key| services.subscribe_album_track_position(&position_id, key),
            move |services, position| {
                let album_id = album_id.clone();
                async move { services.inner.views.album_track_key_at(&album_id, position).await }
            },
        )
    }

    /// The reads of the work `work_id`'s children.
    pub fn work_child_list(&self, work_id: String) -> ListReads {
        let (window_id, position_id) = (work_id.clone(), work_id.clone());
        list_reads(
            self,
            move |services, window| services.subscribe_work_child_window(&window_id, window),
            move |services, key| services.subscribe_work_child_position(&position_id, key),
            move |services, position| {
                let work_id = work_id.clone();
                async move { services.inner.views.work_child_key_at(&work_id, position).await }
            },
        )
    }
}

/// The reads one list's view opens: `window` opens its live window, from
/// `services`; `position` opens the live position of a key; and `key_at`
/// reads the key at a position once.
pub(super) fn list_reads<KeyAt>(
    services: &AppServices,
    window: impl Fn(&AppServices, ListWindow) -> bae_views::LiveListWindow + Send + Sync + 'static,
    position: impl Fn(&AppServices, OrderKey) -> bae_views::LiveListPosition + Send + Sync + 'static,
    key_at: impl Fn(AppServices, u64) -> KeyAt + Send + Sync + 'static,
) -> ListReads
where
    KeyAt: Future<Output = Result<Option<OrderKey>, DbError>> + Send + 'static,
{
    let (for_window, for_position, for_key) =
        (services.clone(), services.clone(), services.clone());
    ListReads {
        window: Box::new(move |request| window(&for_window, request)),
        position: Box::new(move |key| position(&for_position, key)),
        key_at: Box::new(move |at| {
            let read = key_at(for_key.clone(), at);
            Box::pin(
                async move { read.await.map_err(bae_store::live_query::LiveQueryError::Query) },
            )
        }),
    }
}

impl AppServices {
    /// One window of the album grid under `sort`, flat or grouped by artist.
    pub fn subscribe_album_window(
        &self,
        sort: &[bae_views::AlbumSortCriterion],
        group_by_artist: bool,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_album_window(sort, group_by_artist, window),
            bae_views::album::resolve_album_window,
        )
    }

    /// How many rows the album grid has, and how many albums: the first
    /// value at once, then one each time a write moves either.
    pub fn subscribe_album_count(
        &self,
        group_by_artist: bool,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        self.inner.manager.values_until_closed(
            self.inner.views.subscribe_album_count(group_by_artist).process(|counts| {
                Ok(bae_views::ListCount {
                    rows: counts.rows,
                    items: counts.albums,
                    extra: bae_views::ListCountExtra::AlbumGrid {
                        can_shuffle: counts.can_shuffle,
                        grouped: counts.grouped,
                    },
                })
            }),
        )
    }

    /// How many rows of the album grid come before the row at `key`.
    pub fn subscribe_album_position(
        &self,
        sort: &[bae_views::AlbumSortCriterion],
        group_by_artist: bool,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_album_position(sort, group_by_artist, key),
        )
    }

    /// One window of the artist list under `sort`.
    pub fn subscribe_artist_window(
        &self,
        sort: &[bae_views::ArtistSortCriterion],
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_artist_window(sort, window),
            bae_views::artist::resolve_artist_window,
        )
    }

    /// How many artists the list has.
    pub fn subscribe_artist_count(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_artist_count())
    }

    /// How many artists come before the row at `key`.
    pub fn subscribe_artist_position(
        &self,
        sort: &[bae_views::ArtistSortCriterion],
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(&self.inner.manager, self.inner.views.subscribe_artist_position(sort, key))
    }

    /// One window of the composer list under `sort`.
    pub fn subscribe_composer_window(
        &self,
        sort: &[bae_views::ComposerSortCriterion],
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_composer_window(sort, window),
            bae_views::composer::resolve_composer_window,
        )
    }

    /// How many composers the list has.
    pub fn subscribe_composer_count(
        &self,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_composer_count())
    }

    /// How many composers come before the row at `key`.
    pub fn subscribe_composer_position(
        &self,
        sort: &[bae_views::ComposerSortCriterion],
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(&self.inner.manager, self.inner.views.subscribe_composer_position(sort, key))
    }

    /// One window of `artist_id`'s composer page.
    pub fn subscribe_composer_page_window(
        &self,
        artist_id: &str,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_composer_detail_window(artist_id, window),
            bae_views::composer_detail::resolve_composer_detail_window,
        )
    }

    /// How many rows `artist_id`'s composer page holds.
    pub fn subscribe_composer_page_count(
        &self,
        artist_id: &str,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_composer_detail_count(artist_id))
    }

    /// How many rows of `artist_id`'s composer page come before the row at
    /// `key`.
    pub fn subscribe_composer_page_position(
        &self,
        artist_id: &str,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_composer_detail_position(artist_id, key),
        )
    }

    /// One window of the artists the picker's `query` finds; a blank text
    /// (`None`) finds none.
    pub fn subscribe_artist_search_window(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_artist_search_window(query, window),
            bae_views::artist_search::resolve_artist_search_window,
        )
    }

    /// How many artists the picker's `query` finds.
    pub fn subscribe_artist_search_count(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_artist_search_count(query))
    }

    /// How many of the artists the picker's `query` finds come before the
    /// row at `key`.
    pub fn subscribe_artist_search_position(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_artist_search_position(query, key),
        )
    }

    /// One window of the albums library search's `query` lists; no query
    /// (`None`) lists none.
    pub fn subscribe_search_album_window(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_search_album_window(query, window),
            bae_views::album::resolve_search_album_window,
        )
    }

    /// How many albums library search's `query` lists; no query (`None`)
    /// lists none.
    pub fn subscribe_search_album_count(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_search_album_count(query))
    }

    /// How many of the albums library search's `query` lists come before the
    /// row at `key`.
    pub fn subscribe_search_album_position(
        &self,
        query: Option<bae_views::LibrarySearchQuery>,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_search_album_position(query, key),
        )
    }

    /// One window of the album `album_id`'s tracks.
    pub fn subscribe_album_track_window(
        &self,
        album_id: &str,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_album_track_window(album_id, window),
            bae_views::album::resolve_album_track_window,
        )
    }

    /// How many tracks the album `album_id` lists.
    pub fn subscribe_album_track_count(
        &self,
        album_id: &str,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_album_track_count(album_id))
    }

    /// How many of the album `album_id`'s tracks come before the row at
    /// `key`.
    pub fn subscribe_album_track_position(
        &self,
        album_id: &str,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_album_track_position(album_id, key),
        )
    }

    /// One window of the work `work_id`'s children.
    pub fn subscribe_work_child_window(
        &self,
        work_id: &str,
        window: bae_store::ListWindow,
    ) -> bae_views::LiveListWindow {
        list_window(
            &self.inner.manager,
            self.inner.views.subscribe_work_child_window(work_id, window),
            bae_views::composer::resolve_work_child_window,
        )
    }

    /// How many children the work `work_id` has.
    pub fn subscribe_work_child_count(
        &self,
        work_id: &str,
    ) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
        list_rows(&self.inner.manager, self.inner.views.subscribe_work_child_count(work_id))
    }

    /// How many of the work `work_id`'s children come before the row at
    /// `key`.
    pub fn subscribe_work_child_position(
        &self,
        work_id: &str,
        key: bae_store::list_windows::OrderKey,
    ) -> bae_views::LiveListPosition {
        list_position(
            &self.inner.manager,
            self.inner.views.subscribe_work_child_position(work_id, key),
        )
    }
}

/// One window of a list, as its first value at once, then one for each
/// write at or before its last row and one each time the list's view moves
/// it.
pub(super) fn list_window<Row: Clone + PartialEq + Send + 'static>(
    manager: &bae_library::LibraryManager,
    query: coven::ReconfigurableLiveQuery<
        bae_store::ListWindow,
        bae_store::WindowRead<Row>,
        bae_store::DbError,
    >,
    resolve: fn(bae_store::WindowRead<Row>) -> bae_views::ListWindowValue,
) -> bae_views::LiveListWindow {
    bae_views::LiveListWindow::new(manager.until_closed(query), move |_read, window| {
        Some(resolve(window))
    })
}

/// How many rows a list has, as its first value at once, then one each
/// time a write moves it.
pub(super) fn list_rows(
    manager: &bae_library::LibraryManager,
    query: coven::LiveQuery<u64, bae_store::DbError>,
) -> bae_store::live_query::LiveValues<bae_views::ListCount> {
    manager.values_until_closed(query.process(|rows| {
        Ok(bae_views::ListCount { rows, items: rows, extra: bae_views::ListCountExtra::None })
    }))
}

/// How many rows of a list come before the row at a key, as one live query
/// whose key the caller changes without a new subscription.
pub(super) fn list_position(
    manager: &bae_library::LibraryManager,
    query: coven::ReconfigurableLiveQuery<
        bae_store::list_windows::OrderKey,
        u64,
        bae_store::DbError,
    >,
) -> bae_views::LiveListPosition {
    bae_views::LiveListPosition::new(manager.until_closed(query), |read, rows_before| {
        Some(bae_views::ListPosition { key: read.request.clone(), rows_before })
    })
}
