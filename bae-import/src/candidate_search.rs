//! What one candidate's typed search has turned up so far.
//!
//! A person's search asks every configured provider, and the providers
//! answer at their own pace. So the run is not one awaited call with one
//! answer: each source stores its own part of it, and the pane draws whatever
//! has arrived. When MusicBrainz answers first, the pane shows its groups
//! while Discogs is still looking; when Discogs answers, bae merges its rows
//! into them; the pane names a provider that fails beside what the other
//! found, rather than going blank.
//!
//! bae reads nothing more about a result until the person picks it: the cards
//! show at once, each catalog's albums apart. Picking a MusicBrainz result
//! reads its release's documents, and bae joins the Discogs album they name
//! to the cards by the same rules that join an identify run's rows (see
//! [`bae_catalogs::album_links`]).
//!
//! bae stores the search as rows (`import_candidate_search` and the tables
//! beside it), and bae rewrites the list's cards each time a provider
//! answers.
//! `CandidateSearch` holds what a read of those rows returns: its cards are
//! the ones the rows state, and the read counts each part's releases and
//! decides where the search stands. The driver that runs the lookups is
//! [`crate::ImportServiceHandle::start_candidate_search`].

use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::SearchQuery;
use bae_catalogs::Failure;
use bae_identify::release_group::ReleaseGroup;

/// The run a stored search is on: what each catalog's answer names to land
/// on it. A new search for a key is on a new run, and no run is used twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SearchRun(i64);

impl SearchRun {
    pub(crate) fn of(number: i64) -> Self {
        Self(number)
    }

    pub(crate) fn number(self) -> i64 {
        self.0
    }
}

/// One provider's part of a candidate's manual search.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceSearch {
    /// Switched off: the person is not asking this source, so it was never
    /// asked. The switch says so; nothing else needs to.
    Off,
    /// The source needs a credential this library does not hold, so it was
    /// never asked. Saying so is not the same as saying it found nothing, and
    /// not the same as [`Self::Off`]: the person fixes one by supplying the
    /// credential and the other by switching the source back on.
    NotConfigured,
    Searching,
    /// The source answered with `found` releases, which reach a surface on
    /// the search's cards.
    Done {
        found: u32,
    },
    /// The source could not answer, or bae broke asking it.
    Failed(Failure),
}

/// Where a candidate's typed search stands as a whole: the one glyph a
/// surface heads it with. The search's read decides it from the parts' rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    /// A source is still looking.
    Searching,
    /// No source is looking and none failed, and a source named a release.
    /// Every search asks a source: one the library asks none for fails.
    Found,
    /// No source is looking and none failed, a source answered, and no
    /// source named a release. A source that was not asked is not an empty
    /// answer.
    NoMatches,
    /// No source is looking and at least one failed, whatever the others
    /// found.
    Failed,
}

/// A candidate's typed search as its rows read: the query, each source's
/// part of it, the album cards of every release a source returned, and where
/// the search stands as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSearch {
    pub query: SearchQuery,
    /// Each catalog's part of this search, one entry per
    /// [`LookupCatalog`], in that order. Neither is the main one: a
    /// surface iterates this.
    pub sources: Vec<(LookupCatalog, SourceSearch)>,
    /// Every settled source's results as album cards, the cards the search's
    /// own write stored: a card gains a second source's rows the moment that
    /// source answers.
    pub groups: Vec<ReleaseGroup>,
    pub status: SearchStatus,
}

#[cfg(test)]
impl CandidateSearch {
    /// This source's part of the search, or `None` for a source the search was
    /// not started with.
    pub(crate) fn source(&self, source: LookupCatalog) -> Option<&SourceSearch> {
        self.sources.iter().find(|(candidate, _)| *candidate == source).map(|(_, state)| state)
    }
}
