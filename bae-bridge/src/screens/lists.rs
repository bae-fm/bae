//! The library's lists as the screens that draw them pull them: each window
//! of a list, its count and the position of a key in it is a live read of its
//! own, the same objects for every list.

use crate::types::{
    BridgeError, BridgeList, BridgeListCount, BridgeListCountExtra, BridgeListPosition,
    BridgeListRows, BridgeListWindow, BridgeListWindowValue, BridgeSortValue,
};
use bae_store::list_windows::OrderKey;
use bae_store::ListWindow;

pulled_read! {
    /// One window of a list, a range of its order, read live: it moves in
    /// place with `set_window`, and the list's other windows are other
    /// subscriptions.
    ListWindowSubscription: bae_views::LiveListWindow => BridgeListWindowValue,
    converted by BridgeListWindowValue::from_core,
    built by list_window
}

#[uniffi::export]
impl ListWindowSubscription {
    /// Read `window` of the list from now on.
    pub fn set_window(&self, window: BridgeListWindow) -> Result<(), BridgeError> {
        self.inner.set(window.into_core()).map(|_revision| ()).map_err(BridgeError::from)
    }
}

pulled_read! {
    /// How many rows a list has, and what changes with them, read live.
    ListCountSubscription:
        bae_store::live_query::LiveValues<bae_views::ListCount> => BridgeListCount,
    converted by BridgeListCount::from_core,
    built by list_count
}

pulled_read! {
    /// How many rows of a list come before the row at a key, read live: it
    /// moves in place with `set_key`.
    ListPositionSubscription: bae_views::LiveListPosition => BridgeListPosition,
    converted by BridgeListPosition::from_core,
    built by list_position
}

#[uniffi::export]
impl ListPositionSubscription {
    /// Read the position of the row at `key` from now on.
    pub fn set_key(&self, key: Vec<BridgeSortValue>) -> Result<(), BridgeError> {
        self.inner.set(order_key(key)).map(|_revision| ()).map_err(BridgeError::from)
    }
}

/// The sort values a screen sent back, as the key they are.
pub(crate) fn order_key(values: Vec<BridgeSortValue>) -> OrderKey {
    OrderKey::new(values.into_iter().map(BridgeSortValue::into_core).collect())
}

impl BridgeList {
    /// The album grid's sort in core's terms.
    pub(crate) fn album_sort(
        sort: Vec<crate::types::BridgeSortCriterion>,
    ) -> Vec<bae_views::AlbumSortCriterion> {
        sort.into_iter().map(crate::types::BridgeSortCriterion::into_core).collect()
    }
}

impl BridgeList {
    /// The artist list's sort in core's terms.
    pub(crate) fn artist_sort(
        sort: Vec<crate::types::BridgeArtistSortCriterion>,
    ) -> Vec<bae_views::ArtistSortCriterion> {
        sort.into_iter().map(crate::types::BridgeArtistSortCriterion::into_core).collect()
    }

    /// The composer list's sort in core's terms.
    pub(crate) fn composer_sort(
        sort: Vec<crate::types::BridgeComposerSortCriterion>,
    ) -> Vec<bae_views::ComposerSortCriterion> {
        sort.into_iter().map(crate::types::BridgeComposerSortCriterion::into_core).collect()
    }

    /// The Storage Manager's sort in core's terms.
    pub(crate) fn storage_sort(
        sort: crate::types::BridgeStorageSort,
    ) -> bae_views::StorageSortCriterion {
        sort.into_core()
    }

    /// What an artist picker's text searches for: a blank text is no search.
    pub(crate) fn artist_search_query(text: &str) -> Option<bae_views::LibrarySearchQuery> {
        bae_views::LibrarySearchQuery::parse(text)
    }
}

impl BridgeListWindow {
    pub(crate) fn into_core(self) -> ListWindow {
        let key = |key: Vec<BridgeSortValue>| order_key(key);
        match self {
            Self::Forward { from, until, limit } => {
                ListWindow::Forward { from: from.map(key), until: until.map(key), limit }
            }
            Self::Backward { until, limit } => ListWindow::Backward { until: key(until), limit },
        }
    }

    fn from_core(window: ListWindow) -> Self {
        let values = |key: &OrderKey| BridgeSortValue::of_key(key);
        match window {
            ListWindow::Forward { from, until, limit } => Self::Forward {
                from: from.as_ref().map(values),
                until: until.as_ref().map(values),
                limit,
            },
            ListWindow::Backward { until, limit } => {
                Self::Backward { until: values(&until), limit }
            }
        }
    }
}

impl BridgeListWindowValue {
    fn from_core(value: bae_views::ListWindowValue) -> Self {
        Self {
            window: BridgeListWindow::from_core(value.window),
            rows: match value.rows {
                bae_views::ListRows::Albums(rows) => BridgeListRows::Albums {
                    rows: rows
                        .into_iter()
                        .map(crate::types::BridgeAlbumAppearance::from_core)
                        .collect(),
                },
                bae_views::ListRows::Artists(rows) => BridgeListRows::Artists {
                    rows: rows
                        .into_iter()
                        .map(crate::types::BridgeArtistSummary::from_core)
                        .collect(),
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
                bae_views::ListRows::ArtistSearch(rows) => BridgeListRows::ArtistSearch {
                    rows: rows
                        .into_iter()
                        .map(crate::types::BridgeArtistSearchResult::from_core)
                        .collect(),
                },
                bae_views::ListRows::Storage(rows) => BridgeListRows::Storage {
                    rows: rows.into_iter().map(crate::types::BridgeStorageRow::from_core).collect(),
                },
            },
            first: value.first.as_ref().map(BridgeSortValue::of_key),
            beyond: value.beyond.as_ref().map(BridgeSortValue::of_key),
        }
    }
}

impl BridgeListCount {
    fn from_core(count: bae_views::ListCount) -> Self {
        Self {
            rows: count.rows,
            extra: match count.extra {
                bae_views::ListCountExtra::None => BridgeListCountExtra::None,
                bae_views::ListCountExtra::AlbumGrid { albums } => {
                    BridgeListCountExtra::AlbumGrid { albums }
                }
                bae_views::ListCountExtra::Storage { total_size, searching } => {
                    BridgeListCountExtra::Storage { total_size, searching }
                }
            },
        }
    }
}

impl BridgeListPosition {
    fn from_core(position: bae_views::ListPosition) -> Self {
        Self { key: BridgeSortValue::of_key(&position.key), rows_before: position.rows_before }
    }
}
