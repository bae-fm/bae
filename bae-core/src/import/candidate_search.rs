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
//! Nothing more is read about a result until the person opens it: the cards
//! show at once, each catalog's albums apart. Opening a MusicBrainz result
//! reads its release's documents, and what they state its album is on Discogs
//! joins the cards by the one rule set an identify run's rows are joined by
//! (see [`crate::import::album_links`]).
//!
//! Pure. The driver that runs the lookups and publishes each landing is
//! [`crate::import::ImportServiceHandle::start_candidate_search`].

use crate::db::LibraryStatus;
use crate::import::album_links::{self, AlbumLink, AlbumLinks, GroupLinks};
use crate::import::release_group::{group_results, ReleaseGroup};
use crate::import::search::{MetadataResult, SearchQuery};
use crate::import::types::{Catalog, CatalogAvailability, MetadataRef, SourceAvailability};
use crate::signals::Failure;
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
    /// The source could not answer, or bae broke asking it.
    Failed(Failure),
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
    /// Each catalog's part of this search, one entry per
    /// [`Catalog::LOOKUP`] member, in that order. Neither is the main one: a
    /// surface iterates this and every method below folds over it.
    pub sources: Vec<(Catalog, SourceSearch)>,
    /// Every settled source's results, folded into album cards — re-derived
    /// whenever a source lands, so a card gains a second source's rows the
    /// moment that source answers.
    pub groups: Vec<ReleaseGroup>,
    /// One status per result across every settled source, each carrying its own
    /// release id.
    pub library_statuses: Vec<LibraryStatus>,
    /// The MusicBrainz results the person opened, in the order they did —
    /// what their documents state is already folded into `groups`.
    pub opened: Vec<OpenedResult>,
}

/// A MusicBrainz result the person opened, by release id.
#[derive(Debug, Clone, PartialEq)]
pub enum OpenedResult {
    /// Its documents are being read.
    Reading { release_id: String },
    /// What its documents state its album is on the other lookup catalog;
    /// `Unread` where they could not be had.
    Read { release_id: String, links: AlbumLinks },
}

impl OpenedResult {
    fn release_id(&self) -> &str {
        match self {
            OpenedResult::Reading { release_id } | OpenedResult::Read { release_id, .. } => release_id,
        }
    }
}

impl CandidateSearch {
    /// A search just submitted: every source this library asks is looking, and
    /// each source it does not ask says which reason it is rather than
    /// pretending to look.
    pub fn started(query: SearchQuery, sources: &[CatalogAvailability]) -> Self {
        Self {
            query,
            sources: sources
                .iter()
                .map(|entry| (entry.catalog, SourceSearch::starting(entry.state)))
                .collect(),
            groups: Vec::new(),
            library_statuses: Vec::new(),
            opened: Vec::new(),
        }
    }

    /// This source's part of the search, or `None` for a source the search was
    /// not started with.
    pub fn source(&self, source: Catalog) -> Option<&SourceSearch> {
        self.sources
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .map(|(_, state)| state)
    }

    /// Land one source's answer and re-derive the result area from every
    /// source that has answered.
    ///
    /// Only a source this search is still waiting on takes an answer. A source
    /// it never carried, and one that has stopped looking since the lookup went
    /// out — switched off while it was in flight — both belong to a dispatch
    /// this value has moved past, so their answers are dropped rather than
    /// reopening a part that is settled.
    pub fn record(
        &mut self,
        source: Catalog,
        outcome: Result<Vec<(MetadataResult, LibraryStatus)>, Failure>,
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
        if !matches!(entry.1, SourceSearch::Searching) {
            debug!(
                "dropped a {} search landing: it is no longer looking",
                source.as_str()
            );
            return;
        }
        entry.1 = settled;
        self.regroup();
    }

