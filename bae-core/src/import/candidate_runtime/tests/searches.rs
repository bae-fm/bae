// A candidate's typed search: which run's answers land, and which are refused.

/// A library that asks every catalog it can — what these tests search from
/// unless they are about a catalog that is not asked.
fn every_source_on() -> Vec<CatalogAvailability> {
    Catalog::LOOKUP
        .into_iter()
        .map(|catalog| CatalogAvailability {
            catalog,
            state: SourceAvailability::On,
        })
        .collect()
}

fn search_query() -> crate::import::search::SearchQuery {
    crate::import::search::SearchQuery::General {
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
    }
}

fn search_result(source: Catalog, release_id: &str) -> crate::import::search::MetadataResult {
    crate::import::search::MetadataResult {
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1992),
        ..crate::import::search::MetadataResult::for_test(source, release_id, None)
    }
}

/// The search `key` holds, as its snapshot draws it.
async fn search_of(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
) -> Option<CandidateSearch> {
    runtime
        .snapshot(key)
        .await
        .unwrap()
        .and_then(|runtime| runtime.search)
}

/// Both sources land on the same run, one after the other, and neither
/// landing loses the other's answer.
#[tokio::test]
async fn two_landings_on_one_run_both_stand() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    assert!(runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        Catalog::Discogs,
        Ok(vec![search_result(Catalog::Discogs, "dg-1")]),
    ));

    let landed = search_of(&runtime, key)
        .await
        .expect("the search is what is in flight for the key");
    assert!(matches!(
        landed.source(Catalog::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert!(matches!(
        landed.source(Catalog::Discogs),
        Some(SourceSearch::Done { .. })
    ));
}

/// A superseded run's landing goes nowhere and leaves the new run untouched.
#[tokio::test]
async fn a_superseded_run_cannot_land() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let first = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    let second = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    assert!(!runtime.search_run_is_current(key, first));
    assert!(runtime.search_run_is_current(key, second));

    assert!(!runtime.land_search(
        key,
        first,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(matches!(
        search_of(&runtime, key)
            .await
            .expect("the second run stands")
            .source(Catalog::MusicBrainz),
        Some(SourceSearch::Searching)
    ));
}

/// A cleared search is gone from the key, and the run it was on cannot be
/// started again by a landing that was still out.
#[tokio::test]
async fn a_cleared_search_cannot_land() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.clear_search(key);
    assert!(
        runtime.snapshot(key).await.unwrap().is_none(),
        "nothing else was running"
    );
    assert!(!runtime.search_run_is_current(key, run));
    assert!(!runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.snapshot(key).await.unwrap().is_none());
}

/// Retry re-asks only the failed source, on a new run, keeping what the other
/// found. A search with nothing to re-ask changes nothing.
#[tokio::test]
async fn a_retry_re_asks_only_the_failed_sources_on_a_new_run() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );
    assert!(runtime.land_search(
        key,
        run,
        Catalog::MusicBrainz,
        Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
    ));
    assert!(runtime.land_search(
        key,
        run,
        Catalog::Discogs,
        Err(crate::signals::LookupFailure::Network.into()),
    ));

    let (query, sources, retried) = runtime.retry_search(key).expect("Discogs failed");
    assert_eq!(query, search_query());
    assert_eq!(sources, vec![Catalog::Discogs]);
    assert!(runtime.search_run_is_current(key, retried));
    assert!(!runtime.search_run_is_current(key, run));
    let search = search_of(&runtime, key)
        .await
        .expect("the retried search is in flight");
    assert!(matches!(
        search.source(Catalog::MusicBrainz),
        Some(SourceSearch::Done { .. })
    ));
    assert_eq!(
        search.source(Catalog::Discogs),
        Some(&SourceSearch::Searching)
    );

    assert!(runtime.land_search(
        key,
        retried,
        Catalog::Discogs,
        Ok(vec![search_result(Catalog::Discogs, "dg-1")]),
    ));
    assert!(
        runtime.retry_search(key).is_none(),
        "a settled search has no source to re-ask"
    );
    assert!(
        runtime
            .retry_search(&crate::import::CandidateKey::test("/watch/a/rel2"))
            .is_none(),
        "a key with no search has none either"
    );
}

/// Switching a source off closes its part of every search running right now,
/// wherever the person is looking, and leaves its run standing.
#[tokio::test]
async fn switching_a_source_off_closes_its_part_of_every_live_search() {
    let runtime = CandidateRuntime::for_test().await;
    let searching = &crate::import::CandidateKey::test("/watch/a/rel1");
    let run = runtime.start_search(
        searching,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.switch_source_off(Catalog::MusicBrainz);

    let search = search_of(&runtime, searching)
        .await
        .expect("the search is still what is in flight for the key");
    assert_eq!(
        search.source(Catalog::MusicBrainz),
        Some(&SourceSearch::Off)
    );
    assert_eq!(
        search.source(Catalog::Discogs),
        Some(&SourceSearch::Searching),
        "the source still being asked is untouched"
    );
    assert!(
        runtime.search_run_is_current(searching, run),
        "the run stands, so the other source's lookup still lands"
    );
}

/// The lookup the dropped source already had out lands on a part that is no
/// longer looking, so it goes nowhere and the search stays closed on it.
#[tokio::test]
async fn a_landing_for_a_switched_off_source_goes_nowhere() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    let run = runtime.start_search(
        key,
        CandidateSearch::started(search_query(), &every_source_on()),
    );

    runtime.switch_source_off(Catalog::MusicBrainz);
    assert!(
        runtime.land_search(
            key,
            run,
            Catalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        ),
        "the run is current, so the landing reaches the search"
    );

    let search = search_of(&runtime, key)
        .await
        .expect("the search is what is in flight for the key");
    assert_eq!(
        search.source(Catalog::MusicBrainz),
        Some(&SourceSearch::Off),
        "a part that is not looking takes no answer"
    );
    assert!(search.groups.is_empty());
}
