//! Whether the releases an import pane offers are already in the library,
//! read through one live query whose checks change as the offers do.

use crate::db::{LibraryCheck, LibraryStatus};
use crate::live_query::CancellableLiveQuery;
use std::collections::BTreeSet;

/// One value the subscription delivered: a status for each release it was
/// asked about, and the revision of the request that asked.
#[derive(Debug, Clone)]
pub struct LibraryStatusSnapshot {
    pub statuses: Vec<LibraryStatus>,
    pub request_revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryStatusSubscriptionError {
    #[error("library status subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// The library membership of a set of offered releases. A new set of offers
/// is a new request on the same query, and one read per database change
/// answers every release in it.
pub struct LibraryStatusSubscription {
    query: CancellableLiveQuery<BTreeSet<LibraryCheck>, Vec<LibraryStatus>>,
}

impl LibraryStatusSubscription {
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<BTreeSet<LibraryCheck>, Vec<LibraryStatus>>,
    ) -> Self {
        Self {
            query: CancellableLiveQuery::new(query),
        }
    }

    /// Check `checks` from now on. Returns the revision the answer will
    /// carry; the same checks in any order are the standing request.
    pub fn set_checks(
        &self,
        checks: impl IntoIterator<Item = LibraryCheck>,
    ) -> Result<u64, LibraryStatusSubscriptionError> {
        self.query
            .set(checks.into_iter().collect())
            .map_err(|_| LibraryStatusSubscriptionError::Cancelled)
    }

    pub async fn next(&self) -> Result<LibraryStatusSnapshot, LibraryStatusSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(|_| LibraryStatusSubscriptionError::Cancelled)?;
        let request_revision = event.revision().get();
        Ok(LibraryStatusSnapshot {
            statuses: event.into_result()?,
            request_revision,
        })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}
