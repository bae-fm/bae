use bae_store::list_windows::OrderKey;
use bae_store::live_query::LiveRead;
use bae_store::{ListWindow, WindowRead};

/// One artist's group in the album grid grouped by artist. Its range includes
/// unloaded rows, so clients can lay out every section without fetching every
/// album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBrowseSection {
    /// The artist's id.
    pub id: String,
    /// The artist's name.
    pub title: String,
    /// The artist's picture, if it has one.
    pub image: Option<bae_records::ImageRef>,
    /// The grid's position of the artist's first album.
    pub first_position: u64,
    /// How many albums the artist has: all of them, never a page of them.
    pub album_count: u64,
}

impl LibraryBrowseSection {
    /// The line under the artist's name in its group's heading: how many
    /// albums credit it, as in the artist list.
    pub fn count_line(&self) -> bae_types::count_label::CountLabel {
        crate::models::album_count_line(self.album_count)
    }
}

/// The rows of a list's window or page, by the list they belong to.
#[derive(Debug, Clone, PartialEq)]
pub enum ListRows {
    /// The album grid's rows, each an album appearance.
    Albums(Vec<crate::album_detail::AlbumAppearance>),
    /// The artist list's rows.
    Artists(Vec<crate::DbArtistSummary>),
    /// The composer list's rows.
    Composers(Vec<crate::DbComposerSummary>),
    /// A composer's page's rows: its works, then its unlinked credits.
    ComposerPage(Vec<crate::album_detail::ComposerDetailRow>),
    /// The download queue's entries, each with its release.
    Downloads(Vec<bae_transfers::DownloadRow>),
    /// The tracks after the playing one, as the queue shows them.
    Upcoming(Vec<bae_playback::QueueItem>),
    /// The artists an artist picker's text finds.
    ArtistSearch(Vec<crate::album_detail::ArtistSearchResult>),
    /// The Storage Manager's rows, each a release beside its album.
    Storage(Vec<crate::album_detail::StorageRow>),
    /// An album's tracks as a media browser lists them: its main release's.
    AlbumTracks(Vec<crate::album_detail::TrackSearchResult>),
    /// A work's children as a media browser lists them: its parts, then the
    /// releases that perform it.
    WorkChildren(Vec<crate::album_detail::WorkChild>),
    /// The albums a library search lists, in title order.
    SearchAlbums(Vec<crate::album_detail::AlbumSearchResult>),
    /// History's plays, newest first.
    History(Vec<crate::history::HistoryRow>),
    /// The import list's items.
    #[cfg(desktop)]
    Import(Vec<bae_import::ImportListItem>),
}

/// One delivery of a list's window: the request it answers, its rows, and
/// each row's key, in the same order.
#[derive(Debug, Clone)]
pub struct ListWindowValue {
    pub window: ListWindow,
    pub rows: ListRows,
    pub keys: Vec<OrderKey>,
}

impl ListWindowValue {
    /// Build the value from `read`, with its rows in the [`ListRows`] variant
    /// that `rows` returns.
    pub fn of<R>(read: WindowRead<R>, rows: impl FnOnce(Vec<R>) -> ListRows) -> Self {
        let WindowRead { window, rows: keyed } = read;
        let (read_rows, keys) = keyed.into_iter().unzip();
        Self { window, rows: rows(read_rows), keys }
    }
}

/// A window of a list read live: the list's view moves the window on the
/// same live query, and each read answers the window last set.
pub type LiveListWindow = LiveRead<ListWindow, ListWindowValue>;

/// One delivery of a list's count: how many rows it has, which sizes its
/// scroll view, and what changes with them.
#[derive(Debug, Clone, PartialEq)]
pub struct ListCount {
    pub rows: u64,
    /// How many distinct items the rows show: the album grid's albums, which
    /// grouped by artist show once under each of their artists, and the row
    /// count for every other list.
    pub items: u64,
    pub extra: ListCountExtra,
}

/// What a list's count carries beside its rows.
#[derive(Debug, Clone, PartialEq)]
pub enum ListCountExtra {
    None,
    /// Whether the library holds at least one track, so the album grid's
    /// Shuffle has something to play, and whether the grid groups its rows
    /// by artist.
    AlbumGrid {
        can_shuffle: bool,
        grouped: bool,
    },
    /// The Storage Manager's total size of its releases' files, and whether
    /// a search narrows the list.
    Storage {
        total_size: u64,
        searching: bool,
    },
    /// History's menu of whose plays: the people it offers, and whose plays
    /// it shows.
    History {
        people: Vec<crate::history::HistoryPerson>,
        shown: crate::history::HistoryShown,
    },
    /// The import list's count while filters narrow it: how many of its
    /// tab's entries the filters keep. The variant also holds the count of
    /// every tab.
    #[cfg(desktop)]
    Import {
        narrowed: Option<bae_import::NarrowedCount>,
        tabs: bae_import::TriageTabCounts,
    },
}

/// Where a key sits in a list: how many rows come before it.
#[derive(Debug, Clone, PartialEq)]
pub struct ListPosition {
    pub key: OrderKey,
    pub rows_before: u64,
}

/// The position of a key read live: the caller changes the key on the same
/// live query, and each read answers the key last set.
pub type LiveListPosition = LiveRead<OrderKey, ListPosition>;
