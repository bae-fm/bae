/// A window of the queue's upcoming tail: where the screen counts it from,
/// after the playing entry, and how many entries it holds.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct BridgeQueueWindow {
    pub offset: u64,
    pub limit: u64,
}

/// One sort value of a row, as the list's order evaluates it for the row.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeSortValue {
    Null,
    Integer { value: i64 },
    Real { value: f64 },
    Text { value: String },
    Blob { value: Vec<u8> },
}

/// One artist's group in the album grid grouped by artist.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryBrowseSection {
    /// The artist's id.
    pub id: String,
    /// The artist's name.
    pub title: String,
    /// The artist's picture, if it has one.
    pub image: Option<crate::types::BridgeImageRef>,
    /// The grid's position of the artist's first album.
    pub first_position: u64,
    /// How many albums the artist has: all of them, never a page of them.
    pub album_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLiveQueryCause {
    Request,
    Write,
    RequestAndWrite,
}

impl BridgeLibraryBrowseSection {
    pub(crate) fn from_core(section: bae_views::LibraryBrowseSection) -> Self {
        Self {
            id: section.id,
            title: section.title,
            image: section.image.map(crate::types::BridgeImageRef::from_core),
            first_position: section.first_position,
            album_count: section.album_count,
        }
    }
}

/// Which list a window, a count or a position is of. A list is a value, not a
/// type of its own, because the bridge exports no generics.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeList {
    /// The album grid under `sort`, flat or grouped by artist.
    AlbumGrid { sort: Vec<crate::types::BridgeSortCriterion>, group_by_artist: bool },
    /// The artist list under `sort`.
    Artists { sort: Vec<crate::types::BridgeArtistSortCriterion> },
    /// The composer list under `sort`.
    Composers { sort: Vec<crate::types::BridgeComposerSortCriterion> },
    /// The rows of a composer's page.
    ComposerPage { artist_id: String },
    /// The download queue, in the order queued.
    Downloads,
    /// The artists an artist picker's `text` finds; blank text finds none.
    ArtistSearch { text: String },
    /// The Storage Manager's releases under `sort` and `filter`, narrowed by
    /// the library search `search` finds when it is not blank.
    Storage {
        sort: crate::types::BridgeStorageSort,
        filter: crate::types::BridgeStorageFilter,
        search: String,
    },
}

/// A window of a list: a range of its order, bounded by the sort values of a
/// row at each end, which a delivered window gave for its first row or for
/// the row beyond its far end.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeListWindow {
    /// The rows at or after `from` (the list's top when none) and before
    /// `until` (no end when none), at most `limit` of them.
    Forward { from: Option<Vec<BridgeSortValue>>, until: Option<Vec<BridgeSortValue>>, limit: u64 },
    /// The `limit` rows nearest before `until`, in the list's order.
    Backward { until: Vec<BridgeSortValue>, limit: u64 },
}

/// The rows of a window, by the list they belong to.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeListRows {
    Albums {
        rows: Vec<crate::types::BridgeAlbumAppearance>,
    },
    Artists {
        rows: Vec<crate::types::BridgeArtistSummary>,
    },
    Composers {
        rows: Vec<crate::types::BridgeComposerSummary>,
    },
    ComposerPage {
        rows: Vec<crate::types::BridgeComposerDetailRow>,
    },
    Downloads {
        rows: Vec<crate::types::BridgeDownloadOp>,
    },
    ArtistSearch {
        rows: Vec<crate::types::BridgeArtistSearchResult>,
    },
    Storage {
        rows: Vec<crate::types::BridgeStorageRow>,
    },
    /// The import list's items, and the header of the group the first of
    /// them is a member of when the window starts inside the group, which
    /// the window does not hold.
    #[cfg(desktop)]
    Import {
        items: Vec<crate::types::BridgeImportListItem>,
        leading_header: Option<crate::types::BridgeImportListItem>,
    },
}

/// One delivery of a list's window.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeListWindowValue {
    /// The window this answers, so a screen that has since replaced it drops
    /// the value.
    pub window: BridgeListWindow,
    pub rows: BridgeListRows,
    /// The sort values of the first row; none for a window of no rows.
    pub first: Option<Vec<BridgeSortValue>>,
    /// The sort values of the row past the window's far end: below its last
    /// row when its limit cut it short, above its first row for a backward
    /// window. None when it reached its bound, the list's end or the list's
    /// top.
    pub beyond: Option<Vec<BridgeSortValue>>,
}

/// One delivery of a list's count.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeListCount {
    /// How many rows the list has, which sizes its scroll view.
    pub rows: u64,
    pub extra: BridgeListCountExtra,
}

/// What a list's count carries beside its rows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeListCountExtra {
    None,
    /// The album grid's album count; grouped by artist it is less than its
    /// rows.
    AlbumGrid {
        albums: u64,
    },
    /// The Storage Manager's total size of its releases' files, and whether
    /// a search narrows the list.
    Storage {
        total_size: u64,
        searching: bool,
    },
    /// How many of the import list's tab entries its filters keep, while
    /// they narrow it.
    #[cfg(desktop)]
    Import {
        narrowed: Option<crate::types::BridgeNarrowedCount>,
    },
}

/// One delivery of where a key sits in a list.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeListPosition {
    pub key: Vec<BridgeSortValue>,
    /// How many rows come before the row at the key.
    pub rows_before: u64,
}
