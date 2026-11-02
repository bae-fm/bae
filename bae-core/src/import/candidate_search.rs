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
//! (see [`crate::catalog_model::album_links`]).
//!
//! The search is stored as rows (`import_candidate_search` and the tables
//! beside it), each landing a write; this is the value a read of them makes,
//! its cards derived from the parts that have landed. The driver that runs
//! the lookups is [`crate::import::ImportServiceHandle::start_candidate_search`].

use crate::catalog_model::album_links::{self, AlbumLinks, GroupLinks};
use crate::catalog_model::records::MetadataRef;
use crate::catalog_model::search::{MetadataResult, SearchQuery};
use crate::catalog_model::Failure;
use crate::config::{CatalogAvailability, SourceAvailability};
use crate::import::release_group::{group_results, ReleaseGroup};
use bae_catalogs::catalog::{Catalog, LookupCatalog};

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
    /// Every settled source's results, folded into album cards as the
    /// search is read, so a card gains a second source's rows the moment that
    /// source answers.
    pub groups: Vec<ReleaseGroup>,
    /// The MusicBrainz results on the list a pick read — the release picked,
    /// or a partner it claimed — in list order. What their documents state is
    /// already folded into `groups`.
    pub picked: Vec<PickedResult>,
}

/// A MusicBrainz result a pick read: what its documents state its album is on
/// the other lookup catalog, `Unread` where they could not be had.
#[derive(Debug, Clone, PartialEq)]
pub struct PickedResult {
    pub release_id: String,
    pub links: AlbumLinks,
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
    /// and the results a pick read, its cards folded from them.
    pub(crate) fn of(
        query: SearchQuery,
        sources: Vec<(LookupCatalog, SourceSearch)>,
        picked: Vec<PickedResult>,
    ) -> Self {
        let mut search = Self { query, sources, groups: Vec::new(), picked };
        search.regroup();
        search
    }

    /// This source's part of the search, or `None` for a source the search was
    /// not started with.
    pub fn source(&self, source: LookupCatalog) -> Option<&SourceSearch> {
        self.sources.iter().find(|(candidate, _)| *candidate == source).map(|(_, state)| state)
    }

    /// Land what a picked MusicBrainz release's documents state its album
    /// is, and re-derive the result area with it. Answers what its album on
    /// the list was then read to be, to keep; `None` where the release is not
    /// a MusicBrainz result on the list, which no pick of this search's reads.
    /// Only a MusicBrainz release's documents state its album on another
    /// catalog, so a pick claims nothing about any other release; and a
    /// partner a pick claims that the list does not hold is no result of this
    /// search.
    pub(crate) fn record_picked(
        &mut self,
        release: &MetadataRef,
        links: AlbumLinks,
    ) -> Option<Vec<(String, Vec<MetadataRef>)>> {
        if release.catalog != Catalog::MusicBrainz {
            return None;
        }
        let group = self
            .landed()
            .find(|result| result.source == release.catalog && result.release_id == release.key)
            .map(|result| result.source_group_id.clone())?;
        let read = PickedResult { release_id: release.key.clone(), links };
        match self.picked.iter_mut().find(|picked| picked.release_id == release.key) {
            Some(picked) => *picked = read,
            None => self.picked.push(read),
        }
        self.regroup();
        let landed: Vec<&MetadataResult> = self.landed().collect();
        let groups: Vec<GroupLinks> = self
            .read_groups(&landed)
            .into_iter()
            .filter(|read| Some(&read.group) == group.as_ref())
            .collect();
        Some(album_links::to_keep(&groups, &landed))
    }

    /// Every settled source's results, in source order.
    fn landed(&self) -> impl Iterator<Item = &MetadataResult> {
        self.sources.iter().flat_map(|(_, state)| state.results())
    }

