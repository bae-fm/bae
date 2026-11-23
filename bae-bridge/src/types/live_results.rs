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
    /// The line under the artist's name in the group's heading: how many
    /// albums the artist has.
    pub count_line: crate::types::BridgeCountLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLiveQueryCause {
    Request,
    Write,
    RequestAndWrite,
}

impl BridgeLibraryBrowseSection {
    pub(crate) fn from_core(section: bae_views::LibraryBrowseSection) -> Self {
        let count_line = crate::types::BridgeCountLabel::from_core(section.count_line());
        Self {
            id: section.id,
            title: section.title,
            image: section.image.map(crate::types::BridgeImageRef::from_core),
            first_position: section.first_position,
            album_count: section.album_count,
            count_line,
        }
    }
}

/// Where an album's card is in the album grid: its position, and when the
/// grid groups by artist, the id of the artist whose group holds it (the
/// section's `id`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeAlbumPlace {
    pub position: u64,
    pub section_id: Option<String>,
}

impl BridgeAlbumPlace {
    pub(crate) fn from_core(place: bae_views::album::AlbumPlace) -> Self {
        Self { position: place.position, section_id: place.section_id }
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
    /// The download queue, in the order queued.
    Downloads,
    /// The context's tracks after the playing one, in the context lane's
    /// position order, or in its shuffled order when `shuffled` is true.
    Upcoming { shuffled: bool },
    /// The artists an artist picker's `text` finds; blank text finds none.
    ArtistSearch { text: String },
    /// The Storage Manager's releases under `sort` and `filter`, narrowed by
    /// the library search `search` finds when it is not blank.
    Storage {
        sort: crate::types::BridgeStorageSort,
        filter: crate::types::BridgeStorageFilter,
        search: String,
    },
    /// History's plays, newest first: everyone's, or the plays of the person
    /// whose member id `person` names.
    History { person: Option<String> },
}

/// Which list a media browser reads a page or the count of: the lists it
/// shows a page at a time.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePagedList {
    /// The flat album grid under `sort`.
    AlbumGrid { sort: Vec<crate::types::BridgeSortCriterion> },
    /// The composer list under `sort`.
    Composers { sort: Vec<crate::types::BridgeComposerSortCriterion> },
    /// The rows of a composer's page.
    ComposerPage { artist_id: String },
    /// An album's tracks: its main release's, in playing order.
    AlbumTracks { album_id: String },
    /// A work's children: its parts, then the releases that perform it.
    WorkChildren { work_id: String },
    /// The albums library search lists for `query`: the first it finds, up
    /// to library search's result limit (`SEARCH_RESULT_LIMIT`), in title
    /// order. A blank query lists none.
    SearchAlbums { query: String },
}

/// The rows of a window or a page, by the list they belong to.
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
    History {
        rows: Vec<crate::types::BridgeHistoryPlay>,
    },
    /// The import list's items.
    #[cfg(desktop)]
    Import {
        items: Vec<crate::types::BridgeImportListItem>,
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
    /// History's menu of whose plays: the people it offers, this device's
    /// person first and the others by name, and whose plays it shows.
    History {
        people: Vec<super::BridgeHistoryPerson>,
        shown: super::BridgeHistoryShown,
    },
    /// How many of the import list's tab entries its filters keep, while
    /// they narrow it, and every tab's count, which the tab bar shows.
    #[cfg(desktop)]
    Import {
        narrowed: Option<crate::types::BridgeNarrowedCount>,
        tabs: crate::types::BridgeTriageTabCounts,
    },
}
