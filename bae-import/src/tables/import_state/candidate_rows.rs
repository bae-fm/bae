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

impl<'a> CandidateRows<'a> {
    /// The clause keeping these candidates' rows by `column`, and what it
    /// binds; no clause for every candidate.
    pub(crate) fn filter(self, column: &str) -> CandidateFilter {
        match self {
            #[cfg(any(test, feature = "test-utils"))]
            Self::All => CandidateFilter { clause: String::new(), binds: CandidateBinds::None },
            Self::One(one) => one.filter(column),
        }
    }
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

/// The `import_candidate_state` row joined as `state` that is the current
/// state of the release the scan stores at the key a statement binds as
/// `:key`: the state of the content hash the scan holds, at the file-edit
/// revision the scan holds. A statement reading a scanned release's current
/// state takes this as a FROM item, with [`CURRENT_AT_KEY`] in its WHERE.
/// The scan's row is a table of the statement, so coven narrows `state` by
/// it and reruns the statement only for the writes that reach the key's
/// state: a condition holding a subquery narrows nothing.
pub(crate) fn current_state_from_sql(state: &str) -> String {
    format!(
        "scan_candidate AS scanned \
         JOIN import_candidate_state AS {state} \
             ON {state}.content_hash = scanned.content_hash \
            AND {state}.edit_revision = scanned.file_edit_revision"
    )
}

/// The condition of [`current_state_from_sql`] that names the key.
pub(crate) const CURRENT_AT_KEY: &str = "scanned.path = :key";

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