    /// What each MusicBrainz album on `list` was read to be, from the picked
    /// results' documents.
    fn read_groups(&self, list: &[&MetadataResult]) -> Vec<GroupLinks> {
        album_links::read_groups(list, |release| {
            self.picked
                .iter()
                .find(|picked| picked.release_id == release)
                .map(|picked| picked.links.clone())
        })
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
            && self.groups.is_empty()
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

    /// Re-fold every settled source's results, in source order — so a card two
    /// sources describe reads as the earlier source's with the later one's rows
    /// merged in — each MusicBrainz record carrying what its album was read to
    /// be from the results picked so far.
    fn regroup(&mut self) {
        let mut results: Vec<MetadataResult> = self.landed().cloned().collect();
        let groups = self.read_groups(&results.iter().collect::<Vec<_>>());
        for result in &mut results {
            album_links::apply(result, &groups);
        }
        // Typed search: nothing was judged against the candidate's own text,
        // so the rows keep the pressing-year order alone.
        self.groups = group_results(crate::import::release_group::unranked(results), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A library that asks every source.
    fn all_on() -> Vec<CatalogAvailability> {
        availability(&[
            (LookupCatalog::MusicBrainz, SourceAvailability::On),
            (LookupCatalog::Discogs, SourceAvailability::On),
        ])
    }

    /// A library with no Discogs key: it is listed, and it is not asked.
    fn discogs_unconfigured() -> Vec<CatalogAvailability> {
        availability(&[
            (LookupCatalog::MusicBrainz, SourceAvailability::On),
            (LookupCatalog::Discogs, SourceAvailability::NotConfigured),
        ])
    }

    fn availability(states: &[(LookupCatalog, SourceAvailability)]) -> Vec<CatalogAvailability> {
        states.iter().map(|&(catalog, state)| CatalogAvailability { catalog, state }).collect()
    }

    fn query() -> SearchQuery {
        SearchQuery::General { artist: "Artist Name".to_string(), album: "Album Title".to_string() }
    }

    fn result(source: impl Into<Catalog>, release_id: &str, group_id: &str) -> MetadataResult {
        MetadataResult {
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1992),
            barcodes: vec!["012345678905".to_string()],
            ..MetadataResult::for_test(source, release_id, Some(group_id))
        }
    }

    /// A source that answered with `results`.
    fn done(results: Vec<MetadataResult>) -> SourceSearch {
        SourceSearch::Done { results }
    }

    fn failed(failure: crate::catalog_model::LookupFailure) -> SourceSearch {
        SourceSearch::Failed(failure.into())
    }

    /// The search of [`query`] with MusicBrainz's and Discogs's parts as given.
    fn search(musicbrainz: SourceSearch, discogs: SourceSearch) -> CandidateSearch {
        CandidateSearch::of(
            query(),
            vec![(LookupCatalog::MusicBrainz, musicbrainz), (LookupCatalog::Discogs, discogs)],
            Vec::new(),
        )
    }

    #[test]
    fn no_matches_requires_a_completed_successful_lookup() {
        let started = CandidateSearch::started(query(), &discogs_unconfigured());
        assert!(!started.has_no_matches());
        assert_eq!(started.status(), SearchStatus::Searching);

        let failing = search(
            failed(crate::catalog_model::LookupFailure::Network),
            SourceSearch::NotConfigured,
        );
        assert!(!failing.has_no_matches());
        assert_eq!(failing.status(), SearchStatus::Failed);

        let empty = search(done(Vec::new()), SourceSearch::NotConfigured);
        assert!(empty.has_no_matches());
        assert_eq!(empty.status(), SearchStatus::NoMatches);
    }

    /// One source failing heads the search with the failure, whatever the
    /// other found: the gap is what a person has to know about.
    #[test]
    fn a_failed_source_heads_the_search_even_beside_matches() {
        let found = vec![result(Catalog::MusicBrainz, "mb-1", "group-x")];
        assert_eq!(
            search(done(found.clone()), SourceSearch::Searching).status(),
            SearchStatus::Searching
        );
        assert_eq!(
            search(done(found.clone()), failed(crate::catalog_model::LookupFailure::Timeout))
                .status(),
            SearchStatus::Failed
        );
        assert_eq!(
            search(done(found), done(vec![result(Catalog::Discogs, "dg-1", "master-7")])).status(),
            SearchStatus::Found
        );
    }

    fn musicbrainz(release_id: &str) -> crate::catalog_model::records::MetadataRef {
        crate::catalog_model::records::MetadataRef::new(Catalog::MusicBrainz, release_id)
    }

    /// A typed search reads nothing up front: its cards show each catalog's
    /// album apart until the person picks a result, and then what that
    /// release's documents state joins them.
    #[test]
    fn a_typed_search_s_cards_join_after_a_result_is_picked_and_not_before() {
        let unpaired = |source: Catalog, release_id: &str, group_id: &str| {
            let mut found = result(source, release_id, group_id);
            found.barcodes = Vec::new();
            done(vec![found])
        };
        let mut search = search(
            unpaired(Catalog::MusicBrainz, "mb-1", "group-x"),
            unpaired(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.groups.len(), 2, "nothing joins before a pick");

        let link = crate::catalog_model::records::MetadataRef::new(Catalog::Discogs, "master-7");
        assert_eq!(
            search.record_picked(
                &crate::catalog_model::records::MetadataRef::new(Catalog::Discogs, "dg-1"),
                AlbumLinks::Read(vec![link.clone()]),
            ),
            None,
            "a Discogs release's documents state no other catalog's album"
        );
        assert_eq!(
            search.record_picked(&musicbrainz("mb-9"), AlbumLinks::Read(vec![link.clone()])),
            None,
            "nor is a release off the list a result of this search"
        );
        assert_eq!(search.groups.len(), 2);

        let kept = search.record_picked(&musicbrainz("mb-1"), AlbumLinks::Read(vec![link.clone()]));
        assert_eq!(search.groups.len(), 1, "the picked result joins the cards");
        assert_eq!(kept, Some(vec![("group-x".to_string(), vec![link])]));
    }

    /// A picked result whose documents link nothing is read against what the
    /// landed list prints: one catalog number under one label joins the
    /// albums, and that is what the group is kept as. Before it is picked the
    /// same numbers join nothing.
    #[test]
    fn a_picked_result_no_document_links_is_joined_by_what_the_list_prints() {
        let numbered = |source: Catalog, release_id: &str, group_id: &str| {
            let mut found = result(source, release_id, group_id);
            found.barcodes = Vec::new();
            found.labels =
                vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB 100"))];
            done(vec![found])
        };
        let mut search = search(
            numbered(Catalog::MusicBrainz, "mb-1", "group-x"),
            numbered(Catalog::Discogs, "dg-1", "master-7"),
        );
        assert_eq!(search.groups.len(), 2);

        let kept = search.record_picked(&musicbrainz("mb-1"), AlbumLinks::Read(Vec::new()));
        assert_eq!(
            kept,
            Some(vec![(
                "group-x".to_string(),
                vec![crate::catalog_model::records::MetadataRef::new(Catalog::Discogs, "master-7")]
            )])
        );
        assert_eq!(search.groups.len(), 1);
    }

    /// The results a pick read join the cards as the search is read, as they
    /// did when the pick landed.
    #[test]
    fn a_search_read_with_its_picked_results_joins_their_cards() {
        let unpaired = |source: Catalog, release_id: &str, group_id: &str| {
            let mut found = result(source, release_id, group_id);
            found.barcodes = Vec::new();
            done(vec![found])
        };
        let read = CandidateSearch::of(
            query(),
            vec![
                (LookupCatalog::MusicBrainz, unpaired(Catalog::MusicBrainz, "mb-1", "group-x")),
                (LookupCatalog::Discogs, unpaired(Catalog::Discogs, "dg-1", "master-7")),
            ],
            vec![PickedResult {
                release_id: "mb-1".to_string(),
                links: AlbumLinks::Read(vec![crate::catalog_model::records::MetadataRef::new(
                    Catalog::Discogs,
                    "master-7",
                )]),
            }],
        );
        assert_eq!(read.groups.len(), 1);
    }

    #[test]
    fn a_started_search_is_looking_on_every_asked_source() {
        let search = CandidateSearch::started(query(), &all_on());
        assert_eq!(
            search.sources,
            vec![
                (LookupCatalog::MusicBrainz, SourceSearch::Searching),
                (LookupCatalog::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(
            search.searching_sources(),
            vec![LookupCatalog::MusicBrainz, LookupCatalog::Discogs]
        );
        assert!(!search.is_settled());
        assert!(search.groups.is_empty());
    }

    #[test]
    fn an_unconfigured_discogs_is_never_asked() {
        let started = CandidateSearch::started(query(), &discogs_unconfigured());
        assert_eq!(started.source(LookupCatalog::Discogs), Some(&SourceSearch::NotConfigured));
        assert_eq!(started.searching_sources(), vec![LookupCatalog::MusicBrainz]);
        assert!(!started.is_settled(), "MusicBrainz is still looking");

        let answered = search(
            done(vec![result(Catalog::MusicBrainz, "mb-1", "group-x")]),
            SourceSearch::NotConfigured,
        );
        assert!(answered.is_settled());
        assert!(answered.searching_sources().is_empty());
    }

    /// A source the person switched off is listed and not asked, and says so
    /// as its own state: "off" is a different fact from "no credential", and
    /// each is fixed a different way.
    #[test]
    fn a_source_switched_off_is_listed_and_not_asked() {
        let search = CandidateSearch::started(
            query(),
            &availability(&[
                (LookupCatalog::MusicBrainz, SourceAvailability::Off),
                (LookupCatalog::Discogs, SourceAvailability::On),
            ]),
        );

        assert_eq!(
            search.sources,
            vec![
                (LookupCatalog::MusicBrainz, SourceSearch::Off),
                (LookupCatalog::Discogs, SourceSearch::Searching),
            ]
        );
        assert_eq!(search.searching_sources(), vec![LookupCatalog::Discogs]);
        assert!(!search.is_settled(), "Discogs is still looking");
    }

    /// Nothing is asked, so nothing is still looking: the search is settled on
    /// arrival, and it never claims to have found nothing — no source answered.
    #[test]
    fn a_search_with_no_source_to_ask_is_settled_and_found_nothing_it_looked_for() {
        let search = CandidateSearch::started(
            query(),
            &availability(&[
                (LookupCatalog::MusicBrainz, SourceAvailability::Off),
                (LookupCatalog::Discogs, SourceAvailability::NotConfigured),
            ]),
        );

        assert!(search.searching_sources().is_empty());
        assert!(search.is_settled());
        assert!(!search.has_no_matches(), "no source answered, so nothing answered with nothing");
    }

    /// The first source to land draws its groups while the other is still
    /// looking — the whole point of keeping the sources apart.
    #[test]
    fn the_first_source_to_land_draws_while_the_other_looks() {
        let search = search(
            done(vec![result(Catalog::MusicBrainz, "mb-1", "group-x")]),
            SourceSearch::Searching,
        );
        assert!(!search.is_settled());
        assert_eq!(search.groups.len(), 1);
        assert_eq!(search.groups[0].pressings().count(), 1);
    }

    /// Discogs landing merges into the card MusicBrainz already drew: same
    /// album, same barcode, so one card with one row on two sources.
    #[test]
    fn a_later_source_merges_into_the_groups_already_drawn() {
        let search = search(
            done(vec![result(Catalog::MusicBrainz, "mb-1", "group-x")]),
            done(vec![result(Catalog::Discogs, "dg-1", "master-7")]),
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
    }

    #[test]
    fn a_failed_source_keeps_the_other_source_s_groups() {
        let search = search(
            done(vec![result(Catalog::MusicBrainz, "mb-1", "group-x")]),
            failed(crate::catalog_model::LookupFailure::Network),
        );
        assert!(search.is_settled());
        assert_eq!(search.failed_sources(), vec![LookupCatalog::Discogs]);
        assert_eq!(search.groups.len(), 1);
    }

    /// Both sources answering with nothing is a settled search with no groups
    /// — the "no matches, try different terms" case, told apart from a failure
    /// by there being no failed source.
    #[test]
    fn both_sources_answering_with_nothing_settles_empty() {
        let search = search(done(Vec::new()), done(Vec::new()));
        assert!(search.is_settled());
        assert!(search.groups.is_empty());
        assert!(search.failed_sources().is_empty());
        assert!(search.has_no_matches());
    }

    /// A search whose only looking source was switched off is settled, and it
    /// never claims to have found nothing: nothing answered.
    #[test]
    fn a_search_whose_last_looking_source_is_off_is_settled() {
        let search = search(SourceSearch::Off, done(Vec::new()));
        assert!(search.is_settled());
        assert!(search.searching_sources().is_empty());
        assert_eq!(search.status(), SearchStatus::NoMatches);
    }
}
