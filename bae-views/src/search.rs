//! Library-search policy: what counts as a searchable query, and how many
//! results a search returns. Both are domain decisions; they live here once so no
//! surface (the desktop title bar, the mobile search fields, the MCP tool) decides
//! them differently — or, as the MCP tool did, not at all.
//!
//! Its tables and the triggers keeping them are `search.sql`.

/// The SQL creating this part's tables and triggers.
pub(super) const SQL: &str = include_str!("search.sql");

/// A non-blank library search query. [`parse`](Self::parse) is the single
/// definition of "what counts as a searchable query": it trims surrounding
/// whitespace and rejects an empty or all-whitespace string. A blank query is
/// therefore never turned into a pattern that matches — and returns — every
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

    /// The trimmed, non-blank query text, for the pattern the DB builds.
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

/// A live library search whose query changes in place: one reconfigurable
/// query for as long as the search field is open, instead of a new live query
/// per keystroke.
pub struct LiveLibrarySearch {
    query: bae_db::live_query::CancellableLiveQuery<
        Option<LibrarySearchQuery>,
        crate::LibrarySearchProjection,
    >,
    resolve: std::sync::Arc<
        dyn Fn(crate::LibrarySearchProjection) -> crate::album_detail::SearchResults + Send + Sync,
    >,
}

impl LiveLibrarySearch {
    pub fn new(
        query: bae_db::live_query::CancellableLiveQuery<
            Option<LibrarySearchQuery>,
            crate::LibrarySearchProjection,
        >,
        resolve: impl Fn(crate::LibrarySearchProjection) -> crate::album_detail::SearchResults
            + Send
            + Sync
            + 'static,
    ) -> Self {
        Self { query, resolve: std::sync::Arc::new(resolve) }
    }

    /// Search for `raw`, parsed as every surface parses a query: a blank one is
    /// no search. Returns the revision the answer will carry.
    pub fn set_query(&self, raw: &str) -> Result<u64, bae_db::live_query::LiveQueryError> {
        self.query
            .set(LibrarySearchQuery::parse(raw))
            .map_err(bae_db::live_query::LiveQueryError::from)
    }

    pub async fn next(&self) -> Result<LibrarySearchSnapshot, bae_db::live_query::LiveQueryError> {
        let event = self.query.next().await.map_err(bae_db::live_query::LiveQueryError::from)?;
        let request_revision = event.revision.0;
        let query = event.request.clone();
        let projection = event.result?;
        Ok(LibrarySearchSnapshot { query, results: (self.resolve)(projection), request_revision })
    }

    pub async fn cancel(&self) {
        self.query.close().await;
    }
}

/// How many results a library search returns. The one place this is decided —
/// surfaces no longer each pass their own limit into `search_library`.
pub const SEARCH_RESULT_LIMIT: usize = 50;

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
