//! A person's typed search as its rows keep it: each catalog's answer
//! landing on the run it was asked for, a retry, a source switched off, and
//! a picked release's documents joining it.

use super::*;
use crate::candidate_search::{SearchStatus, SourceSearch};
use crate::tables::import_state::tests::{pane_candidate_key, stored_pane_candidate};
use bae_catalogs::catalog::Catalog;
use bae_catalogs::records::MetadataRef;
use bae_preferences::{CatalogAvailability, SourceAvailability};
use bae_store::test_fixtures::empty_db;

/// A library that asks every catalog it can — what these tests search from
/// unless they are about a catalog that is not asked.
fn every_source_on() -> Vec<CatalogAvailability> {
    LookupCatalog::ALL
        .into_iter()
        .map(|catalog| CatalogAvailability { catalog, state: SourceAvailability::On })
        .collect()
}

fn search_query() -> SearchQuery {
    SearchQuery::General { artist: "Artist Name".to_string(), album: "Album Title".to_string() }
}

pub(crate) fn search_result(source: LookupCatalog, release_id: &str) -> MetadataResult {
    MetadataResult {
        title: "Album Title".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        year: Some(1992),
        ..MetadataResult::for_test(source, release_id, None)
    }
}

/// Tables over a library whose scan holds one folder candidate, and its key.
pub(crate) async fn scanned() -> (ImportTables, CandidateKey, tempfile::TempDir) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    stored_pane_candidate(&db).await;
    (ImportTables::new(db), pane_candidate_key(), tmp)
}

/// The search `key` holds, as the candidate's runtime reads it.
pub(crate) async fn search_of(
    tables: &ImportTables,
    key: &CandidateKey,
) -> Option<CandidateSearch> {
    tables.load_candidate_runtime(key).await.unwrap().search
}

/// Whether `run` is the run `key`'s search is on.
async fn is_current(tables: &ImportTables, key: &CandidateKey, run: SearchRun) -> bool {
    let key = key.clone();
    let current = tables
        .store
        .read(move |sql| {
            Ok(sql
                .query_row(
                    "SELECT search_run FROM import_candidate_search WHERE candidate_key = ?",
                    [&key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?)
        })
        .await
        .unwrap();
    current == Some(run.number())
}

/// The releases `source` returned to the search `run`, in the order it
/// returned them.
async fn returned(
    tables: &ImportTables,
    run: SearchRun,
    source: LookupCatalog,
) -> Vec<MetadataResult> {
    every_returned(tables, run).await.into_iter().filter(|result| result.source == source).collect()
}

/// Every release the search `run` holds, under its catalog parts in their
/// order.
async fn every_returned(tables: &ImportTables, run: SearchRun) -> Vec<MetadataResult> {
    tables
        .store
        .read(move |sql| super::super::match_record::load_search_run_matches_on(&sql, run.number()))
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.result)
        .collect()
}

/// Start a search of [`search_query`] for `key` with each catalog as
/// `sources` says; the run, and the catalogs to ask.
async fn start(
    tables: &ImportTables,
    key: &CandidateKey,
    sources: &[CatalogAvailability],
) -> (SearchRun, Vec<LookupCatalog>) {
    tables.start_candidate_search(key, &search_query(), sources).await.unwrap()
}

/// Start a search of [`search_query`] for `key`, every catalog asked.
pub(crate) async fn started_search(tables: &ImportTables, key: &CandidateKey) -> SearchRun {
    start(tables, key, &every_source_on()).await.0
}

/// A library that lists MusicBrainz and Discogs as given.
fn availability(
    musicbrainz: SourceAvailability,
    discogs: SourceAvailability,
) -> Vec<CatalogAvailability> {
    vec![
        CatalogAvailability { catalog: LookupCatalog::MusicBrainz, state: musicbrainz },
        CatalogAvailability { catalog: LookupCatalog::Discogs, state: discogs },
    ]
}

/// Land `outcome` as `source`'s answer on `run`; whether it landed.
pub(crate) async fn land(
    tables: &ImportTables,
    run: SearchRun,
    source: LookupCatalog,
    outcome: Result<Vec<MetadataResult>, Failure>,
) -> bool {
    tables.land_candidate_search(run, source, outcome).await.unwrap()
}

/// Both sources land on the same run, one after the other, and neither
/// landing loses the other's answer; each source's releases read back under
/// it, in the order it returned them.
#[tokio::test]
async fn two_landings_on_one_run_both_stand() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;

    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![
                search_result(LookupCatalog::MusicBrainz, "mb-1"),
                search_result(LookupCatalog::MusicBrainz, "mb-2"),
            ]),
        )
        .await
    );
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![search_result(LookupCatalog::Discogs, "dg-1")]),
        )
        .await
    );

    let landed =
        search_of(&tables, key).await.expect("the search is what is in flight for the key");
    assert_eq!(landed.query, search_query());
    assert_eq!(
        landed.sources,
        [
            (LookupCatalog::MusicBrainz, SourceSearch::Done { found: 2 }),
            (LookupCatalog::Discogs, SourceSearch::Done { found: 1 }),
        ]
    );
    let ids = |results: Vec<MetadataResult>| {
        results.into_iter().map(|result| result.release_id).collect::<Vec<_>>()
    };
    assert_eq!(ids(returned(&tables, run, LookupCatalog::MusicBrainz).await), ["mb-1", "mb-2"]);
    assert_eq!(ids(returned(&tables, run, LookupCatalog::Discogs).await), ["dg-1"]);
}

