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

/// Which list a view or a count is of. A list is a value, not a type of its
/// own, because the bridge exports no generics.
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
    /// An album's tracks as a media browser lists them: its main release's,
    /// in playing order.
    AlbumTracks { album_id: String },
    /// A work's children as a media browser lists them: its parts, then the
    /// releases that perform it.
    WorkChildren { work_id: String },
    /// The download queue, in the order queued.
    Downloads,
    /// The context's tracks after the playing one, in the context lane's
    /// position order, or in its shuffled order when `shuffled` is true.
    Upcoming { shuffled: bool },
    /// The artists an artist picker's `text` finds; blank text finds none.
    ArtistSearch { text: String },
    /// The albums library search lists for `query` as a media browser lists
    /// them: the first it finds, up to library search's result limit
    /// (`SEARCH_RESULT_LIMIT`), in title order. A blank query lists none.
    SearchAlbums { query: String },
    /// The Storage Manager's releases under `sort` and `filter`, narrowed by
    /// the library search `search` finds when it is not blank.
    Storage {
        sort: crate::types::BridgeStorageSort,
        filter: crate::types::BridgeStorageFilter,
        search: String,
    },
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
    Upcoming {
        rows: Vec<crate::types::BridgeQueueEntry>,
    },
    ArtistSearch {
        rows: Vec<crate::types::BridgeArtistSearchResult>,
    },
    Storage {
        rows: Vec<crate::types::BridgeStorageRow>,
    },
    AlbumTracks {
        rows: Vec<crate::types::BridgeBrowseTrack>,
    },
    WorkChildren {
        rows: Vec<crate::types::BridgeWorkChild>,
    },
    SearchAlbums {
        rows: Vec<crate::types::BridgeAlbumSearchResult>,
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

/// One delivery of a list's view: the rows of the window core keeps around
/// the rows on screen, the first at position `start`, each next one at the
/// position after.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeListView {
    pub start: u64,
    pub rows: BridgeListRows,
}

/// One delivery of a list's count.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeListCount {
    /// How many rows the list has, which sizes its scroll view.
    pub rows: u64,
    /// How many distinct items the rows show: the album grid's albums, which
    /// grouped by artist show once under each of their artists, and the row
    /// count for every other list.
    pub items: u64,
    pub extra: BridgeListCountExtra,
}

/// What a list's count carries beside its rows.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeListCountExtra {
    None,
    AlbumGrid {
        /// Whether the library holds at least one track, so Shuffle Library
        /// has something to play.
        can_shuffle: bool,
        /// Whether the grid groups its rows by artist, so the grid lays them
        /// out from the read that counts them.
        grouped: bool,
    },
    /// The Storage Manager's total size of its releases' files, and whether
    /// a search narrows the list.
    Storage {
        total_size: u64,
        searching: bool,
    },
    /// How many of the import list's tab entries its filters keep, while
    /// they narrow it, and every tab's count, which the tab bar shows.
    #[cfg(desktop)]
    Import {
        narrowed: Option<crate::types::BridgeNarrowedCount>,
        tabs: crate::types::BridgeTriageTabCounts,
    },
}