    /// Stop asking `source`: its part of this search closes, and whatever it
    /// had found leaves the result area. What the other sources found stays.
    ///
    /// A lookup already out for it still lands here and is dropped, because a
    /// part that is not looking takes no answer.
    pub fn switch_off(&mut self, source: Catalog) {
        let Some(entry) = self
            .sources
            .iter_mut()
            .find(|(candidate, _)| *candidate == source)
        else {
            return;
        };
        if matches!(entry.1, SourceSearch::Off) {
            return;
        }
        entry.1 = SourceSearch::Off;
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

    /// Open `release`: whether its documents are to be read now — a
    /// MusicBrainz result on the list that was not opened before, marked as
    /// being read. Only a MusicBrainz release's documents state its album on
    /// another catalog.
    pub fn open(&mut self, release: &MetadataRef) -> bool {
        let listed = self.landed().any(|(result, _)| {
            result.source == release.catalog && result.release_id == release.key
        });
        let opened = self
            .opened
            .iter()
            .any(|opened| opened.release_id() == release.key);
        if release.catalog != Catalog::MusicBrainz || !listed || opened {
            return false;
        }
        self.opened.push(OpenedResult::Reading {
            release_id: release.key.clone(),
        });
        true
    }

    /// Land what an opened MusicBrainz release's documents state its album
    /// is, and re-derive the result area with it. Answers what its album on
    /// the list was then read to be, to keep; nothing where the release is not
    /// a MusicBrainz result on the list.
    pub fn record_opened(
        &mut self,
        release: &MetadataRef,
        links: AlbumLinks,
    ) -> Vec<(String, Vec<AlbumLink>)> {
        if release.catalog != Catalog::MusicBrainz {
            return Vec::new();
        }
        let Some(group) = self
            .landed()
            .find(|(result, _)| {
                result.source == release.catalog && result.release_id == release.key
            })
            .map(|(result, _)| result.source_group_id.clone())
        else {
            return Vec::new();
        };
        let read = OpenedResult::Read {
            release_id: release.key.clone(),
            links,
        };
        match self
            .opened
            .iter_mut()
            .find(|opened| opened.release_id() == release.key)
        {
            Some(opened) => *opened = read,
            None => self.opened.push(read),
        }
        self.regroup();
        let landed: Vec<&MetadataResult> = self.landed().map(|(result, _)| result).collect();
        let groups: Vec<GroupLinks> = self
            .read_groups(&landed)
            .into_iter()
            .filter(|read| Some(&read.group) == group.as_ref())
            .collect();
        album_links::to_keep(&groups, &landed)
    }

    /// Every settled source's results, in source order.
    fn landed(&self) -> impl Iterator<Item = &(MetadataResult, LibraryStatus)> {
        self.sources.iter().flat_map(|(_, state)| state.results())
    }

    /// What each MusicBrainz album on `list` was read to be, from the opened
    /// results' documents.
    fn read_groups(&self, list: &[&MetadataResult]) -> Vec<GroupLinks> {
        album_links::read_groups(list, |release| {
            self.opened.iter().find_map(|opened| match opened {
                OpenedResult::Read { release_id, links } if release_id == release => {
                    Some(links.clone())
                }
                OpenedResult::Read { .. } | OpenedResult::Reading { .. } => None,
            })
        })
    }

    /// The sources with a lookup to run — every asked source of a just-started
    /// search, and the re-asked ones after a Retry.
    pub fn searching_sources(&self) -> Vec<Catalog> {
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
    fn sources_matching(&self, predicate: impl Fn(&SourceSearch) -> bool) -> Vec<Catalog> {
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
    pub fn failed_sources(&self) -> Vec<Catalog> {
        self.sources_matching(|state| matches!(state, SourceSearch::Failed(_)))
    }

    /// Re-fold every settled source's results, in source order — so a card two
    /// sources describe reads as the earlier source's with the later one's rows
    /// merged in — each MusicBrainz record carrying what its album was read to
    /// be from the results opened so far.
    fn regroup(&mut self) {
        let (mut results, statuses): (Vec<MetadataResult>, Vec<LibraryStatus>) =
            self.landed().cloned().unzip();
        let groups = self.read_groups(&results.iter().collect::<Vec<_>>());
        for result in &mut results {
            album_links::apply(result, &groups);
        }
        // Typed search: nothing was judged against the candidate's own text,
        // so the rows keep the pressing-year order alone.
        self.groups = group_results(crate::import::release_group::unranked(results), None);
        self.library_statuses = statuses;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library that asks every source.
    fn all_on() -> Vec<CatalogAvailability> {
        availability(&[
            (Catalog::MusicBrainz, SourceAvailability::On),
            (Catalog::Discogs, SourceAvailability::On),
        ])
    }

    /// A library with no Discogs key: it is listed, and it is not asked.
    fn discogs_unconfigured() -> Vec<CatalogAvailability> {
        availability(&[
            (Catalog::MusicBrainz, SourceAvailability::On),
            (Catalog::Discogs, SourceAvailability::NotConfigured),
        ])
    }

    fn availability(
        states: &[(Catalog, SourceAvailability)],
    ) -> Vec<CatalogAvailability> {
        states
            .iter()
            .map(|&(catalog, state)| CatalogAvailability { catalog, state })
            .collect()
    }

    fn query() -> SearchQuery {
        SearchQuery::General {
            artist: "Artist Name".to_string(),
            album: "Album Title".to_string(),
        }
    }

    fn result(source: Catalog, release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            source,
            release_id: release_id.to_string(),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1992),
            labels: Vec::new(),
            area: None,
            status: None,
            packaging: None,
            discogs_details: Vec::new(),
            barcodes: vec!["012345678905".to_string()],
            media: crate::pressing::StatedMedia::Undescribed,
            links: Vec::new(),
            cover_art: None,
            source_group_id: Some(group_id.to_string()),
            album_links: crate::import::album_links::AlbumLinks::NotAsked,
            source_tracks: None,
            document_failure: None,
            album_first_year: None,
            track_titles: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn answer(
        source: Catalog,
        release_id: &str,
        group_id: &str,
    ) -> Result<Vec<(MetadataResult, LibraryStatus)>, Failure> {
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
        search.record(Catalog::MusicBrainz, Err(crate::signals::LookupFailure::Network.into()));
        assert!(!search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::Failed);
        search.restart_failed();
        assert!(!search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::Searching);
        search.record(Catalog::MusicBrainz, Ok(Vec::new()));
        assert!(search.has_no_matches());
        assert_eq!(search.status(), SearchStatus::NoMatches);
    }

    /// One source failing heads the search with the failure, whatever the
    /// other found: the gap is what a person has to know about.
    #[test]
    fn a_failed_source_heads_the_search_even_beside_matches() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        assert_eq!(search.status(), SearchStatus::Searching);
        search.record(Catalog::Discogs, Err(crate::signals::LookupFailure::Timeout.into()));
        assert_eq!(search.status(), SearchStatus::Failed);
        search.restart_failed();
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.status(), SearchStatus::Found);
    }

    fn musicbrainz(release_id: &str) -> crate::import::MetadataRef {
        crate::import::MetadataRef::new(Catalog::MusicBrainz, release_id)
    }

    /// A typed search reads nothing up front: its cards show each catalog's
    /// album apart until the person opens a result, and then what that
    /// release's documents state joins them.
    #[test]
    fn a_typed_search_s_cards_join_after_a_result_is_opened_and_not_before() {
        let unpaired = |source: Catalog, release_id: &str, group_id: &str| {
            let mut found = result(source, release_id, group_id);
            found.barcodes = Vec::new();
            Ok(vec![(found, LibraryStatus::absent(release_id))])
        };
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            unpaired(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(
            Catalog::Discogs,
            unpaired(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.groups.len(), 2, "nothing joins before an open");

        assert!(
            !search.open(&crate::import::MetadataRef::new(Catalog::Discogs, "dg-1")),
            "a Discogs release's documents state no other catalog's album"
        );
        assert!(search.open(&musicbrainz("mb-1")));
        assert!(
            !search.open(&musicbrainz("mb-1")),
            "a result being read is not read again"
        );
        assert!(!search.open(&musicbrainz("mb-9")), "nor one off the list");
        assert_eq!(search.groups.len(), 2, "nor while its documents are read");

        let link = AlbumLink {
            album: crate::import::MetadataRef::new(Catalog::Discogs, "master-7"),
            stated: crate::import::album_links::AlbumStatement::Page,
        };
        let kept = search.record_opened(
            &musicbrainz("mb-1"),
            AlbumLinks::Read(vec![link.clone()]),
        );
        assert_eq!(search.groups.len(), 1, "the opened result joins the cards");
        assert_eq!(kept, vec![("group-x".to_string(), vec![link])]);
    }

    /// An opened result whose documents link nothing is read against what the
    /// landed list prints: one catalog number under one label joins the
    /// albums, and that is what the group is kept as. Before it is opened the
    /// same numbers join nothing.
    #[test]
    fn an_opened_result_no_document_links_is_joined_by_what_the_list_prints() {
        let numbered = |source: Catalog, release_id: &str, group_id: &str| {
            let mut found = result(source, release_id, group_id);
            found.barcodes = Vec::new();
            found.labels = vec![crate::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB 100"))];
            Ok(vec![(found, LibraryStatus::absent(release_id))])
        };
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            numbered(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(Catalog::Discogs, numbered(Catalog::Discogs, "dg-1", "master-7"));
        assert_eq!(search.groups.len(), 2);

        assert!(search.open(&musicbrainz("mb-1")));
        let kept = search.record_opened(&musicbrainz("mb-1"), AlbumLinks::Read(Vec::new()));
        let joined = AlbumLink {
            album: crate::import::MetadataRef::new(Catalog::Discogs, "master-7"),
            stated: crate::import::album_links::AlbumStatement::CatalogNumber {
                musicbrainz_release: "mb-1".to_string(),
                release: crate::import::MetadataRef::new(Catalog::Discogs, "dg-1"),
            },
        };
        assert_eq!(kept, vec![("group-x".to_string(), vec![joined])]);
        assert_eq!(search.groups.len(), 1);
    }

    #[test]
    fn a_started_search_is_looking_on_every_asked_source() {
        let search = CandidateSearch::started(query(), &all_on());
        assert_eq!(
            search.sources,
            vec![
                (Catalog::MusicBrainz, SourceSearch::Searching),
                (Catalog::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(
            search.searching_sources(),
            vec![Catalog::MusicBrainz, Catalog::Discogs]
        );
        assert!(!search.is_settled());
        assert!(search.groups.is_empty());
    }

    #[test]
    fn an_unconfigured_discogs_is_never_asked() {
        let mut search = CandidateSearch::started(query(), &discogs_unconfigured());
        assert_eq!(
            search.source(Catalog::Discogs),
            Some(&SourceSearch::NotConfigured)
        );
        assert_eq!(
            search.searching_sources(),
            vec![Catalog::MusicBrainz]
        );
        assert!(!search.is_settled(), "MusicBrainz is still looking");

        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
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
                (Catalog::MusicBrainz, SourceAvailability::Off),
                (Catalog::Discogs, SourceAvailability::On),
            ]),
        );

        assert_eq!(
            search.sources,
            vec![
                (Catalog::MusicBrainz, SourceSearch::Off),
                (Catalog::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(search.searching_sources(), vec![Catalog::Discogs]);
        assert!(!search.is_settled(), "Discogs is still looking");
    }

    /// A search asks the sources it was started with and no others. An answer
    /// from outside that set belongs to a dispatch this value has moved past,
    /// so it is dropped rather than reopening a settled search.
    #[test]
    fn an_answer_from_a_source_the_search_never_asked_is_dropped() {
        let mut search = CandidateSearch::started(
            query(),
            &availability(&[(Catalog::Discogs, SourceAvailability::On)]),
        );
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );

        assert!(search.source(Catalog::MusicBrainz).is_none());
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
                (Catalog::MusicBrainz, SourceAvailability::Off),
                (Catalog::Discogs, SourceAvailability::NotConfigured),
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
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        assert!(!search.is_settled());
        assert_eq!(
            search.source(Catalog::Discogs),
            Some(&SourceSearch::Searching)
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].pressings().count(), 1);
        assert_eq!(search.library_statuses.len(), 1);
    }

    /// Discogs landing merges into the card MusicBrainz already drew: same
    /// album, same barcode, so one card with one row on two sources.
    #[test]
    fn a_later_source_merges_into_the_groups_already_drawn() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert!(search.is_settled());
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].sources.len(), 2);
        assert_eq!(
            search.groups[0].sections[0].pressings[0]
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
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(Catalog::Discogs, Err(crate::signals::LookupFailure::Network.into()));
        assert!(search.is_settled());
        assert_eq!(search.failed_sources(), vec![Catalog::Discogs]);
        assert_eq!(search.groups.len(), 1);
    }

    /// Retry re-asks only the failed source, and keeps what the other found.
    #[test]
    fn retry_restarts_only_the_failed_sources() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(Catalog::Discogs, Err(crate::signals::LookupFailure::Timeout.into()));

        search.restart_failed();
        assert_eq!(search.searching_sources(), vec![Catalog::Discogs]);
        assert_eq!(
            search.source(Catalog::Discogs),
            Some(&SourceSearch::Searching)
        );
        assert!(matches!(
            search.source(Catalog::MusicBrainz),
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
        search.record(Catalog::MusicBrainz, Ok(Vec::new()));
        search.record(Catalog::Discogs, Ok(Vec::new()));
        assert!(search.is_settled());
        assert!(search.groups.is_empty());
        assert!(search.failed_sources().is_empty());
        assert!(search.has_no_matches());
    }

    /// Only a source still looking takes an answer. A second answer from a
    /// source that has already settled is a landing from a dispatch this value
    /// moved past, so it does not overwrite what is there.
    #[test]
    fn a_source_that_has_settled_takes_no_second_answer() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-1", "master-7"),
        );
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-2", "master-8"),
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].sections[0].pressings[0].lead().release_id, "dg-1");
    }

    /// A retry puts the failed source back to looking, and only then does its
    /// new answer land — replacing the failure rather than adding to it.
    #[test]
    fn a_retried_source_lands_its_new_answer() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(Catalog::Discogs, Err(crate::signals::LookupFailure::Timeout.into()));
        search.restart_failed();
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-2", "master-8"),
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].sections[0].pressings[0].lead().release_id, "dg-2");
    }

    /// Switching a source off mid-search closes its part and takes its results
    /// out of the list, leaving the other source's standing — and the answer
    /// its lookup was already out for lands nowhere.
    #[test]
    fn switching_a_source_off_drops_its_results_and_its_late_answer() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-1", "group-x"),
        );
        search.record(
            Catalog::Discogs,
            answer(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.groups[0].sources.len(), 2);

        search.switch_off(Catalog::MusicBrainz);
        assert_eq!(
            search.source(Catalog::MusicBrainz),
            Some(&SourceSearch::Off)
        );
        assert_eq!(search.groups.len(), 1);
        assert_eq!(
            search.groups[0]
                .sources
                .iter()
                .map(|carrier| carrier.source)
                .collect::<Vec<_>>(),
            vec![Catalog::Discogs]
        );
        assert_eq!(search.library_statuses.len(), 1);

        // The lookup that was in flight for it when the switch went off.
        search.record(
            Catalog::MusicBrainz,
            answer(Catalog::MusicBrainz, "mb-2", "group-y"),
        );
        assert_eq!(
            search.source(Catalog::MusicBrainz),
            Some(&SourceSearch::Off)
        );
        assert_eq!(search.groups.len(), 1);
    }

    /// A search whose only looking source is switched off is settled, and it
    /// never claims to have found nothing: nothing answered.
    #[test]
    fn switching_off_the_last_looking_source_settles_the_search() {
        let mut search = CandidateSearch::started(query(), &all_on());
        search.record(Catalog::Discogs, Ok(Vec::new()));
        assert!(!search.is_settled());
        search.switch_off(Catalog::MusicBrainz);
        assert!(search.is_settled());
        assert!(search.searching_sources().is_empty());
        assert_eq!(search.status(), SearchStatus::NoMatches);
    }
}
