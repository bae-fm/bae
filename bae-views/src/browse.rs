use bae_store::live_query::LiveRead;
use bae_store::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows, WindowsTold};

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
/// are equal. Grouped, `sections` are the artists' headings, delivered when
/// they changed since the delivery before and in the delivery whose rows they
/// fit: `None` says the last ones still stand.
#[derive(Debug, Clone)]
pub struct AlbumBrowseSnapshot {
    pub list: LibraryBrowseSnapshot<crate::album_detail::AlbumAppearance>,
    pub sections: Option<Vec<LibraryBrowseSection>>,
    pub row_count: u64,
}

/// A snapshot that names windows of a list, so a read can leave out those the
/// screen already holds.
pub trait WindowedSnapshot: Send + 'static {
    type Row: Clone + PartialEq + Send + 'static;

    fn windows_mut(&mut self) -> &mut Vec<LibraryBrowseWindow<Self::Row>>;
}

impl<Row: Clone + PartialEq + Send + 'static> WindowedSnapshot for LibraryBrowseSnapshot<Row> {
    type Row = Row;

    fn windows_mut(&mut self) -> &mut Vec<LibraryBrowseWindow<Row>> {
        &mut self.windows
    }
}

impl WindowedSnapshot for AlbumBrowseSnapshot {
    type Row = crate::album_detail::AlbumAppearance;

    fn windows_mut(&mut self) -> &mut Vec<LibraryBrowseWindow<Self::Row>> {
        &mut self.list.windows
    }
}

/// A windowed list read live: the windows it reads move in place, and each
/// read resolves to the snapshot its screen takes, naming only the windows
/// whose rows differ from the last it delivered for them, and every window
/// asked for since. A write that moves the count, or a row off every window,
/// delivers no rows.
pub struct LiveLibraryBrowse<Snapshot: WindowedSnapshot> {
    read: LiveRead<LibraryPageWindows, Snapshot>,
    told: WindowsTold<Snapshot::Row>,
}

impl<Snapshot: WindowedSnapshot> LiveLibraryBrowse<Snapshot> {
    /// `query`'s reads that answer the windows last set, each resolved by
    /// `resolve`; it starts with no windows.
    pub fn new<Projection: Clone + PartialEq + Send + 'static>(
        query: bae_store::live_query::CancellableLiveQuery<
            LibraryPageWindows,
            Projection,
            bae_store::DbError,
        >,
        resolve: impl FnMut(
                bae_store::live_query::ReadFor<'_, LibraryPageWindows>,
                Projection,
            ) -> Option<Snapshot>
            + Send
            + 'static,
    ) -> Self {
        Self {
            read: LiveRead::new(query, resolve),
            told: WindowsTold::new(LibraryPageWindows::new()),
        }
    }

    /// Read `windows` from now on, and return the revision each read for
    /// them carries.
    pub fn set(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<u64, bae_store::live_query::LiveQueryError<bae_store::DbError>> {
        self.told.request(&windows);
        self.read.set(windows)
    }

    /// The next value that answers the windows last set.
    pub async fn next(
        &self,
    ) -> Result<Snapshot, bae_store::live_query::LiveQueryError<bae_store::DbError>> {
        let mut snapshot = self.read.next().await?;
        let windows = snapshot.windows_mut();
        *windows = self.told.changed(std::mem::take(windows));
        Ok(snapshot)
    }

    /// Stop reading.
    pub async fn cancel(&self) {
        self.read.cancel().await;
    }
}

pub type LiveAlbumBrowse = LiveLibraryBrowse<AlbumBrowseSnapshot>;
pub type LiveArtistBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<crate::DbArtistSummary>>;
pub type LiveComposerBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<crate::DbComposerSummary>>;
/// A composer's page's rows, in the windows its screen shows.
pub type LiveComposerDetailRows =
    LiveLibraryBrowse<LibraryBrowseSnapshot<crate::album_detail::ComposerDetailRow>>;

/// The download queue as the Downloads pane draws it: its windows, each row
/// an entry beside its release.
pub type LiveDownloadBrowse = LiveLibraryBrowse<LibraryBrowseSnapshot<bae_transfers::DownloadRow>>;