/// A started search lists every catalog in the library's order, searching
/// where the library asks it, and asks only those: a catalog switched off and
/// one that needs a credential each say which they are.
#[tokio::test]
async fn a_started_search_asks_only_the_catalogs_the_library_asks() {
    use SourceAvailability::{NotConfigured, Off, On};
    let (tables, key, _tmp) = scanned().await;
    let key = &key;

    let (_, asked) = start(&tables, key, &availability(Off, On)).await;
    assert_eq!(asked, [LookupCatalog::Discogs]);
    let search = search_of(&tables, key).await.unwrap();
    assert_eq!(
        search.sources,
        [
            (LookupCatalog::MusicBrainz, SourceSearch::Off),
            (LookupCatalog::Discogs, SourceSearch::Searching),
        ]
    );
    assert_eq!(search.status, SearchStatus::Searching);

    let (_, asked) = start(&tables, key, &availability(On, NotConfigured)).await;
    assert_eq!(asked, [LookupCatalog::MusicBrainz]);
    assert_eq!(
        search_of(&tables, key).await.unwrap().sources,
        [
            (LookupCatalog::MusicBrainz, SourceSearch::Searching),
            (LookupCatalog::Discogs, SourceSearch::NotConfigured),
        ]
    );

    let (_, asked) = start(&tables, key, &availability(Off, NotConfigured)).await;
    assert!(asked.is_empty());
}

