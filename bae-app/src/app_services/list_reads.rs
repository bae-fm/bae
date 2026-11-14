//! The long lists' reads on [`AppServices`]: each list's window, count and
//! position are live reads of their own, built the same way for every list.

use super::*;

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
                    extra: bae_views::ListCountExtra::AlbumGrid { albums: counts.albums },
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
}

/// One window of a list, as its first value at once, then one for each
/// write inside its range and one for each window it is moved to.
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
    manager.values_until_closed(
        query.process(|rows| {
            Ok(bae_views::ListCount { rows, extra: bae_views::ListCountExtra::None })
        }),
    )
}

/// How many rows of a list come before the row at a key, as one live
/// query whose key moves in place.
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
