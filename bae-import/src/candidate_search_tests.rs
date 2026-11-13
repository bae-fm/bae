use super::*;
use bae_catalogs::catalog::Catalog;

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

fn result(source: LookupCatalog, release_id: &str, group_id: &str) -> MetadataResult {
    MetadataResult {
        title: "Album Title".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        year: Some(1992),
        barcodes: vec!["012345678905".to_string()],
        ..MetadataResult::for_test(source, release_id, Some(group_id))
    }
}

/// A source that answered with `results`.
fn done(results: Vec<MetadataResult>) -> SourceSearch {
    SourceSearch::Done { results }
}

fn failed(failure: bae_catalogs::LookupFailure) -> SourceSearch {
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

    let failing = search(failed(bae_catalogs::LookupFailure::Network), SourceSearch::NotConfigured);
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
    let found = vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")];
    assert_eq!(
        search(done(found.clone()), SourceSearch::Searching).status(),
        SearchStatus::Searching
    );
    assert_eq!(
        search(done(found.clone()), failed(bae_catalogs::LookupFailure::Timeout)).status(),
        SearchStatus::Failed
    );
    assert_eq!(
        search(done(found), done(vec![result(LookupCatalog::Discogs, "dg-1", "master-7")]))
            .status(),
        SearchStatus::Found
    );
}

fn musicbrainz(release_id: &str) -> bae_catalogs::records::LookupRef {
    bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, release_id)
}

/// A typed search reads nothing up front: its cards show each catalog's
/// album apart until the person picks a result, and then what that
/// release's documents state joins them.
#[test]
fn a_typed_search_s_cards_join_after_a_result_is_picked_and_not_before() {
    let unpaired = |source: LookupCatalog, release_id: &str, group_id: &str| {
        let mut found = result(source, release_id, group_id);
        found.barcodes = Vec::new();
        done(vec![found])
    };
    let mut search = search(
        unpaired(LookupCatalog::MusicBrainz, "mb-1", "group-x"),
        unpaired(LookupCatalog::Discogs, "dg-1", "master-7"),
    );
    assert_eq!(search.groups.len(), 2, "nothing joins before a pick");

    let link = bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "master-7");
    assert_eq!(
        search.record_picked(
            &bae_catalogs::records::LookupRef::new(LookupCatalog::Discogs, "dg-1"),
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
    let numbered = |source: LookupCatalog, release_id: &str, group_id: &str| {
        let mut found = result(source, release_id, group_id);
        found.barcodes = Vec::new();
        found.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB 100"))];
        done(vec![found])
    };
    let mut search = search(
        numbered(LookupCatalog::MusicBrainz, "mb-1", "group-x"),
        numbered(LookupCatalog::Discogs, "dg-1", "master-7"),
    );
    assert_eq!(search.groups.len(), 2);

    let kept = search.record_picked(&musicbrainz("mb-1"), AlbumLinks::Read(Vec::new()));
    assert_eq!(
        kept,
        Some(vec![(
            "group-x".to_string(),
            vec![bae_catalogs::records::MetadataRef::new(Catalog::Discogs, "master-7")]
        )])
    );
    assert_eq!(search.groups.len(), 1);
}

/// The results a pick read join the cards as the search is read, as they
/// did when the pick landed.
#[test]
fn a_search_read_with_its_picked_results_joins_their_cards() {
    let unpaired = |source: LookupCatalog, release_id: &str, group_id: &str| {
        let mut found = result(source, release_id, group_id);
        found.barcodes = Vec::new();
        done(vec![found])
    };
    let read = CandidateSearch::of(
        query(),
        vec![
            (LookupCatalog::MusicBrainz, unpaired(LookupCatalog::MusicBrainz, "mb-1", "group-x")),
            (LookupCatalog::Discogs, unpaired(LookupCatalog::Discogs, "dg-1", "master-7")),
        ],
        vec![PickedResult {
            release_id: "mb-1".to_string(),
            links: AlbumLinks::Read(vec![bae_catalogs::records::MetadataRef::new(
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
        done(vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
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
        done(vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
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
        done(vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
        done(vec![result(LookupCatalog::Discogs, "dg-1", "master-7")]),
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
        done(vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
        failed(bae_catalogs::LookupFailure::Network),
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
