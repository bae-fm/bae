//! What one candidate's typed search has turned up so far.
//!
//! A person's search asks every configured provider, and the providers
//! answer at their own pace. So the run is not one awaited call with one
//! answer: each source lands its own part of it, and the pane draws whatever
//! has landed. MusicBrainz answering first shows its groups while Discogs is
//! still looking; Discogs answering merges its rows into them; a provider that
//! fails is named beside what the other found rather than blanking the pane.
//!
//! Nothing more is read about a result until the person picks it: the cards
//! show at once, each catalog's albums apart. Picking a MusicBrainz result
//! reads its release's documents, and what they state its album is on Discogs
//! joins the cards by the one rule set an identify run's rows are joined by
//! (see [`bae_catalogs::album_links`]).
//!
//! The search is stored as rows (`import_candidate_search` and the tables
//! beside it), each landing a write that lays the list out again; this is
//! the value a read of them makes, its cards the ones the rows state. The driver that runs
//! the lookups is [`crate::ImportServiceHandle::start_candidate_search`].

use bae_catalogs::catalog::LookupCatalog;
use bae_catalogs::search::{MetadataResult, SearchQuery};
use bae_catalogs::Failure;
use bae_identify::release_group::ReleaseGroup;
use bae_preferences::{CatalogAvailability, SourceAvailability};

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
    /// not the same as [`Self::Off`] — one is fixed by supplying the
    /// credential, the other by switching the source back on.
    NotConfigured,
    Searching,
    Done {
        results: Vec<MetadataResult>,
    },
    /// The source could not answer, or bae broke asking it.
    Failed(Failure),
}

impl SourceSearch {
    /// The part a source starts with, given whether this library asks it.
    pub(crate) fn starting(state: SourceAvailability) -> Self {
        match state {
            SourceAvailability::On => SourceSearch::Searching,
            SourceAvailability::Off => SourceSearch::Off,
            SourceAvailability::NotConfigured => SourceSearch::NotConfigured,
        }
    }

    fn is_settled(&self) -> bool {
        match self {
            SourceSearch::Off
            | SourceSearch::NotConfigured
            | SourceSearch::Done { .. }
            | SourceSearch::Failed(_) => true,
            SourceSearch::Searching => false,
        }
    }

    fn results(&self) -> &[MetadataResult] {
        match self {
            SourceSearch::Done { results } => results,
            SourceSearch::Off
            | SourceSearch::NotConfigured
            | SourceSearch::Searching
            | SourceSearch::Failed(_) => &[],
        }
    }
}

/// Where a candidate's typed search stands as a whole — the one glyph a
/// surface heads it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchStatus {
    /// A source is still looking.
    Searching,
    /// Every source has landed and none failed; at least one named a release.
    Found,
    /// Every source has landed, none failed, and a source that answered named
    /// no release. An unavailable provider is not an empty answer.
    NoMatches,
    /// Every source has landed and at least one failed, whatever the others
    /// found.
    Failed,
}

/// A candidate's typed search: the query, each source's part of it, and the
/// result area derived from every part that has landed.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSearch {
    pub query: SearchQuery,
    /// Each catalog's part of this search, one entry per
    /// [`LookupCatalog`], in that order. Neither is the main one: a
    /// surface iterates this and every method below folds over it.
    pub sources: Vec<(LookupCatalog, SourceSearch)>,
    /// Every settled source's results as album cards, the cards the search's
    /// own write stored: a card gains a second source's rows the moment that
    /// source answers.
    pub groups: Vec<ReleaseGroup>,
}

impl CandidateSearch {
    /// A search just submitted: every source this library asks is looking, and
    /// each source it does not ask says which reason it is rather than
    /// pretending to look.
    pub fn started(query: SearchQuery, sources: &[CatalogAvailability]) -> Self {
        Self::of(
            query,
            sources
                .iter()
                .map(|entry| (entry.catalog, SourceSearch::starting(entry.state)))
                .collect(),
            Vec::new(),
        )
    }

    /// The search that asked `query`, with each source's part as it stands
    /// and the cards its answers laid out.
    pub(crate) fn of(
        query: SearchQuery,
        sources: Vec<(LookupCatalog, SourceSearch)>,
        groups: Vec<ReleaseGroup>,
    ) -> Self {
        Self { query, sources, groups }
    }

    /// This source's part of the search, or `None` for a source the search was
    /// not started with.
    pub fn source(&self, source: LookupCatalog) -> Option<&SourceSearch> {
        self.sources.iter().find(|(candidate, _)| *candidate == source).map(|(_, state)| state)
    }

    /// The MusicBrainz release group of `release`, the album its pick reads
    /// the documents of; `None` where `release` is no MusicBrainz result on the
    /// list, or names no group, which no pick of this search reads.
    pub(crate) fn pickable_group(
        &self,
        release: &bae_catalogs::records::LookupRef,
    ) -> Option<String> {
        if release.catalog != LookupCatalog::MusicBrainz {
            return None;
        }
        self.landed()
            .find(|result| result.release() == *release)
            .and_then(|result| result.source_group_id.clone())
    }

    /// Every settled source's results, in source order.
    fn landed(&self) -> impl Iterator<Item = &MetadataResult> {
        self.sources.iter().flat_map(|(_, state)| state.results())
    }

    /// The sources with a lookup to run — every asked source of a just-started
    /// search, and the re-asked ones after a Retry.
    pub fn searching_sources(&self) -> Vec<LookupCatalog> {
        self.sources_matching(|state| matches!(state, SourceSearch::Searching))
    }

    /// Whether every source has landed — nothing is still looking.
    fn is_settled(&self) -> bool {
        self.sources.iter().all(|(_, state)| state.is_settled())
    }

    /// A completed, successful lookup found no releases. A source that was not
    /// asked, or that failed, is not an empty answer.
    fn has_no_matches(&self) -> bool {
        self.is_settled()
            && self.landed().next().is_none()
            && self.failed_sources().is_empty()
            && self.sources.iter().any(|(_, state)| matches!(state, SourceSearch::Done { .. }))
    }

    /// The sources whose part satisfies `predicate`, in source order.
    fn sources_matching(&self, predicate: impl Fn(&SourceSearch) -> bool) -> Vec<LookupCatalog> {
        self.sources
            .iter()
            .filter(|(_, state)| predicate(state))
            .map(|(source, _)| *source)
            .collect()
    }

    /// Where the search stands as a whole.
    pub fn status(&self) -> SearchStatus {
        if !self.is_settled() {
            SearchStatus::Searching
        } else if !self.failed_sources().is_empty() {
            SearchStatus::Failed
        } else if self.has_no_matches() {
            SearchStatus::NoMatches
        } else {
            SearchStatus::Found
        }
    }

    /// The sources that failed, for the lines that name them and the Retry
    /// that re-asks them.
    pub fn failed_sources(&self) -> Vec<LookupCatalog> {
        self.sources_matching(|state| matches!(state, SourceSearch::Failed(_)))
    }
}

#[cfg(test)]
#[path = "candidate_search_tests.rs"]
mod tests;
