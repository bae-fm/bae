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

#[test]
fn a_failed_source_is_named_beside_the_other_source() {
    let search = search(
        done(vec![result(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
        failed(bae_catalogs::LookupFailure::Network),
    );
    assert!(search.is_settled());
    assert_eq!(search.failed_sources(), vec![LookupCatalog::Discogs]);
}

/// Both sources answering with nothing is a settled search with no releases
/// — the "no matches, try different terms" case, told apart from a failure
/// by there being no failed source.
#[test]
fn both_sources_answering_with_nothing_settles_empty() {
    let search = search(done(Vec::new()), done(Vec::new()));
    assert!(search.is_settled());
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
