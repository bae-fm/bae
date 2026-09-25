//! The album grid's multi-selection, read through one live query whose ids
//! change as the selection does.

use crate::album_detail::AlbumSummary;
use crate::db::AlbumSelectionProjection;
use crate::live_query::CancellableLiveQuery;
use std::collections::BTreeSet;
use std::sync::Arc;

/// One value the subscription delivered: the ids it read and the summary of
/// each that is still in the library. A requested id with no summary names
/// an album that is gone.
#[derive(Debug, Clone)]
pub struct AlbumSelectionSnapshot {
    pub requested: BTreeSet<String>,
    pub albums: Vec<AlbumSummary>,
}

#[derive(Debug, thiserror::Error)]
pub enum AlbumSelectionSubscriptionError {
    #[error("album selection subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The selected albums' summaries — what the grid's bulk actions act on —
/// and whether each still exists. A new selection is a new request on the
/// same query, and one read per database change answers every selected album.
pub struct AlbumSelectionSubscription {
    query: CancellableLiveQuery<BTreeSet<String>, AlbumSelectionProjection>,
    resolve: Arc<dyn Fn(AlbumSelectionProjection) -> Vec<AlbumSummary> + Send + Sync>,
}

impl AlbumSelectionSubscription {
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<BTreeSet<String>, AlbumSelectionProjection>,
        resolve: impl Fn(AlbumSelectionProjection) -> Vec<AlbumSummary> + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: CancellableLiveQuery::new(query),
            resolve: Arc::new(resolve),
        }
    }

    /// Read `album_ids` from now on.
    pub fn set_albums(
        &self,
        album_ids: impl IntoIterator<Item = String>,
    ) -> Result<(), AlbumSelectionSubscriptionError> {
        self.query
            .set(album_ids.into_iter().collect())
            .map(|_| ())
            .map_err(|_| AlbumSelectionSubscriptionError::Cancelled)
    }

    pub async fn next(&self) -> Result<AlbumSelectionSnapshot, AlbumSelectionSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(|_| AlbumSelectionSubscriptionError::Cancelled)?;
        let requested = event.request().clone();
        let projection = event.into_result()?;
        Ok(AlbumSelectionSnapshot {
            requested,
            albums: (self.resolve)(projection),
        })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
