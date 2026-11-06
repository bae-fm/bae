use bae_store::live_query::LiveRead;
use bae_store::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};

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
    /// Where the artist's albums start in the grid's rows, and all of them:
    /// never a page of them, so `limit` is how many albums the artist has.
    pub window: LibraryPageWindow,
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

/// One delivery of the album grid. Grouped by artist, an album shows once in
/// each of its artists' sections, so the grid's positions count appearances:
/// `row_count` of them, over the list's `total_count` albums. Flat, the two
/// are equal and there are no sections.
#[derive(Debug, Clone)]
pub struct AlbumBrowseSnapshot {
    pub list: LibraryBrowseSnapshot<crate::album_detail::AlbumAppearance>,
    pub sections: Vec<LibraryBrowseSection>,
    pub row_count: u64,
}

/// A windowed list read live: the windows it reads move in place, and each
/// read resolves to the snapshot its screen takes.
pub type LiveLibraryBrowse<Snapshot> = LiveRead<LibraryPageWindows, Snapshot>;

pub type LiveAlbumBrowse = LiveLibraryBrowse<AlbumBrowseSnapshot>;
pub type LiveArtistBrowse =
    LiveLibraryBrowse<LibraryBrowseSnapshot<crate::album_detail::ArtistSummary>>;
pub type LiveComposerBrowse =
    LiveLibraryBrowse<LibraryBrowseSnapshot<crate::album_detail::ComposerSummary>>;
/// A composer's page's rows, in the windows its screen shows.
pub type LiveComposerDetailRows =
    LiveLibraryBrowse<LibraryBrowseSnapshot<crate::album_detail::ComposerDetailRow>>;

/// The download queue as the Downloads pane draws it: its windows, each row
/// an entry beside its release.
pub type LiveDownloadBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<bae_transfers::DownloadRow>>;
