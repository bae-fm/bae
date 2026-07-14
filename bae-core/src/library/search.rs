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
