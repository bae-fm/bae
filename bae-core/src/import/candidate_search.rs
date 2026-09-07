//! What one candidate's typed search has turned up so far.
//!
//! A person's search asks every configured provider, and the providers
//! answer at their own pace. So the run is not one awaited call with one
//! answer: it is a value each source lands its own part of, and the pane draws
//! whatever has landed. MusicBrainz answering first shows its groups while
//! Discogs is still looking; Discogs answering merges its rows into them; a
//! provider that fails is named beside what the other found rather than
//! blanking the pane.
//!
//! Pure. The driver that runs the lookups and publishes each landing is
//! [`crate::import::ImportServiceHandle::start_candidate_search`].

use crate::db::LibraryStatus;
use crate::import::release_group::{group_results, ReleaseGroup};
use crate::import::search::{MetadataResult, SearchQuery};
use crate::import::types::{MetadataSource, MetadataSourceAvailability, SourceAvailability};
use crate::signals::LookupFailure;
use tracing::debug;

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
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed(LookupFailure),
}

impl SourceSearch {
    /// The part a source starts with, given whether this library asks it.
    fn starting(state: SourceAvailability) -> Self {
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

    fn results(&self) -> &[(MetadataResult, LibraryStatus)] {
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
    /// Each source's part of this search, one entry per
    /// [`MetadataSource`], in [`MetadataSource::ALL`] order. No source is the
    /// main one: a surface iterates this and every method below folds over it.
    pub sources: Vec<(MetadataSource, SourceSearch)>,
    /// Every settled source's results, folded into album cards — re-derived
    /// whenever a source lands, so a card gains a second source's rows the
    /// moment that source answers.
    pub groups: Vec<ReleaseGroup>,
    /// One status per result across every settled source, each carrying its own
    /// release id.
    pub library_statuses: Vec<LibraryStatus>,
}

impl CandidateSearch {
    /// A search just submitted: every source this library asks is looking, and
    /// each source it does not ask says which reason it is rather than
    /// pretending to look.
    pub fn started(query: SearchQuery, sources: &[MetadataSourceAvailability]) -> Self {
        Self {
            query,
            sources: sources
                .iter()
                .map(|entry| (entry.source, SourceSearch::starting(entry.state)))
                .collect(),
            groups: Vec::new(),
            library_statuses: Vec::new(),
        }
    }

    /// This source's part of the search, or `None` for a source the search was
    /// not started with.
    pub fn source(&self, source: MetadataSource) -> Option<&SourceSearch> {
        self.sources
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .map(|(_, state)| state)
    }

    /// Land one source's answer and re-derive the result area from every
    /// source that has answered.
    ///
    /// An answer from a source this search does not carry is dropped: the
    /// search names the sources it asked, and a landing outside them belongs
    /// to a dispatch this value has already moved past.
    pub fn record(
        &mut self,
        source: MetadataSource,
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure>,
    ) {
        let settled = match outcome {
            Ok(results) => SourceSearch::Done { results },
            Err(failure) => SourceSearch::Failed(failure),
        };
        let Some(entry) = self
            .sources
            .iter_mut()
            .find(|(candidate, _)| *candidate == source)
        else {
            debug!(
                "dropped a {} search landing: this search did not ask it",
                source.as_str()
            );
            return;
        };
        entry.1 = settled;
        self.regroup();
    }

    /// Put every failed source back to looking — what a Retry does before it
    /// re-dispatches. The results the other sources found stay on the value
    /// and keep drawing, and [`Self::searching_sources`] then names exactly
    /// the sources to re-ask.
    pub fn restart_failed(&mut self) {
        for (_, state) in self.sources.iter_mut() {
            if matches!(state, SourceSearch::Failed(_)) {
                *state = SourceSearch::Searching;
            }
        }
    }

    /// The sources with a lookup to run — every asked source of a just-started
    /// search, and the re-asked ones after a Retry.
    pub fn searching_sources(&self) -> Vec<MetadataSource> {
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
            && self.groups.is_empty()
            && self.failed_sources().is_empty()
            && self
                .sources
                .iter()
                .any(|(_, state)| matches!(state, SourceSearch::Done { .. }))
    }

    /// The sources whose part satisfies `predicate`, in source order.
    fn sources_matching(&self, predicate: impl Fn(&SourceSearch) -> bool) -> Vec<MetadataSource> {
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
    pub fn failed_sources(&self) -> Vec<MetadataSource> {
        self.sources_matching(|state| matches!(state, SourceSearch::Failed(_)))
    }

    /// Re-fold every settled source's results, in source order — so a card two
    /// sources describe reads as the earlier source's with the later one's rows
    /// merged in.
    fn regroup(&mut self) {
        let landed: Vec<(MetadataResult, LibraryStatus)> = self
            .sources
            .iter()
            .flat_map(|(_, state)| state.results())
            .cloned()
            .collect();
        let (results, statuses): (Vec<MetadataResult>, Vec<LibraryStatus>) =
            landed.into_iter().unzip();
        self.groups = group_results(results);
        self.library_statuses = statuses;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library that asks every source.
    fn all_on() -> Vec<MetadataSourceAvailability> {
        availability(&[
            (MetadataSource::MusicBrainz, SourceAvailability::On),
            (MetadataSource::Discogs, SourceAvailability::On),
        ])
    }

    /// A library with no Discogs key: it is listed, and it is not asked.
    fn discogs_unconfigured() -> Vec<MetadataSourceAvailability> {
        availability(&[
            (MetadataSource::MusicBrainz, SourceAvailability::On),
            (MetadataSource::Discogs, SourceAvailability::NotConfigured),
        ])
    }

    fn availability(
        states: &[(MetadataSource, SourceAvailability)],
    ) -> Vec<MetadataSourceAvailability> {
        states
            .iter()
            .map(|&(source, state)| MetadataSourceAvailability { source, state })
            .collect()
    }

    fn query() -> SearchQuery {
        SearchQuery::General {
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
        }
    }

    fn result(source: MetadataSource, release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            source,
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1992),
            format: None,
            label: None,
            catalog_number: None,
            country: None,
            barcode: Some("012345678905".to_string()),
            cover_art: None,
            source_group_id: Some(group_id.to_string()),
            source_tracks: None,
        }
    }

    fn answer(
        source: MetadataSource,
        release_id: &str,
        group_id: &str,
    ) -> Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure> {
        Ok(vec![(
            result(source, release_id, group_id),
            LibraryStatus::absent(release_id),
        )])
    }

    #[test]
    fn no_matches_requires_a_completed_successful_lookup() {
        let mut search = CandidateSearch::started(query(), &discogs_unconfigured());
        assert!(!search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::Searching);
        search.record(MetadataSource::MusicBrainz, Err(LookupFailure::Network));
        assert!(!search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::Failed);
        search.restart_failed();
        assert!(!search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::Searching);
        search.record(MetadataSource::MusicBrainz, Ok(Vec::new()));
        assert!(search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::NoMatches);
    }

    /// One source failing heads the search with the failure, whatever the
    /// other found: the gap is what a person has to know about.
    #[test]
    fn a_failed_source_heads_the_search_even_beside_matches() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        assert_eq!(search.status(), SearchStatus::Searching);
        search.record(MetadataSource::Discogs, Err(LookupFailure::Timeout));
        assert_eq!(search.status(), SearchStatus::Failed);
        search.restart_failed();
        search.record(
            MetadataSource::Discogs,
            answer(MetadataSource::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.status(), SearchStatus::Found);
    }

    #[test]
    fn a_started_search_is_looking_on_every_asked_source() {
        let search = CandidateSearch::started(query(), &all_on());
        assert_eq!(
            search.sources,
            vec![
                (MetadataSource::MusicBrainz, SourceSearch::Searching),
                (MetadataSource::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(
            search.searching_sources(),
            vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
        );
        assert!(!search.is_settled());
        assert!(search.groups.is_empty());
    }

    #[test]
    fn an_unconfigured_discogs_is_never_asked() {
        let mut search = CandidateSearch::started(query(), &discogs_unconfigured());
        assert_eq!(
            search.source(MetadataSource::Discogs),
            Some(&SourceSearch::NotConfigured)
        );
        assert_eq!(
            search.searching_sources(),
            vec![MetadataSource::MusicBrainz]
        );
        assert!(!search.is_settled(), "MusicBrainz is still looking");

        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        assert!(search.is_settled());
        assert!(search.searching_sources().is_empty());
    }

    /// A source the person switched off is listed and not asked, and says so
    /// as its own state: "off" is a different fact from "no credential", and
    /// each is fixed a different way.
    #[test]
    fn a_source_switched_off_is_listed_and_not_asked() {
        let search = CandidateSearch::started(
            query(),
            &availability(&[
                (MetadataSource::MusicBrainz, SourceAvailability::Off),
                (MetadataSource::Discogs, SourceAvailability::On),
            ]),
        );

        assert_eq!(
            search.sources,
            vec![
                (MetadataSource::MusicBrainz, SourceSearch::Off),
                (MetadataSource::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(search.searching_sources(), vec![MetadataSource::Discogs]);
        assert!(!search.is_settled(), "Discogs is still looking");
    }

    /// A search asks the sources it was started with and no others. An answer
    /// from outside that set belongs to a dispatch this value has moved past,
    /// so it is dropped rather than reopening a settled search.
    #[test]
    fn an_answer_from_a_source_the_search_never_asked_is_dropped() {
        let mut search = CandidateSearch::started(
            query(),
            &availability(&[(MetadataSource::Discogs, SourceAvailability::On)]),
        );
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );

        assert!(search.source(MetadataSource::MusicBrainz).is_none());
        assert!(search.groups.is_empty());
        assert!(!search.is_settled(), "the asked source is still looking");
    }

    /// Nothing is asked, so nothing is still looking: the search is settled on
    /// arrival, and it never claims to have found nothing — no source answered.
    #[test]
    fn a_search_with_no_source_to_ask_is_settled_and_found_nothing_it_looked_for() {
        let search = CandidateSearch::started(
            query(),
            &availability(&[
                (MetadataSource::MusicBrainz, SourceAvailability::Off),
                (MetadataSource::Discogs, SourceAvailability::NotConfigured),
            ]),
        );

        assert!(search.searching_sources().is_empty());
        assert!(search.is_settled());
        assert!(
            !search.has_no_matches(),
            "no source answered, so nothing answered with nothing"
        );
    }

    /// The first source to land draws its groups while the other is still
    /// looking — the whole point of keeping the sources apart.
    #[test]
    fn the_first_source_to_land_draws_while_the_other_looks() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        assert!(!search.is_settled());
        assert_eq!(
            search.source(MetadataSource::Discogs),
            Some(&SourceSearch::Searching)
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].pressings.len(), 1);
        assert_eq!(search.library_statuses.len(), 1);
    }

    /// Discogs landing merges into the card MusicBrainz already drew: same
    /// album, same barcode, so one card with one row on two sources.
    #[test]
    fn a_later_source_merges_into_the_groups_already_drawn() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(
            MetadataSource::Discogs,
            answer(MetadataSource::Discogs, "dg-1", "master-7"),
        );
        assert!(search.is_settled());
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].sources.len(), 2);
        assert_eq!(
            search.groups[0].pressings[0]
                .releases
                .iter()
                .map(|release| release.release_id.as_str())
                .collect::<Vec<_>>(),
            vec!["mb-1", "dg-1"]
        );
        assert_eq!(search.library_statuses.len(), 2);
    }

    #[test]
    fn a_failed_source_keeps_the_other_source_s_groups() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(MetadataSource::Discogs, Err(LookupFailure::Network));
        assert!(search.is_settled());
        assert_eq!(search.failed_sources(), vec![MetadataSource::Discogs]);
        assert_eq!(search.groups.len(), 1);
    }

    /// Retry re-asks only the failed source, and keeps what the other found.
    #[test]
    fn retry_restarts_only_the_failed_sources() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(MetadataSource::Discogs, Err(LookupFailure::Timeout));

        search.restart_failed();
        assert_eq!(search.searching_sources(), vec![MetadataSource::Discogs]);
        assert_eq!(
            search.source(MetadataSource::Discogs),
            Some(&SourceSearch::Searching)
        );
        assert!(matches!(
            search.source(MetadataSource::MusicBrainz),
            Some(SourceSearch::Done { .. })
        ));
        assert_eq!(search.groups.len(), 1, "the MusicBrainz card still draws");
        assert!(search.failed_sources().is_empty());
    }

    /// Both sources answering with nothing is a settled search with no groups
    /// — the "no matches, try different terms" case, told apart from a failure
    /// by there being no failed source.
    #[test]
    fn both_sources_answering_with_nothing_settles_empty() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(MetadataSource::MusicBrainz, Ok(Vec::new()));
        search.record(MetadataSource::Discogs, Ok(Vec::new()));
        assert!(search.is_settled());
        assert!(search.groups.is_empty());
        assert!(search.failed_sources().is_empty());
        assert!(search.has_no_matches());
    }

    /// A second answer from the same source replaces the first: a retry's
    /// results are the source's answer, not an addition to a stale one.
    #[test]
    fn a_second_answer_from_one_source_replaces_the_first() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            MetadataSource::Discogs,
            answer(MetadataSource::Discogs, "dg-1", "master-7"),
        );
        search.record(
            MetadataSource::Discogs,
            answer(MetadataSource::Discogs, "dg-2", "master-8"),
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].pressings[0].lead().release_id, "dg-2");
    }
}