/// The search's read decides where it stands: searching while a part is,
/// failed when a part failed whatever the others found, no matches when a
/// part answered and no part returned a release, and found otherwise. A part
/// that was not asked is not an empty answer.
#[tokio::test]
async fn the_read_decides_where_the_search_stands() {
    use SourceAvailability::{NotConfigured, Off, On};
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let found = |source: LookupCatalog| {
        let release = format!("{}-1", source.as_str());
        Ok(vec![search_result(source, &release)])
    };
    let timed_out = || Err(Failure::from(bae_catalogs::LookupFailure::Timeout));
    let status = || async { search_of(&tables, key).await.unwrap().status };

    let run = started_search(&tables, key).await;
    assert!(
        land(&tables, run, LookupCatalog::MusicBrainz, found(LookupCatalog::MusicBrainz)).await
    );
    assert_eq!(status().await, SearchStatus::Searching, "Discogs is still looking");
    assert!(land(&tables, run, LookupCatalog::Discogs, timed_out()).await);
    assert_eq!(status().await, SearchStatus::Failed, "a failure heads the search beside a find");

    let run = started_search(&tables, key).await;
    assert!(
        land(&tables, run, LookupCatalog::MusicBrainz, found(LookupCatalog::MusicBrainz)).await
    );
    assert!(land(&tables, run, LookupCatalog::Discogs, found(LookupCatalog::Discogs)).await);
    assert_eq!(status().await, SearchStatus::Found);

    let (run, _) = start(&tables, key, &availability(On, NotConfigured)).await;
    assert!(land(&tables, run, LookupCatalog::MusicBrainz, Ok(Vec::new())).await);
    assert_eq!(status().await, SearchStatus::NoMatches);
    assert_eq!(
        search_of(&tables, key).await.unwrap().source(LookupCatalog::MusicBrainz),
        Some(&SourceSearch::Done { found: 0 })
    );

    let (run, _) = start(&tables, key, &availability(On, NotConfigured)).await;
    assert!(land(&tables, run, LookupCatalog::MusicBrainz, timed_out()).await);
    assert_eq!(status().await, SearchStatus::Failed);

    let run = started_search(&tables, key).await;
    assert!(land(&tables, run, LookupCatalog::MusicBrainz, Ok(Vec::new())).await);
    assert!(land(&tables, run, LookupCatalog::Discogs, Ok(Vec::new())).await);
    assert_eq!(status().await, SearchStatus::NoMatches, "both answered with nothing");

    let (run, _) = start(&tables, key, &availability(Off, On)).await;
    assert!(land(&tables, run, LookupCatalog::Discogs, Ok(Vec::new())).await);
    assert_eq!(status().await, SearchStatus::NoMatches);

    start(&tables, key, &availability(Off, NotConfigured)).await;
    assert_eq!(
        status().await,
        SearchStatus::Found,
        "no source answered, so nothing answered with nothing"
    );
}

/// A new search replaces the key's search on a new run: the replaced run's
/// landing goes nowhere and leaves the new one untouched.
#[tokio::test]
async fn a_replaced_search_cannot_land() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let first = started_search(&tables, key).await;
    let second = started_search(&tables, key).await;
    assert_ne!(first, second);
    assert!(!is_current(&tables, key, first).await);
    assert!(is_current(&tables, key, second).await);

    assert!(
        !land(
            &tables,
            first,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(LookupCatalog::MusicBrainz, "mb-1")]),
        )
        .await
    );
    assert_eq!(
        search_of(&tables, key)
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
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let unscanned = &pane_candidate_key();
    assert!(tables
        .start_candidate_search(unscanned, &search_query(), &every_source_on())
        .await
        .is_err());
    assert!(search_of(&tables, unscanned).await.is_none());

    let release = &CandidateKey::reidentify("release-1");
    started_search(&tables, release).await;
    assert!(search_of(&tables, release).await.is_some());
}

