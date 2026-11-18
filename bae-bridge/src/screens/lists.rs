//! This module gives screens a live read of each list's view, the window
//! core keeps around the rows on screen, and of each list's row count, with
//! the same objects for every list.

use super::PulledRead;
use crate::types::{
    BridgeError, BridgeList, BridgeListCount, BridgeListCountExtra, BridgeListRows, BridgeListView,
};
use bae_views::list_view::{ListViewHandle, ListViewValue, Visible};

impl PulledRead for ListViewHandle {
    type Value = ListViewValue;

    async fn next(&self) -> Result<ListViewValue, BridgeError> {
        ListViewHandle::next(self).await.map_err(BridgeError::from)
    }

    async fn cancel(&self) -> Result<(), BridgeError> {
        ListViewHandle::cancel(self);
        Ok(())
    }
}

pulled_read! {
    /// One list's view, read live: the window core keeps around the rows on
    /// screen, with the position of its first row. `set_visible` tells core
    /// which rows the screen shows, and core moves and sizes the window.
    ListViewSubscription: ListViewHandle => BridgeListView,
    converted by BridgeListView::from_core,
    built by list_view
}

#[uniffi::export]
impl ListViewSubscription {
    /// The screen shows the rows from position `first` to position `last`.
    pub fn set_visible(&self, first: u64, last: u64) {
        self.inner.set_visible(Visible::new(first, last));
    }
}

pulled_read! {
    /// One page of a list, read live: the rows at the page's places, none
    /// past the list's end, as a media browser shows them.
    ListPageSubscription:
        bae_store::live_query::LiveValues<bae_views::ListRows> => BridgeListRows,
    converted by BridgeListRows::from_core,
    built by list_page
}

pulled_read! {
    /// Read live how many rows a list has, with the values its count carries
    /// (`BridgeListCountExtra`).
    ListCountSubscription:
        bae_store::live_query::LiveValues<bae_views::ListCount> => BridgeListCount,
    converted by BridgeListCount::from_core,
    built by list_count
}

impl BridgeList {
    /// Converts the album grid's sort from the bridge type to core's type.
    pub(crate) fn album_sort(
        sort: Vec<crate::types::BridgeSortCriterion>,
    ) -> Vec<bae_views::AlbumSortCriterion> {
        sort.into_iter().map(crate::types::BridgeSortCriterion::into_core).collect()
    }
}

impl BridgeList {
    /// Converts the artist list's sort from the bridge type to core's type.
    pub(crate) fn artist_sort(
        sort: Vec<crate::types::BridgeArtistSortCriterion>,
    ) -> Vec<bae_views::ArtistSortCriterion> {
        sort.into_iter().map(crate::types::BridgeArtistSortCriterion::into_core).collect()
    }

    /// Converts the composer list's sort from the bridge type to core's type.
    pub(crate) fn composer_sort(
        sort: Vec<crate::types::BridgeComposerSortCriterion>,
    ) -> Vec<bae_views::ComposerSortCriterion> {
        sort.into_iter().map(crate::types::BridgeComposerSortCriterion::into_core).collect()
    }

    /// Parses a search's text, an artist picker's or library search's, into
    /// a search query, or returns None when the text is blank.
    pub(crate) fn search_query(text: &str) -> Option<bae_views::LibrarySearchQuery> {
        bae_views::LibrarySearchQuery::parse(text)
    }
}

impl BridgeListView {
    pub(crate) fn from_core(value: ListViewValue) -> Self {
        Self { start: value.start, rows: BridgeListRows::from_core(value.rows) }
    }
}

impl BridgeListRows {
    pub(crate) fn from_core(rows: bae_views::ListRows) -> Self {
        match rows {
            bae_views::ListRows::Albums(rows) => BridgeListRows::Albums {
                rows: rows
                    .into_iter()
                    .map(crate::types::BridgeAlbumAppearance::from_core)
                    .collect(),
            },
            bae_views::ListRows::Artists(rows) => BridgeListRows::Artists {
                rows: rows.into_iter().map(crate::types::BridgeArtistSummary::from_core).collect(),
            },
            bae_views::ListRows::Composers(rows) => BridgeListRows::Composers {
                rows: rows
                    .into_iter()
                    .map(crate::types::BridgeComposerSummary::from_core)
                    .collect(),
            },
            bae_views::ListRows::ComposerPage(rows) => BridgeListRows::ComposerPage {
                rows: rows
                    .into_iter()
                    .map(crate::types::BridgeComposerDetailRow::from_core)
                    .collect(),
            },
            bae_views::ListRows::Downloads(rows) => BridgeListRows::Downloads {
                rows: rows.into_iter().map(crate::types::BridgeDownloadOp::from_core).collect(),
            },
            bae_views::ListRows::Upcoming(rows) => BridgeListRows::Upcoming {
                rows: rows.into_iter().map(crate::types::BridgeQueueEntry::from_core).collect(),
            },
            bae_views::ListRows::ArtistSearch(rows) => BridgeListRows::ArtistSearch {
                rows: rows
                    .into_iter()
                    .map(crate::types::BridgeArtistSearchResult::from_core)
                    .collect(),
            },
            bae_views::ListRows::Storage(rows) => BridgeListRows::Storage {
                rows: rows.into_iter().map(crate::types::BridgeStorageRow::from_core).collect(),
            },
            bae_views::ListRows::AlbumTracks(rows) => BridgeListRows::AlbumTracks {
                rows: rows.into_iter().map(crate::types::BridgeBrowseTrack::from_core).collect(),
            },
            bae_views::ListRows::WorkChildren(rows) => BridgeListRows::WorkChildren {
                rows: rows.into_iter().map(crate::types::BridgeWorkChild::from_core).collect(),
            },
            bae_views::ListRows::SearchAlbums(rows) => BridgeListRows::SearchAlbums {
                rows: rows
                    .into_iter()
                    .map(crate::types::BridgeAlbumSearchResult::from_core)
                    .collect(),
            },
            #[cfg(desktop)]
            bae_views::ListRows::Import(items) => BridgeListRows::Import {
                items: items
                    .into_iter()
                    .map(crate::types::BridgeImportListItem::from_core)
                    .collect(),
            },
        }
    }
}

impl BridgeListCount {
    pub(crate) fn from_core(count: bae_views::ListCount) -> Self {
        Self {
            rows: count.rows,
            items: count.items,
            extra: match count.extra {
                bae_views::ListCountExtra::None => BridgeListCountExtra::None,
                bae_views::ListCountExtra::AlbumGrid { can_shuffle, grouped } => {
                    BridgeListCountExtra::AlbumGrid { can_shuffle, grouped }
                }
                bae_views::ListCountExtra::Storage { total_size, searching } => {
                    BridgeListCountExtra::Storage { total_size, searching }
                }
                #[cfg(desktop)]
                bae_views::ListCountExtra::Import { narrowed, tabs } => {
                    let bae_import::TriageTabCounts { pending, done, skipped } = tabs;
                    BridgeListCountExtra::Import {
                        narrowed: narrowed.map(crate::types::BridgeNarrowedCount::from_core),
                        tabs: crate::types::BridgeTriageTabCounts { pending, done, skipped },
                    }
                }
            },
        }
    }
}
