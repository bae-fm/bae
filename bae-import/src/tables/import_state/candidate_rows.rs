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