/// Retry re-asks only the failed source, on the same run, keeping what the
/// other found. A search with nothing to re-ask changes nothing.
#[tokio::test]
async fn a_retry_re_asks_only_the_failed_sources() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(LookupCatalog::MusicBrainz, "mb-1")]),
        )
        .await
    );
    let unavailable: Failure = bae_catalogs::LookupFailure::Provider { status: Some(503) }.into();
    assert!(land(&tables, run, LookupCatalog::Discogs, Err(unavailable.clone())).await);
    assert_eq!(
        search_of(&tables, key).await.unwrap().source(LookupCatalog::Discogs),
        Some(&SourceSearch::Failed(unavailable)),
        "the failure reads back as it landed"
    );

    let (retried, query, sources) =
        tables.retry_candidate_search(key).await.unwrap().expect("Discogs failed");
    assert_eq!(retried, run);
    assert_eq!(query, search_query());
    assert_eq!(sources, vec![LookupCatalog::Discogs]);
    let search = search_of(&tables, key).await.expect("the retried search is in flight");
    assert!(matches!(search.source(LookupCatalog::MusicBrainz), Some(SourceSearch::Done { .. })));
    assert_eq!(search.source(LookupCatalog::Discogs), Some(&SourceSearch::Searching));

    assert!(
        land(
            &tables,
            retried,
            LookupCatalog::Discogs,
            Ok(vec![search_result(LookupCatalog::Discogs, "dg-1")]),
        )
        .await
    );
    assert!(
        tables.retry_candidate_search(key).await.unwrap().is_none(),
        "a settled search has no source to re-ask"
    );
    assert!(
        tables
            .retry_candidate_search(&CandidateKey::test("/watch/a/rel2"))
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
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    let broke: Failure = bae_catalogs::InternalFailure {
        detail: "reading the answer: unexpected end of input".to_string(),
    }
    .into();
    assert!(land(&tables, run, LookupCatalog::Discogs, Err(broke.clone())).await);
    assert_eq!(
        search_of(&tables, key).await.unwrap().source(LookupCatalog::Discogs),
        Some(&SourceSearch::Failed(broke)),
        "bae breaking reads back as its error"
    );

    let (_, _, sources) = tables.retry_candidate_search(key).await.unwrap().unwrap();
    assert_eq!(sources, vec![LookupCatalog::Discogs]);
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(LookupCatalog::MusicBrainz, "mb-1")]),
        )
        .await,
        "MusicBrainz's answer was out before the retry and lands after it"
    );
}

/// A part that settled takes no second answer, and a catalog the search
/// never asked takes none at all.
#[tokio::test]
async fn only_a_part_still_searching_takes_an_answer() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let (run, _) = start(
        &tables,
        key,
        &[CatalogAvailability { catalog: LookupCatalog::Discogs, state: SourceAvailability::On }],
    )
    .await;

    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![search_result(LookupCatalog::Discogs, "dg-1")]),
        )
        .await
    );
    assert!(
        !land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![search_result(LookupCatalog::Discogs, "dg-2")]),
        )
        .await,
        "a part that answered takes no second answer"
    );
    assert!(
        !land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(LookupCatalog::MusicBrainz, "mb-1")]),
        )
        .await,
        "the search never asked MusicBrainz"
    );

    let search = search_of(&tables, key).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), None);
    assert_eq!(search.groups.len(), 1);
    assert_eq!(search.groups[0].sections[0].pressings[0].lead().release_id, "dg-1");
}

/// Switching a source off closes its part of every search, wherever the
/// person is looking, and takes what it returned out of the cards; the other
/// source's part and the run stand.
#[tokio::test]
async fn switching_a_source_off_closes_its_part_of_every_search() {
    let (tables, answered, _tmp) = scanned().await;
    let answered = &answered;
    let searching = &CandidateKey::reidentify("release-1");
    let answered_run = started_search(&tables, answered).await;
    let searching_run = started_search(&tables, searching).await;
    for source in LookupCatalog::ALL {
        let release = format!("{}-1", source.as_str());
        assert!(
            land(&tables, answered_run, source, Ok(vec![search_result(source, &release)]),).await
        );
    }
    assert_eq!(search_of(&tables, answered).await.unwrap().groups.len(), 2);

    tables.switch_search_source_off(LookupCatalog::MusicBrainz).await.unwrap();

    let search = search_of(&tables, answered).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), Some(&SourceSearch::Off));
    assert!(matches!(search.source(LookupCatalog::Discogs), Some(SourceSearch::Done { .. })));
    assert_eq!(search.groups.len(), 1, "MusicBrainz's card is gone");
    let search = search_of(&tables, searching).await.unwrap();
    assert_eq!(search.source(LookupCatalog::MusicBrainz), Some(&SourceSearch::Off));
    assert_eq!(
        search.source(LookupCatalog::Discogs),
        Some(&SourceSearch::Searching),
        "the source still being asked is untouched"
    );
    assert!(
        is_current(&tables, searching, searching_run).await,
        "the run stands, so the other source's answer still lands"
    );
}

