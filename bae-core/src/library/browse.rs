use crate::live_query::CancellableLiveQuery;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
}

pub type LibraryPageWindows = BTreeSet<LibraryPageWindow>;

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryBrowseWindow<Row> {
    pub window: LibraryPageWindow,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone)]
pub struct LibraryBrowseSnapshot<Row> {
    pub windows: Vec<LibraryBrowseWindow<Row>>,
    pub total_count: u64,
    pub request_revision: u64,
    pub cause: coven::ReconfigurableLiveQueryCause,
}

pub type AlbumBrowseSubscription =
    LibraryBrowseSubscription<crate::db::AlbumBrowseProjection, crate::album_detail::AlbumSummary>;
pub type ComposerBrowseSubscription = LibraryBrowseSubscription<
    crate::db::ComposerBrowseProjection,
    crate::album_detail::ComposerSummary,
>;

#[derive(Debug, thiserror::Error)]
pub enum LibraryBrowseSubscriptionError {
    #[error("browse subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

pub struct LibraryBrowseSubscription<Projection, Row> {
    query: CancellableLiveQuery<LibraryPageWindows, Projection>,
    resolve: Arc<
        dyn Fn(Projection, u64, coven::ReconfigurableLiveQueryCause) -> LibraryBrowseSnapshot<Row>
            + Send
            + Sync,
    >,
}

impl<Projection, Row> LibraryBrowseSubscription<Projection, Row>
where
    Projection: Clone + PartialEq + Send + 'static,
    Row: Send + 'static,
{
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<LibraryPageWindows, Projection>,
        resolve: impl Fn(Projection, u64, coven::ReconfigurableLiveQueryCause) -> LibraryBrowseSnapshot<Row>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            query: CancellableLiveQuery::new(query),
            resolve: Arc::new(resolve),
        }
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), LibraryBrowseSubscriptionError> {
        self.query
            .set(windows)
            .map(|_| ())
            .map_err(|_| LibraryBrowseSubscriptionError::Cancelled)
    }

    pub async fn next(&self) -> Result<LibraryBrowseSnapshot<Row>, LibraryBrowseSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(|_| LibraryBrowseSubscriptionError::Cancelled)?;
        let revision = event.revision().get();
        let cause = event.cause();
        let projection = event.into_result()?;
        Ok((self.resolve)(projection, revision, cause))
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
