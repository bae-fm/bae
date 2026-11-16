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
    /// One candidate's content hash and the key of the scanned release whose
    /// file-edit revision the state's edit revision must equal.
    OneAtScan(String, crate::CandidateKey),
}

impl CandidateFilter {
    /// Matches the state row of the candidate `content_hash` only when its
    /// edit revision equals the file-edit revision the scan holds for the
    /// release `key`, and no row otherwise. For a statement reading
    /// `import_candidate_state AS state`.
    pub(crate) fn current_at_scan(content_hash: &str, key: &crate::CandidateKey) -> Self {
        Self {
            clause:
                "WHERE state.content_hash = :candidate \
                     AND state.edit_revision = (SELECT scan.file_edit_revision \
                                                FROM scan_candidate AS scan WHERE scan.path = :key)"
                    .to_string(),
            binds: CandidateBinds::OneAtScan(content_hash.to_string(), key.clone()),
        }
    }

    /// What the clause binds, as a statement's parameters.
    pub(crate) fn params(&self) -> Vec<(&'static str, &dyn rusqlite::ToSql)> {
        match &self.binds {
            #[cfg(any(test, feature = "test-utils"))]
            CandidateBinds::None => Vec::new(),
            CandidateBinds::One(key) => vec![(":candidate", key as &dyn rusqlite::ToSql)],
            CandidateBinds::OneAtScan(content_hash, key) => vec![
                (":candidate", content_hash as &dyn rusqlite::ToSql),
                (":key", key as &dyn rusqlite::ToSql),
            ],
        }
    }
}