/// The answer the dropped source already had out lands on a part that is no
/// longer searching, so it goes nowhere and the search stays closed on it.
#[tokio::test]
async fn a_landing_for_a_switched_off_source_goes_nowhere() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;

    tables.switch_search_source_off(LookupCatalog::MusicBrainz).await.unwrap();
    assert!(
        !land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![search_result(LookupCatalog::MusicBrainz, "mb-1")]),
        )
        .await
    );

    let search =
        search_of(&tables, key).await.expect("the search is what is in flight for the key");
    assert_eq!(
        search.source(LookupCatalog::MusicBrainz),
        Some(&SourceSearch::Off),
        "a part that is not searching takes no answer"
    );
    assert!(search.groups.is_empty());
}

/// The albums kept for the release group `group`, beyond any list.
pub(crate) async fn kept_for(
    tables: &ImportTables,
    group: &str,
) -> Vec<bae_catalogs::album_links::GroupAlbum> {
    let group = MetadataRef::new(Catalog::MusicBrainz, group);
    tables
        .store
        .read(move |sql| {
            crate::tables::album_link_rows::group_albums_of_on(&sql, &[vec![group]])
                .map(|mut kept| kept.pop().expect("one album has one entry"))
        })
        .await
        .unwrap()
}

/// A pick of a MusicBrainz result reads what its documents state its album
/// is, and that joins the cards in every read of the search after it, and is
/// kept for its release group in the landing's own write. A pick of a release
/// the search's list does not hold, a partner no catalog returned to it
/// included, changes nothing.
#[tokio::test]
async fn a_pick_joins_the_search_s_cards_from_then_on() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    let grouped = |source: LookupCatalog, release_id: &str, group: &str| MetadataResult {
        source_group_id: Some(group.to_string()),
        ..search_result(source, release_id)
    };
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![grouped(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
        )
        .await
    );
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![grouped(LookupCatalog::Discogs, "dg-1", "master-7")]),
        )
        .await
    );
    assert_eq!(search_of(&tables, key).await.unwrap().groups.len(), 2);
    let master = MetadataRef::new(Catalog::Discogs, "master-7");
    let links = AlbumLinks::Read(vec![master.clone()]);

    tables
        .land_candidate_search_pick(
            key,
            &bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-partner"),
            links.clone(),
        )
        .await
        .unwrap();
    assert_eq!(search_of(&tables, key).await.unwrap().groups.len(), 2);
    assert!(kept_for(&tables, "group-x").await.is_empty());

    tables
        .land_candidate_search_pick(
            key,
            &bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-1"),
            links.clone(),
        )
        .await
        .unwrap();
    assert_eq!(
        kept_for(&tables, "group-x").await,
        [bae_catalogs::album_links::GroupAlbum { group: "group-x".to_string(), album: master }],
        "the landing keeps what the group was read to be"
    );
    let search = search_of(&tables, key).await.unwrap();
    assert_eq!(search.groups.len(), 1, "the picked result joins the cards");
    assert_cards_are_the_rules_cards(&tables, run, &search).await;
}

/// A search of a folder the scan no longer holds, as one removed after the
/// pane showed it, is refused as that, and stores no search.
#[tokio::test]
async fn a_search_of_a_folder_the_scan_does_not_hold_is_refused() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let key = pane_candidate_key();

    let refused = tables.start_candidate_search(&key, &search_query(), &every_source_on()).await;

    assert!(
        matches!(
            &refused,
            Err(crate::ImportError::CandidateState(
                crate::tables::CandidateStateRefusal::NotScanned(named)
            )) if *named == key
        ),
        "{refused:?}"
    );
}

/// The cards a search reads are the cards the rules make of the releases the
/// search holds, as the last write that changed them grouped them with
/// bae_identify's grouping rules.
pub(crate) async fn assert_cards_are_the_rules_cards(
    tables: &ImportTables,
    run: SearchRun,
    search: &CandidateSearch,
) {
    let landed = every_returned(tables, run).await;
    assert_eq!(
        search.groups,
        bae_identify::release_group::group_results(
            bae_identify::release_group::unranked(landed),
            None
        )
    );
}
