// A candidate's typed search, stored: which answers land, and which are
// refused.

/// A library that asks every catalog it can — what these tests search from
/// unless they are about a catalog that is not asked.
fn every_source_on() -> Vec<CatalogAvailability> {
    bae_catalogs::catalog::LookupCatalog::ALL
        .into_iter()
        .map(|catalog| CatalogAvailability { catalog, state: SourceAvailability::On })
        .collect()
}

fn search_query() -> crate::catalog_model::search::SearchQuery {
    crate::catalog_model::search::SearchQuery::General {
        artist: "Artist Name".to_string(),
        album: "Album Title".to_string(),
    }
}

fn search_result(source: impl Into<Catalog>, release_id: &str) -> crate::catalog_model::search::MetadataResult {
    crate::catalog_model::search::MetadataResult {
        title: "Album Title".to_string(),
        artist: Some("Artist Name".to_string()),
        year: Some(1992),
        ..crate::catalog_model::search::MetadataResult::for_test(source, release_id, None)
    }
}

/// The search `key` holds, as its snapshot draws it.
async fn search_of(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
) -> Option<CandidateSearch> {
    runtime.snapshot(key).await.unwrap().and_then(|runtime| runtime.search)
}

/// Start a search of [`search_query`] for `key`, every catalog asked.
async fn started_search(
    runtime: &CandidateRuntime,
    key: &crate::import::CandidateKey,
) -> crate::import::candidate_search::SearchRun {
    runtime
        .start_search(key, &CandidateSearch::started(search_query(), &every_source_on()))
        .await
        .unwrap()
}

/// Land `outcome` as `source`'s answer on `run`; whether it landed.
async fn land(
    runtime: &CandidateRuntime,
    run: crate::import::candidate_search::SearchRun,
    source: bae_catalogs::catalog::LookupCatalog,
    outcome: Result<Vec<crate::catalog_model::search::MetadataResult>, crate::catalog_model::Failure>,
) -> bool {
    runtime.land_search(run, source, outcome).await.unwrap()
}

/// Both sources land on the same run, one after the other, and neither
/// landing loses the other's answer; each source's releases read back under
/// it, in the order it returned them.
#[tokio::test]
async fn two_landings_on_one_run_both_stand() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = started_search(&runtime, key).await;

    assert!(
        land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![
                search_result(Catalog::MusicBrainz, "mb-1"),
                search_result(Catalog::MusicBrainz, "mb-2"),
            ]),
        )
        .await
    );
    assert!(
        land(&runtime, run, LookupCatalog::Discogs, Ok(vec![search_result(Catalog::Discogs, "dg-1")]),)
            .await
    );

    let landed =
        search_of(&runtime, key).await.expect("the search is what is in flight for the key");
    assert_eq!(landed.query, search_query());
    let ids = |source| match landed.source(source) {
        Some(SourceSearch::Done { results }) => {
            results.iter().map(|result| result.release_id.clone()).collect::<Vec<_>>()
        }
        other => panic!("{source:?} stands at {other:?}"),
    };
    assert_eq!(ids(LookupCatalog::MusicBrainz), ["mb-1", "mb-2"]);
    assert_eq!(ids(LookupCatalog::Discogs), ["dg-1"]);
}

/// A new search replaces the key's search on a new run: the replaced run's
/// landing goes nowhere and leaves the new one untouched.
#[tokio::test]
async fn a_replaced_search_cannot_land() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let first = started_search(&runtime, key).await;
    let second = started_search(&runtime, key).await;
    assert_ne!(first, second);
    assert!(!runtime.search_run_is_current(key, first).await);
    assert!(runtime.search_run_is_current(key, second).await);

    assert!(
        !land(
            &runtime,
            first,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        )
        .await
    );
    assert_eq!(
        search_of(&runtime, key)
            .await
            .expect("the second search stands")
            .source(LookupCatalog::MusicBrainz),
        Some(&SourceSearch::Searching)
    );
}

/// A folder candidate's search is of the files the scan holds it with; a key
/// the scan holds no candidate at has none to search for. A library release
/// being identified again has no files, and searches all the same.
#[tokio::test]
async fn a_search_is_of_a_candidate_the_scan_holds_or_of_a_library_release() {
    let runtime = CandidateRuntime::for_test().await;
    let unscanned = &crate::import::CandidateKey::test("/watch/a/rel1");
    assert!(runtime
        .start_search(unscanned, &CandidateSearch::started(search_query(), &every_source_on()),)
        .await
        .is_err());
    assert!(search_of(&runtime, unscanned).await.is_none());

    let release = &crate::import::CandidateKey::reidentify("release-1");
    started_search(&runtime, release).await;
    assert!(search_of(&runtime, release).await.is_some());
}

