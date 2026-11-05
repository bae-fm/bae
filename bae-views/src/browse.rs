use bae_store::live_query::{CancellableLiveQuery, LiveQueryError};
use bae_store::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};
use std::sync::Arc;

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

pub type LiveAlbumBrowse = LiveLibraryBrowse<crate::AlbumBrowseProjection, AlbumBrowseSnapshot>;
pub type LiveArtistBrowse = LiveLibraryBrowse<
    crate::ArtistBrowseProjection,
    LibraryBrowseSnapshot<crate::album_detail::ArtistSummary>,
>;
pub type LiveComposerBrowse = LiveLibraryBrowse<
    crate::ComposerBrowseProjection,
    LibraryBrowseSnapshot<crate::album_detail::ComposerSummary>,
>;
/// A composer's page's rows, in the windows its screen shows.
pub type LiveComposerDetailRows = LiveLibraryBrowse<
    crate::ComposerDetailRowsProjection,
    LibraryBrowseSnapshot<crate::album_detail::ComposerDetailRow>,
>;

/// The download queue as the Downloads pane draws it: its windows, each row
/// an entry beside its release.
pub type LiveDownloadBrowse = LiveLibraryBrowse<
    bae_transfers::ReleaseQueueWindows<()>,
    LibraryBrowseSnapshot<bae_transfers::DownloadRow>,
>;

/// A windowed list read live: the windows it reads move in place, and each
/// read resolves to the `Snapshot` its screen takes.
pub struct LiveLibraryBrowse<Projection, Snapshot> {
    query: CancellableLiveQuery<LibraryPageWindows, Projection>,
    resolve: Arc<dyn Fn(Projection, u64, coven::LiveQueryCause) -> Snapshot + Send + Sync>,
}

impl<Projection, Snapshot> LiveLibraryBrowse<Projection, Snapshot>
where
    Projection: Clone + PartialEq + Send + 'static,
    Snapshot: Send + 'static,
{
    pub fn new(
        query: CancellableLiveQuery<LibraryPageWindows, Projection>,
        resolve: impl Fn(Projection, u64, coven::LiveQueryCause) -> Snapshot + Send + Sync + 'static,
    ) -> Self {
        Self { query, resolve: Arc::new(resolve) }
    }

    pub fn set_windows(&self, windows: LibraryPageWindows) -> Result<(), LiveQueryError> {
        self.query.set(windows).map(|_| ()).map_err(LiveQueryError::from)
    }

    pub async fn next(&self) -> Result<Snapshot, LiveQueryError> {
        let event = self.query.next().await.map_err(LiveQueryError::from)?;
        let revision = event.revision.0;
        let cause = event.cause;
        let projection = event.result?;
        Ok((self.resolve)(projection, revision, cause))
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
