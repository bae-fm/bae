use bae_db::live_query::CancellableLiveQuery;
use bae_db::{LibraryBrowseWindow, LibraryPageWindow, LibraryPageWindows};
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
    pub image: Option<bae_db::ImageRef>,
    /// Where the artist's albums start in the grid's rows, and all of them:
    /// never a page of them, so `limit` is how many albums the artist has.
    pub window: LibraryPageWindow,
}

#[derive(Debug, Clone)]
pub struct LibraryBrowseSnapshot<Row> {
    pub windows: Vec<LibraryBrowseWindow<Row>>,
    pub sections: Vec<LibraryBrowseSection>,
    pub total_count: u64,
    /// Number of displayed appearances, including albums in multiple sections.
    pub row_count: u64,
    pub request_revision: u64,
    pub cause: coven::LiveQueryCause,
}

pub type LiveAlbumBrowse = LiveLibraryBrowse<
    crate::library::views::AlbumBrowseProjection,
    crate::album_detail::AlbumAppearance,
>;
pub type LiveArtistBrowse = LiveLibraryBrowse<
    crate::library::views::ArtistBrowseProjection,
    crate::album_detail::ArtistSummary,
>;
pub type LiveComposerBrowse = LiveLibraryBrowse<
    crate::library::views::ComposerBrowseProjection,
    crate::album_detail::ComposerSummary,
>;

/// The download queue as the Downloads pane draws it: its windows, each row
/// an entry beside its release.
pub type LiveDownloadBrowse =
    LiveLibraryBrowse<bae_transfers::ReleaseQueueWindows<()>, bae_transfers::DownloadRow>;

#[derive(Debug, thiserror::Error)]
pub enum LiveLibraryBrowseError {
    #[error("browse subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The live query ending is this subscription's cancellation.
impl From<bae_db::live_query::LiveQueryCancelled> for LiveLibraryBrowseError {
    fn from(_: bae_db::live_query::LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

pub struct LiveLibraryBrowse<Projection, Row> {
    query: CancellableLiveQuery<LibraryPageWindows, Projection>,
    resolve: Arc<
        dyn Fn(Projection, u64, coven::LiveQueryCause) -> LibraryBrowseSnapshot<Row> + Send + Sync,
    >,
}

impl<Projection, Row> LiveLibraryBrowse<Projection, Row>
where
    Projection: Clone + PartialEq + Send + 'static,
    Row: Send + 'static,
{
    pub fn new(
        query: CancellableLiveQuery<LibraryPageWindows, Projection>,
        resolve: impl Fn(Projection, u64, coven::LiveQueryCause) -> LibraryBrowseSnapshot<Row>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self { query, resolve: Arc::new(resolve) }
    }

    pub fn set_windows(&self, windows: LibraryPageWindows) -> Result<(), LiveLibraryBrowseError> {
        self.query.set(windows).map(|_| ()).map_err(LiveLibraryBrowseError::from)
    }

    pub async fn next(&self) -> Result<LibraryBrowseSnapshot<Row>, LiveLibraryBrowseError> {
        let event = self.query.next().await.map_err(LiveLibraryBrowseError::from)?;
        let revision = event.revision.0;
        let cause = event.cause;
        let projection = event.result?;
        Ok((self.resolve)(projection, revision, cause))
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