/// Retry re-asks only the failed source, on the same run, keeping what the
/// other found. A search with nothing to re-ask changes nothing.
#[tokio::test]
async fn a_retry_re_asks_only_the_failed_sources() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = started_search(&runtime, key).await;
    assert!(
        land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        )
        .await
    );
    let unavailable: crate::catalog_model::Failure =
        crate::catalog_model::LookupFailure::Provider { status: Some(503) }.into();
    assert!(land(&runtime, run, LookupCatalog::Discogs, Err(unavailable.clone())).await);
    assert_eq!(
        search_of(&runtime, key).await.unwrap().source(LookupCatalog::Discogs),
        Some(&SourceSearch::Failed(unavailable)),
        "the failure reads back as it landed"
    );

    let (retried, query, sources) =
        runtime.retry_search(key).await.unwrap().expect("Discogs failed");
    assert_eq!(retried, run);
    assert_eq!(query, search_query());
    assert_eq!(sources, vec![LookupCatalog::Discogs]);
    let search = search_of(&runtime, key).await.expect("the retried search is in flight");
    assert!(matches!(search.source(LookupCatalog::MusicBrainz), Some(SourceSearch::Done { .. })));
    assert_eq!(search.source(LookupCatalog::Discogs), Some(&SourceSearch::Searching));

    assert!(
        land(
            &runtime,
            retried,
            LookupCatalog::Discogs,
            Ok(vec![search_result(Catalog::Discogs, "dg-1")]),
        )
        .await
    );
    assert!(
        runtime.retry_search(key).await.unwrap().is_none(),
        "a settled search has no source to re-ask"
    );
    assert!(
        runtime
            .retry_search(&crate::import::CandidateKey::test("/watch/a/rel2"))
            .await
            .unwrap()
            .is_none(),
        "a key with no search has none either"
    );
}

/// A retry asked while another source is still searching re-asks only the
/// failed one: the answer already out for the other still lands.
#[tokio::test]
async fn a_retry_leaves_a_source_still_searching_to_its_answer() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = started_search(&runtime, key).await;
    let broke: crate::catalog_model::Failure = crate::catalog_model::InternalFailure {
        detail: "reading the answer: unexpected end of input".to_string(),
    }
    .into();
    assert!(land(&runtime, run, LookupCatalog::Discogs, Err(broke.clone())).await);
    assert_eq!(
        search_of(&runtime, key).await.unwrap().source(LookupCatalog::Discogs),
        Some(&SourceSearch::Failed(broke)),
        "bae breaking reads back as its error"
    );

    let (_, _, sources) = runtime.retry_search(key).await.unwrap().unwrap();
    assert_eq!(sources, vec![LookupCatalog::Discogs]);
    assert!(
        land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        )
        .await,
        "MusicBrainz's answer was out before the retry and lands after it"
    );
}

/// A part that settled takes no second answer, and a catalog the search
/// never asked takes none at all.
#[tokio::test]
async fn only_a_part_still_searching_takes_an_answer() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = runtime
        .start_search(
            key,
            &CandidateSearch::started(
                search_query(),
                &[CatalogAvailability { catalog: LookupCatalog::Discogs, state: SourceAvailability::On }],
            ),
        )
        .await
        .unwrap();

    assert!(
        land(&runtime, run, LookupCatalog::Discogs, Ok(vec![search_result(Catalog::Discogs, "dg-1")]),)
            .await
    );
    assert!(
        !land(&runtime, run, LookupCatalog::Discogs, Ok(vec![search_result(Catalog::Discogs, "dg-2")]),)
            .await,
        "a part that answered takes no second answer"
    );
    assert!(
        !land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        )
        .await,
        "the search never asked MusicBrainz"
    );

    let search = search_of(&runtime, key).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), None);
    assert_eq!(search.groups.len(), 1);
    assert_eq!(search.groups[0].sections[0].pressings[0].lead().release_id, "dg-1");
}

