//! What one candidate's typed search has turned up so far.
//!
//! A person's search asks the chosen providers, and the providers
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
use crate::import::types::MetadataSource;
use crate::signals::LookupFailure;

/// One provider's part of a candidate's manual search.
#[derive(Debug, Clone, PartialEq)]
pub enum SourceSearch {
    /// The search targets another provider.
    NotRequested,
    /// Discogs without a usable key: it was never asked, and saying so is not
    /// the same as saying it found nothing.
    NotConfigured,
    Searching,
    Done {
        results: Vec<(MetadataResult, LibraryStatus)>,
    },
    Failed(LookupFailure),
}

impl SourceSearch {
    fn is_settled(&self) -> bool {
        match self {
            SourceSearch::NotRequested
            | SourceSearch::NotConfigured
            | SourceSearch::Done { .. }
            | SourceSearch::Failed(_) => true,
            SourceSearch::Searching => false,
        }
    }

    fn results(&self) -> &[(MetadataResult, LibraryStatus)] {
        match self {
            SourceSearch::Done { results } => results,
            SourceSearch::NotRequested
            | SourceSearch::NotConfigured
            | SourceSearch::Searching
            | SourceSearch::Failed(_) => &[],
        }
    }
}

/// A candidate's typed search: the query, each source's part of it, and the
/// result area derived from every part that has landed.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSearch {
    pub query: SearchQuery,
    pub musicbrainz: SourceSearch,
    pub discogs: SourceSearch,
    /// Every settled source's results, folded into album cards — re-derived
    /// whenever a source lands, so a card gains its Discogs rows the moment
    /// Discogs answers.
    pub groups: Vec<ReleaseGroup>,
    /// One status per result across every settled source, each carrying its own
    /// release id.
    pub library_statuses: Vec<LibraryStatus>,
}

impl CandidateSearch {
    /// A search just submitted: every configured source is looking, and an
    /// unconfigured Discogs says so instead of pretending to look.
    pub fn started(query: SearchQuery, discogs_configured: bool) -> Self {
        Self {
            query,
            musicbrainz: SourceSearch::Searching,
            discogs: if discogs_configured {
                SourceSearch::Searching
            } else {
                SourceSearch::NotConfigured
            },
            groups: Vec::new(),
            library_statuses: Vec::new(),
        }
    }

    /// Search one provider while leaving the other unrequested.
    pub fn for_source(
        query: SearchQuery,
        source: MetadataSource,
        discogs_configured: bool,
    ) -> Self {
        let mut search = Self::started(query, discogs_configured);
        match source {
            MetadataSource::MusicBrainz => search.discogs = SourceSearch::NotRequested,
            MetadataSource::Discogs => search.musicbrainz = SourceSearch::NotRequested,
        }
        search
    }

    /// Land one source's answer and re-derive the result area from every
    /// source that has answered.
    pub fn record(
        &mut self,
        source: MetadataSource,
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure>,
    ) {
        let settled = match outcome {
            Ok(results) => SourceSearch::Done { results },
            Err(failure) => SourceSearch::Failed(failure),
        };
        match source {
            MetadataSource::MusicBrainz => self.musicbrainz = settled,
            MetadataSource::Discogs => self.discogs = settled,
        }
        self.regroup();
    }

    /// Put every failed source back to looking — what a Retry does before it
    /// re-dispatches. The results the other sources found stay on the value
    /// and keep drawing, and [`Self::searching_sources`] then names exactly
    /// the sources to re-ask.
    pub fn restart_failed(&mut self) {
        for state in [&mut self.musicbrainz, &mut self.discogs] {
            if matches!(state, SourceSearch::Failed(_)) {
                *state = SourceSearch::Searching;
            }
        }
    }

    /// The sources with a lookup to run — every configured source of a
    /// just-started search, and the re-asked ones after a Retry.
    pub fn searching_sources(&self) -> Vec<MetadataSource> {
        [
            (MetadataSource::MusicBrainz, &self.musicbrainz),
            (MetadataSource::Discogs, &self.discogs),
        ]
        .into_iter()
        .filter(|(_, state)| matches!(state, SourceSearch::Searching))
        .map(|(source, _)| source)
        .collect()
    }

    /// Whether every source has landed — nothing is still looking.
    pub fn is_settled(&self) -> bool {
        self.musicbrainz.is_settled() && self.discogs.is_settled()
    }

    /// A completed, successful lookup found no releases. An unavailable or
    /// failed provider is not an empty answer.
    pub fn has_no_matches(&self) -> bool {
        self.is_settled()
            && self.groups.is_empty()
            && self.failed_sources().is_empty()
            && [&self.musicbrainz, &self.discogs]
                .iter()
                .any(|state| matches!(state, SourceSearch::Done { .. }))
    }

    /// The sources that failed, for the lines that name them and the Retry
    /// that re-asks them.
    pub fn failed_sources(&self) -> Vec<MetadataSource> {
        [
            (MetadataSource::MusicBrainz, &self.musicbrainz),
            (MetadataSource::Discogs, &self.discogs),
        ]
        .into_iter()
        .filter(|(_, state)| matches!(state, SourceSearch::Failed(_)))
        .map(|(source, _)| source)
        .collect()
    }

