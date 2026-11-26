//! Which candidates a read takes the stored rows of.

/// Which candidates a loader reads the rows of, by content hash: every one's,
/// or one candidate's, each by its own statement. The one
/// candidate's statement names it by key, which its index reaches and which
/// coven reruns a live query for only when that candidate's rows change.
#[derive(Debug, Clone, Copy)]
pub(crate) enum CandidateRows<'a> {
    /// Every candidate, which only the tests that read back everything a
    /// write stored ask for.
    #[cfg(any(test, feature = "test-utils"))]
    All,
    One(OneCandidate<'a>),
}

/// The one candidate a read takes the rows of: the one whose content hash
/// the caller holds.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OneCandidate<'a> {
    Hash(&'a str),
}

impl OneCandidate<'_> {
    /// The clause keeping this candidate's rows by `column`, and what it
    /// binds.
    pub(crate) fn filter(self, column: &str) -> CandidateFilter {
        CandidateFilter {
            clause: format!("WHERE {column} = {}", self.hash_sql()),
            binds: self.binds(),
        }
    }

    /// An SQL expression for the candidate's content hash, which binds the
    /// parameter the filter's `params` bind.
    pub(crate) fn hash_sql(self) -> &'static str {
        match self {
            Self::Hash(_) => ":candidate",
        }
    }

    fn binds(self) -> CandidateBinds {
        match self {
            Self::Hash(hash) => CandidateBinds::One(hash.to_string()),
        }
    }
}

/// A [`CandidateRows`]' clause, and the parameter it binds.
pub(crate) struct CandidateFilter {
    pub(crate) clause: String,
    binds: CandidateBinds,
}

enum CandidateBinds {
    One(String),
}

impl CandidateFilter {
    /// What the clause binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match &self.binds {
            CandidateBinds::One(key) => vec![(":candidate", key as &dyn rusqlite::ToSql)],
        }
    }
}
