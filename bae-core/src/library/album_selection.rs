//! The album grid's multi-selection, read through one live query whose ids
//! change as the selection does.

use crate::live_query::CancellableLiveQuery;
use std::collections::BTreeSet;

/// One value the subscription delivered: the ids it read, and those of them
/// the library no longer shows, by id.
#[derive(Debug, Clone)]
pub struct AlbumSelectionSnapshot {
    pub requested: BTreeSet<String>,
    pub gone: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AlbumSelectionSubscriptionError {
    #[error("album selection subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The live query ending is this subscription's cancellation.
impl From<crate::live_query::LiveQueryCancelled> for AlbumSelectionSubscriptionError {
    fn from(_: crate::live_query::LiveQueryCancelled) -> Self {
        Self::Cancelled
    }
}

/// Which selected albums are gone from the library, wherever in the grid
/// they sit — what leaves the selection. A new selection is a new request on
/// the same query, and one read per database change answers every selected
/// album.
pub struct AlbumSelectionSubscription {
    query: CancellableLiveQuery<BTreeSet<String>, Vec<String>>,
}

impl AlbumSelectionSubscription {
    pub(crate) fn new(query: CancellableLiveQuery<BTreeSet<String>, Vec<String>>) -> Self {
        Self { query }
    }

    /// Read `album_ids` from now on.
    pub fn set_albums(
        &self,
        album_ids: impl IntoIterator<Item = String>,
    ) -> Result<(), AlbumSelectionSubscriptionError> {
        self.query
            .set(album_ids.into_iter().collect())
            .map(|_| ())
            .map_err(AlbumSelectionSubscriptionError::from)
    }

    pub async fn next(&self) -> Result<AlbumSelectionSnapshot, AlbumSelectionSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(AlbumSelectionSubscriptionError::from)?;
        Ok(AlbumSelectionSnapshot {
            requested: event.request.clone(),
            gone: event.result?,
        })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
