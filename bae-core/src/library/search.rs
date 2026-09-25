//! Library-search policy: what counts as a searchable query, and how many
//! results a search returns. Both are domain decisions; they live here once so no
//! surface (the desktop title bar, the mobile search fields, the MCP tool) decides
//! them differently — or, as the MCP tool did, not at all.

/// A non-blank library search query. [`parse`](Self::parse) is the single
/// definition of "what counts as a searchable query": it trims surrounding
/// whitespace and rejects an empty or all-whitespace string. A blank query is
/// therefore never turned into a `LIKE '%%'` that matches — and returns — every
/// row; it is simply not a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySearchQuery(String);

impl LibrarySearchQuery {
    /// Trim `raw` and return the query, or `None` when nothing is left — the one
    /// place surfaces route a raw input through, instead of each applying their own
    /// trim/blank rule (or none).
    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self(trimmed.to_string()))
        }
    }

    /// The trimmed, non-blank query text, for the `LIKE` pattern the DB builds.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One value a live library search delivered: the query it read, the results
/// for it, and the revision of the request that asked for it.
#[derive(Debug, Clone)]
pub struct LibrarySearchSnapshot {
    pub query: Option<LibrarySearchQuery>,
    pub results: crate::album_detail::SearchResults,
    pub request_revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum LibrarySearchSubscriptionError {
    #[error("search subscription cancelled")]
    Cancelled,
    #[error(transparent)]
    Query(#[from] coven::CovenError),
}

/// A live library search whose query changes in place: one reconfigurable
/// query for as long as the search field is open, instead of a new live query
/// per keystroke.
pub struct LibrarySearchSubscription {
    query: crate::live_query::CancellableLiveQuery<
        Option<LibrarySearchQuery>,
        crate::db::LibrarySearchProjection,
    >,
    resolve: std::sync::Arc<
        dyn Fn(crate::db::LibrarySearchProjection) -> crate::album_detail::SearchResults
            + Send
            + Sync,
    >,
}

impl LibrarySearchSubscription {
    pub(crate) fn new(
        query: coven::ReconfigurableLiveQuery<
            Option<LibrarySearchQuery>,
            crate::db::LibrarySearchProjection,
        >,
        resolve: impl Fn(crate::db::LibrarySearchProjection) -> crate::album_detail::SearchResults
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self {
            query: crate::live_query::CancellableLiveQuery::new(query),
            resolve: std::sync::Arc::new(resolve),
        }
    }

    /// Search for `raw`, parsed as every surface parses a query: a blank one is
    /// no search. Returns the revision the answer will carry.
    pub fn set_query(&self, raw: &str) -> Result<u64, LibrarySearchSubscriptionError> {
        self.query
            .set(LibrarySearchQuery::parse(raw))
            .map_err(|_| LibrarySearchSubscriptionError::Cancelled)
    }

    pub async fn next(&self) -> Result<LibrarySearchSnapshot, LibrarySearchSubscriptionError> {
        let event = self
            .query
            .next()
            .await
            .map_err(|_| LibrarySearchSubscriptionError::Cancelled)?;
        let request_revision = event.revision().get();
        let query = event.request().clone();
        let projection = event.into_result()?;
        Ok(LibrarySearchSnapshot {
            query,
            results: (self.resolve)(projection),
            request_revision,
        })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}

/// How many results a library search returns. The one place this is decided —
/// surfaces no longer each pass their own limit into `search_library`.
pub const SEARCH_RESULT_LIMIT: usize = 50;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_and_whitespace_are_not_searches() {
        assert_eq!(LibrarySearchQuery::parse(""), None);
        assert_eq!(LibrarySearchQuery::parse("   "), None);
        assert_eq!(LibrarySearchQuery::parse("\t \n"), None);
    }

    #[test]
    fn a_query_is_trimmed() {
        let query = LibrarySearchQuery::parse("  Abbey Road  ").expect("non-blank");
        assert_eq!(query.as_str(), "Abbey Road");
    }

    #[test]
    fn interior_whitespace_is_preserved() {
        let query = LibrarySearchQuery::parse("dark side").expect("non-blank");
        assert_eq!(query.as_str(), "dark side");
    }
}
