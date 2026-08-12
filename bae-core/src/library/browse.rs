use std::collections::BTreeSet;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LibraryPageWindow {
    pub offset: u64,
    pub limit: u64,
}

pub type LibraryPageWindows = BTreeSet<LibraryPageWindow>;

#[derive(Debug, Clone)]
pub struct LibraryBrowseWindow<Row> {
    pub window: LibraryPageWindow,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone)]
pub struct LibraryBrowseSnapshot<Row> {
    pub windows: Vec<LibraryBrowseWindow<Row>>,
    pub total_count: u64,
    pub request_revision: u64,
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
    requests: std::sync::Mutex<Option<coven::LiveQueryRequests<LibraryPageWindows>>>,
    query:
        tokio::sync::Mutex<Option<coven::ReconfigurableLiveQuery<LibraryPageWindows, Projection>>>,
    resolve: Arc<dyn Fn(Projection, u64) -> LibraryBrowseSnapshot<Row> + Send + Sync>,
    cancellation: CancellationToken,
}

impl<Projection, Row> LibraryBrowseSubscription<Projection, Row>
where
    Projection: Send + 'static,
    Row: Send + 'static,
{
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<LibraryPageWindows, Projection>,
        resolve: impl Fn(Projection, u64) -> LibraryBrowseSnapshot<Row> + Send + Sync + 'static,
    ) -> Self {
        let requests = query.requests();
        Self {
            requests: std::sync::Mutex::new(Some(requests)),
            query: tokio::sync::Mutex::new(Some(query)),
            resolve: Arc::new(resolve),
            cancellation: CancellationToken::new(),
        }
    }

    pub fn set_windows(
        &self,
        windows: LibraryPageWindows,
    ) -> Result<(), LibraryBrowseSubscriptionError> {
        if self.cancellation.is_cancelled() {
            return Err(LibraryBrowseSubscriptionError::Cancelled);
        }
        self.requests
            .lock()
            .expect("browse request mutex poisoned")
            .as_ref()
            .ok_or(LibraryBrowseSubscriptionError::Cancelled)?
            .set(windows)
            .expect("the browse subscription owns its live query");
        Ok(())
    }

    pub async fn next(&self) -> Result<LibraryBrowseSnapshot<Row>, LibraryBrowseSubscriptionError> {
        let event = tokio::select! {
            biased;
            () = self.cancellation.cancelled() => {
                return Err(LibraryBrowseSubscriptionError::Cancelled);
            }
            event = async {
                let mut query = self.query.lock().await;
                let query = query
                    .as_mut()
                    .ok_or(LibraryBrowseSubscriptionError::Cancelled)?;
                Ok::<_, LibraryBrowseSubscriptionError>(query.next().await)
            } => event?,
        };
        let revision = event.revision().get();
        let projection = event.into_result()?;
        Ok((self.resolve)(projection, revision))
    }

    pub async fn cancel(&self) {
        self.cancellation.cancel();
        self.requests
            .lock()
            .expect("browse request mutex poisoned")
            .take();
        self.query.lock().await.take();
    }
}

impl<Projection, Row> Drop for LibraryBrowseSubscription<Projection, Row> {
    fn drop(&mut self) {
        self.cancellation.cancel();
        self.requests
            .get_mut()
            .expect("browse request mutex poisoned")
            .take();
    }
}
