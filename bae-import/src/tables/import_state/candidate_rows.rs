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
/// the caller holds, or the one the scan stores at a key, whose content hash
/// the statement reads from the scan's row itself. A screen's read names a
/// candidate by its key, so that none of its statements binds a value that
/// another statement returned.
#[derive(Debug, Clone, Copy)]
pub(crate) enum OneCandidate<'a> {
    Hash(&'a str),
    Scanned(&'a crate::CandidateKey),
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

    /// An SQL expression for the candidate's content hash, which binds what
    /// [`Self::params`] returns.
    pub(crate) fn hash_sql(self) -> &'static str {
        match self {
            Self::Hash(_) => ":candidate",
            Self::Scanned(_) => SCANNED_HASH,
        }
    }

    fn binds(self) -> CandidateBinds {
        match self {
            Self::Hash(hash) => CandidateBinds::One(hash.to_string()),
            Self::Scanned(key) => CandidateBinds::AtScan(key.clone()),
        }
    }

    /// What [`Self::hash_sql`] binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match self {
            Self::Hash(hash) => vec![(":candidate", hash as &dyn rusqlite::ToSql)],
            Self::Scanned(key) => vec![(":key", *key as &dyn rusqlite::ToSql)],
        }
    }
}

/// Whether the `import_candidate_state` row joined as `state` is the current
/// state of the release the scan stores at the key a statement binds as
/// `:key`: the state of the content hash the scan holds, at the file-edit
/// revision the scan holds. Every read of a scanned release's current state
/// uses this condition.
pub(crate) fn current_at_scan_sql(state: &str) -> String {
    format!(
        "({state}.content_hash, {state}.edit_revision) = \
             (SELECT scan.content_hash, scan.file_edit_revision \
              FROM scan_candidate AS scan WHERE scan.path = :key)"
    )
}

/// The content hash of the release the scan stores at the key a statement
/// binds as `:key`.
pub(crate) const SCANNED_HASH: &str =
    "(SELECT scan.content_hash FROM scan_candidate AS scan WHERE scan.path = :key)";

/// A [`CandidateRows`]' clause, and the parameter it binds.
pub(crate) struct CandidateFilter {
    pub(crate) clause: String,
    binds: CandidateBinds,
}

enum CandidateBinds {
    #[cfg(any(test, feature = "test-utils"))]
    None,
    One(String),
    /// The key of the scanned release whose content hash and file-edit
    /// revision the state's content hash and edit revision must equal.
    AtScan(crate::CandidateKey),
}

impl CandidateFilter {
    /// Matches the state row of the content hash the scan holds for the
    /// release `key` only when its edit revision equals the file-edit
    /// revision the scan holds, and no row otherwise. For a statement reading
    /// `import_candidate_state AS state`.
    pub(crate) fn current_at_scan(key: &crate::CandidateKey) -> Self {
        Self {
            clause: format!("WHERE {}", current_at_scan_sql("state")),
            binds: CandidateBinds::AtScan(key.clone()),
        }
    }

    /// What the clause binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match &self.binds {
            #[cfg(any(test, feature = "test-utils"))]
            CandidateBinds::None => Vec::new(),
            CandidateBinds::One(key) => vec![(":candidate", key as &dyn rusqlite::ToSql)],
            CandidateBinds::AtScan(key) => vec![(":key", key as &dyn rusqlite::ToSql)],
        }
    }
}
