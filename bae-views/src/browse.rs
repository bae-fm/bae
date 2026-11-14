use bae_store::list_windows::OrderKey;
use bae_store::live_query::LiveRead;
use bae_store::{LibraryBrowseWindow, LibraryPageWindows, ListWindow, WindowRead};

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

/// One delivery of a windowed list: each requested window with its rows, and
/// how many rows the list holds.
#[derive(Debug, Clone)]
pub struct LibraryBrowseSnapshot<Row> {
    pub windows: Vec<LibraryBrowseWindow<Row>>,
    pub total_count: u64,
    pub request_revision: u64,
    pub cause: coven::LiveQueryCause,
}

/// The rows of a list's window, by the list they belong to.
#[derive(Debug, Clone)]
pub enum ListRows {
    /// The album grid's rows, each an album appearance.
    Albums(Vec<crate::album_detail::AlbumAppearance>),
}

/// One delivery of a list's window: the request it answers, its rows, the
/// key of its first row and the key of the row beyond its far end.
#[derive(Debug, Clone)]
pub struct ListWindowValue {
    pub window: ListWindow,
    pub rows: ListRows,
    pub first: Option<OrderKey>,
    pub beyond: Option<OrderKey>,
}

impl ListWindowValue {
    /// `read`, its rows put into the variant of [`ListRows`] `rows` makes.
    pub fn of<R>(read: WindowRead<R>, rows: impl FnOnce(Vec<R>) -> ListRows) -> Self {
        let WindowRead { window, rows: read_rows, first, beyond } = read;
        Self { window, rows: rows(read_rows), first, beyond }
    }
}

/// A window of a list read live: the window moves in place, and each read
/// answers the window last set.
pub type LiveListWindow = LiveRead<ListWindow, ListWindowValue>;

/// One delivery of a list's count: how many rows it has, which sizes its
/// scroll view, and what changes with them.
#[derive(Debug, Clone, PartialEq)]
pub struct ListCount {
    pub rows: u64,
    pub extra: ListCountExtra,
}

/// What a list's count carries beside its rows.
#[derive(Debug, Clone, PartialEq)]
pub enum ListCountExtra {
    None,
    /// The album grid's album count, which grouped by artist is not its row
    /// count: an album shows once under each of its artists.
    AlbumGrid {
        albums: u64,
    },
}

/// Where a key sits in a list: how many rows come before it.
#[derive(Debug, Clone, PartialEq)]
pub struct ListPosition {
    pub key: OrderKey,
    pub rows_before: u64,
}

/// The position of a key read live: the key moves in place, and each read
/// answers the key last set.
pub type LiveListPosition = LiveRead<OrderKey, ListPosition>;

/// A windowed list read live: the windows it reads move in place, and each
/// read resolves to the snapshot its screen takes, carrying every window the
/// request names, read fresh.
pub type LiveLibraryBrowse<Snapshot> = LiveRead<LibraryPageWindows, Snapshot>;

pub type LiveArtistBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<crate::DbArtistSummary>>;
pub type LiveComposerBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<crate::DbComposerSummary>>;
/// A composer's page's rows, in the windows its screen shows.
pub type LiveComposerDetailRows =
    LiveLibraryBrowse<LibraryBrowseSnapshot<crate::album_detail::ComposerDetailRow>>;

/// The download queue as the Downloads pane draws it: its windows, each row
/// an entry beside its release.
pub type LiveDownloadBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<bae_transfers::DownloadRow>>;