/// Switching a source off closes its part of every search, wherever the
/// person is looking, and takes what it returned out of the cards; the other
/// source's part and the run stand.
#[tokio::test]
async fn switching_a_source_off_closes_its_part_of_every_search() {
    let runtime = CandidateRuntime::for_test().await;
    let answered = &crate::import::CandidateKey::test("/watch/a/rel1");
    let searching = &crate::import::CandidateKey::reidentify("release-1");
    scanned_key(&runtime, answered).await;
    let answered_run = started_search(&runtime, answered).await;
    let searching_run = started_search(&runtime, searching).await;
    for source in bae_catalogs::catalog::LookupCatalog::ALL {
        let release = format!("{}-1", source.as_str());
        assert!(
            land(&runtime, answered_run, source, Ok(vec![search_result(source, &release)]),).await
        );
    }
    assert_eq!(search_of(&runtime, answered).await.unwrap().groups.len(), 2);

    runtime.switch_source_off(LookupCatalog::MusicBrainz).await.unwrap();

    let search = search_of(&runtime, answered).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), Some(&SourceSearch::Off));
    assert!(matches!(search.source(LookupCatalog::Discogs), Some(SourceSearch::Done { .. })));
    assert_eq!(search.groups.len(), 1, "MusicBrainz's card is gone");
    let search = search_of(&runtime, searching).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), Some(&SourceSearch::Off));
    assert_eq!(
        search.source(LookupCatalog::Discogs),
        Some(&SourceSearch::Searching),
        "the source still being asked is untouched"
    );
    assert!(
        runtime.search_run_is_current(searching, searching_run).await,
        "the run stands, so the other source's answer still lands"
    );
}

/// The answer the dropped source already had out lands on a part that is no
/// longer searching, so it goes nowhere and the search stays closed on it.
#[tokio::test]
async fn a_landing_for_a_switched_off_source_goes_nowhere() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = started_search(&runtime, key).await;

    runtime.switch_source_off(LookupCatalog::MusicBrainz).await.unwrap();
    assert!(
        !land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(Catalog::MusicBrainz, "mb-1")]),
        )
        .await
    );

    let search =
        search_of(&runtime, key).await.expect("the search is what is in flight for the key");
    assert_eq!(
        search.source(LookupCatalog::MusicBrainz),
        Some(&SourceSearch::Off),
        "a part that is not searching takes no answer"
    );
    assert!(search.groups.is_empty());
}

/// A pick of a MusicBrainz result reads what its documents state its album
/// is, and that joins the cards in every read of the search after it. A pick
/// of a release the search's list does not hold, a partner no catalog
/// returned to it included, changes nothing.
#[tokio::test]
async fn a_pick_joins_the_search_s_cards_from_then_on() {
    let runtime = CandidateRuntime::for_test().await;
    let key = &crate::import::CandidateKey::test("/watch/a/rel1");
    scanned_key(&runtime, key).await;
    let run = started_search(&runtime, key).await;
    let grouped =
        |source: Catalog, release_id: &str, group: &str| crate::catalog_model::search::MetadataResult {
            source_group_id: Some(group.to_string()),
            ..search_result(source, release_id)
        };
    assert!(
        land(
            &runtime,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![grouped(Catalog::MusicBrainz, "mb-1", "group-x")]),
        )
        .await
    );
    assert!(
        land(
            &runtime,
            run,
            LookupCatalog::Discogs,
            Ok(vec![grouped(Catalog::Discogs, "dg-1", "master-7")]),
        )
        .await
    );
    assert_eq!(search_of(&runtime, key).await.unwrap().groups.len(), 2);
    let master = crate::catalog_model::records::MetadataRef::new(Catalog::Discogs, "master-7");
    let links = crate::catalog_model::album_links::AlbumLinks::Read(vec![master.clone()]);

    let off_the_list = runtime
        .land_picked(
            key,
            &crate::catalog_model::records::MetadataRef::new(Catalog::MusicBrainz, "mb-partner"),
            links.clone(),
        )
        .await
        .unwrap();
    assert!(off_the_list.is_empty());
    assert_eq!(search_of(&runtime, key).await.unwrap().groups.len(), 2);

    let kept = runtime
        .land_picked(
            key,
            &crate::catalog_model::records::MetadataRef::new(Catalog::MusicBrainz, "mb-1"),
            links.clone(),
        )
        .await
        .unwrap();
    assert_eq!(kept, vec![("group-x".to_string(), vec![master])]);
    let search = search_of(&runtime, key).await.unwrap();
    assert_eq!(search.groups.len(), 1, "the picked result joins the cards");
    assert_eq!(
        search.picked,
        vec![crate::import::candidate_search::PickedResult {
            release_id: "mb-1".to_string(),
            links,
        }]
    );
}
