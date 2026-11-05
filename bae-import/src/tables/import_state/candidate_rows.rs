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
    One(&'a str),
}

impl<'a> CandidateRows<'a> {
    /// The clause keeping these candidates' rows by `column`, and what it
    /// binds; no clause for every candidate.
    pub(crate) fn filter(self, column: &str) -> CandidateFilter {
        match self {
            #[cfg(any(test, feature = "test-utils"))]
            Self::All => CandidateFilter { clause: String::new(), binds: CandidateBinds::None },
            Self::One(key) => CandidateFilter {
                clause: format!("WHERE {column} = :candidate"),
                binds: CandidateBinds::One(key.to_string()),
            },
        }
    }
}

/// A [`CandidateRows`]' clause, and the parameter it binds.
pub(crate) struct CandidateFilter {
    pub(crate) clause: String,
    binds: CandidateBinds,
}

enum CandidateBinds {
    #[cfg(any(test, feature = "test-utils"))]
    None,
    One(String),
}

impl CandidateFilter {
    /// What the clause binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match &self.binds {
            #[cfg(any(test, feature = "test-utils"))]
            CandidateBinds::None => Vec::new(),
            CandidateBinds::One(key) => vec![(":candidate", key as &dyn rusqlite::ToSql)],
        }
    }
}