    /// Re-fold every settled source's results. MusicBrainz first, so a card
    /// that both sources describe reads as MusicBrainz's with Discogs's rows
    /// merged in.
    fn regroup(&mut self) {
        let landed: Vec<(MetadataResult, LibraryStatus)> = self
            .musicbrainz
            .results()
            .iter()
            .chain(self.discogs.results())
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

    fn status(release_id: &str) -> LibraryStatus {
        LibraryStatus {
            release_id: release_id.to_string(),
            release_in_library: false,
            album_in_library: false,
            album_title: None,
            album_id: None,
        }
    }

    fn answer(
        source: MetadataSource,
        release_id: &str,
        group_id: &str,
    ) -> Result<Vec<(MetadataResult, LibraryStatus)>, LookupFailure> {
        Ok(vec![(
            result(source, release_id, group_id),
            status(release_id),
        )])
    }

    #[test]
    fn a_source_search_only_asks_the_requested_provider() {
        for source in [MetadataSource::MusicBrainz, MetadataSource::Discogs] {
            let mut search = CandidateSearch::for_source(query(), source, true);
            assert_eq!(search.searching_sources(), vec![source]);
            assert!(!search.is_settled());
            search.record(source, answer(source, "release-1", "group-1"));
            assert!(search.is_settled());
            assert_eq!(search.groups.len(), 1);
        }
    }

    #[test]
    fn an_unconfigured_requested_provider_is_not_replaced_by_another_source() {
        let search = CandidateSearch::for_source(query(), MetadataSource::Discogs, false);
        assert_eq!(search.musicbrainz, SourceSearch::NotRequested);
        assert_eq!(search.discogs, SourceSearch::NotConfigured);
        assert!(!search.has_no_matches());
        assert!(search.searching_sources().is_empty());
        assert!(search.is_settled());
    }

    #[test]
    fn no_matches_requires_a_completed_successful_lookup() {
        let mut search = CandidateSearch::for_source(query(), MetadataSource::MusicBrainz, true);
        assert!(!search.has_no_matches());
        search.record(MetadataSource::MusicBrainz, Err(LookupFailure::Network));
        assert!(!search.has_no_matches());
        search.restart_failed();
        assert!(!search.has_no_matches());
        search.record(MetadataSource::MusicBrainz, Ok(Vec::new()));
        assert!(search.has_no_matches());
    }

    #[test]
    fn a_started_search_is_looking_on_every_configured_source() {
        let search = CandidateSearch::started(query(), true);
        assert_eq!(search.musicbrainz, SourceSearch::Searching);
        assert_eq!(search.discogs, SourceSearch::Searching);
        assert_eq!(
            search.searching_sources(),
            vec![MetadataSource::MusicBrainz, MetadataSource::Discogs]
        );
        assert!(!search.is_settled());
        assert!(search.groups.is_empty());
    }

    #[test]
    fn an_unconfigured_discogs_is_never_asked() {
        let mut search = CandidateSearch::started(query(), false);
        assert_eq!(search.discogs, SourceSearch::NotConfigured);
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

    /// The first source to land draws its groups while the other is still
    /// looking — the whole point of keeping the sources apart.
    #[test]
    fn the_first_source_to_land_draws_while_the_other_looks() {
        let mut search = CandidateSearch::started(query(), true);
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        assert!(!search.is_settled());
        assert_eq!(search.discogs, SourceSearch::Searching);
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].pressings.len(), 1);
        assert_eq!(search.library_statuses.len(), 1);
    }

    /// Discogs landing merges into the card MusicBrainz already drew: same
    /// album, same barcode, so one card with one row on two sources.
    #[test]
    fn a_later_source_merges_into_the_groups_already_drawn() {
        let mut search = CandidateSearch::started(query(), true);
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
        let mut search = CandidateSearch::started(query(), true);
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
        let mut search = CandidateSearch::started(query(), true);
        search.record(
            MetadataSource::MusicBrainz,
            answer(MetadataSource::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(MetadataSource::Discogs, Err(LookupFailure::Timeout));

        search.restart_failed();
        assert_eq!(search.searching_sources(), vec![MetadataSource::Discogs]);
        assert_eq!(search.discogs, SourceSearch::Searching);
        assert!(matches!(search.musicbrainz, SourceSearch::Done { .. }));
        assert_eq!(search.groups.len(), 1, "the MusicBrainz card still draws");
        assert!(search.failed_sources().is_empty());
    }

    /// Both sources answering with nothing is a settled search with no groups
    /// — the "no matches, try different terms" case, told apart from a failure
    /// by there being no failed source.
    #[test]
    fn both_sources_answering_with_nothing_settles_empty() {
        let mut search = CandidateSearch::started(query(), true);
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
        let mut search = CandidateSearch::started(query(), true);
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
