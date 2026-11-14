//! A person's typed search as its rows keep it: each catalog's answer
//! landing on the run it was asked for, a retry, a source switched off, and
//! a picked release's documents joining it.

use super::*;
use crate::candidate_search::SourceSearch;
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

fn search_result(source: LookupCatalog, release_id: &str) -> MetadataResult {
    MetadataResult {
        title: "Album Title".to_string(),
        artist: Some(bae_types::credits::CreditLine::of_name("Artist Name")),
        year: Some(1992),
        ..MetadataResult::for_test(source, release_id, None)
    }
}

/// Tables over a library whose scan holds one folder candidate, and its key.
async fn scanned() -> (ImportTables, CandidateKey, tempfile::TempDir) {
    let (db, tmp) = empty_db(crate::TEST_SCHEMA).await;
    stored_pane_candidate(&db).await;
    (ImportTables::new(db), pane_candidate_key(), tmp)
}

/// The search `key` holds, with the run it is on.
async fn stored(tables: &ImportTables, key: &CandidateKey) -> Option<(SearchRun, CandidateSearch)> {
    let key = key.clone();
    tables.store.read(move |sql| Ok(load_on(&sql, Some(&key))?.remove(&key))).await.unwrap()
}

/// The search `key` holds.
async fn search_of(tables: &ImportTables, key: &CandidateKey) -> Option<CandidateSearch> {
    stored(tables, key).await.map(|(_, search)| search)
}

/// Whether `run` is the run `key`'s search is on.
async fn is_current(tables: &ImportTables, key: &CandidateKey, run: SearchRun) -> bool {
    stored(tables, key).await.map(|(current, _)| current) == Some(run)
}

/// Start a search of [`search_query`] for `key`, every catalog asked.
async fn started_search(tables: &ImportTables, key: &CandidateKey) -> SearchRun {
    tables
        .start_candidate_search(key, &CandidateSearch::started(search_query(), &every_source_on()))
        .await
        .unwrap()
}

/// Land `outcome` as `source`'s answer on `run`; whether it landed.
async fn land(
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
        .start_candidate_search(
            unscanned,
            &CandidateSearch::started(search_query(), &every_source_on()),
        )
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
    let run = tables
        .start_candidate_search(
            key,
            &CandidateSearch::started(
                search_query(),
                &[CatalogAvailability {
                    catalog: LookupCatalog::Discogs,
                    state: SourceAvailability::On,
                }],
            ),
        )
        .await
        .unwrap();

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
async fn kept_for(
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
    assert_cards_are_the_rules_cards(&search);
}

/// A search of a folder the scan no longer holds, as one removed after the
/// pane showed it, is refused as that, and stores no search.
#[tokio::test]
async fn a_search_of_a_folder_the_scan_does_not_hold_is_refused() {
    let (db, _tmp) = empty_db(crate::TEST_SCHEMA).await;
    let tables = ImportTables::new(db);
    let key = pane_candidate_key();

    let refused = tables
        .start_candidate_search(&key, &CandidateSearch::started(search_query(), &every_source_on()))
        .await;

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

/// One release of a search's list as the layout stored it: its row, its
/// card and its place among its row's records.
#[derive(Debug, PartialEq, Eq)]
struct Laid {
    release_id: String,
    pressing: i64,
    card: i64,
    record_order: i64,
}

/// Every release of the search `run`, in list order, as stored.
async fn laid_out(tables: &ImportTables, run: SearchRun) -> Vec<Laid> {
    tables
        .store
        .read(move |sql| {
            Ok(sql.query(
                "SELECT release_id, pressing, card, record_order \
                 FROM import_candidate_match AS found \
                 JOIN import_candidate_search_source AS part \
                   ON part.search_run = found.search_run AND part.catalog = found.source \
                 WHERE found.search_run = ? ORDER BY part.position, found.position",
                [run.number()],
                |row| {
                    Ok(Laid {
                        release_id: row.get(0)?,
                        pressing: row.get(1)?,
                        card: row.get(2)?,
                        record_order: row.get(3)?,
                    })
                },
            )?)
        })
        .await
        .unwrap()
}

/// The text of the search `run`'s cards, as `kind ordinal text` under each
/// card, in order.
async fn card_text(tables: &ImportTables, run: SearchRun) -> Vec<String> {
    tables
        .store
        .read(move |sql| {
            Ok(sql.query(
                "SELECT card || ' ' || kind || ' ' || ordinal || ' ' || text \
                 FROM import_candidate_card_text WHERE search_run = ? \
                 ORDER BY card, kind, ordinal",
                [run.number()],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap()
}

/// What the search `run`'s rows list, as `pressing fact line ordinal value
/// catalogs`, in order.
async fn statements(tables: &ImportTables, run: SearchRun) -> Vec<String> {
    tables
        .store
        .read(move |sql| {
            Ok(sql.query(
                "SELECT pressing || ' ' || fact || ' ' || line || ' ' || ordinal || ' ' \
                        || value || ' ' || catalogs \
                 FROM import_candidate_row_statement WHERE search_run = ? \
                 ORDER BY pressing, fact, line, ordinal",
                [run.number()],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap()
}

/// A release whose barcode `code` names the pressing it is, in the group its
/// catalog files its album under.
fn pressed(source: LookupCatalog, release_id: &str, group: &str, code: &str) -> MetadataResult {
    MetadataResult {
        source_group_id: Some(group.to_string()),
        barcodes: vec![code.to_string()],
        ..search_result(source, release_id)
    }
}

/// Each catalog's answer lays the whole list out again: the MusicBrainz and
/// the Discogs release that print one barcode become one row under one card,
/// the MusicBrainz record leading, and a row that nothing pairs stays alone.
/// What the row lists where its records differ is stored with it.
#[tokio::test]
async fn search_answer_pairs_rows_across_both_catalogs_answers() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    let mb = |id, group, code| pressed(LookupCatalog::MusicBrainz, id, group, code);
    let dg = |id, group, code| pressed(LookupCatalog::Discogs, id, group, code);

    let numbered = |mut release: MetadataResult, number: &str| {
        release.labels = vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some(number))];
        release
    };
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![
                numbered(mb("mb-1", "group-x", "012345678905"), "CAT 1"),
                mb("mb-2", "group-y", "5051961234567")
            ]),
        )
        .await
    );
    let alone: Vec<(i64, i64, i64)> = laid_out(&tables, run)
        .await
        .into_iter()
        .map(|laid| (laid.pressing, laid.card, laid.record_order))
        .collect();
    assert_eq!(alone, [(0, 0, 0), (1, 1, 0)], "two releases of two albums are two rows");

    let discogs_later = numbered(dg("dg-1", "master-7", "012345678905"), "CAT 2");
    assert!(land(&tables, run, LookupCatalog::Discogs, Ok(vec![discogs_later])).await);

    assert_eq!(
        laid_out(&tables, run).await,
        [
            Laid { release_id: "mb-1".into(), pressing: 0, card: 0, record_order: 0 },
            Laid { release_id: "mb-2".into(), pressing: 1, card: 1, record_order: 0 },
            Laid { release_id: "dg-1".into(), pressing: 0, card: 0, record_order: 1 },
        ],
        "the Discogs release joins the MusicBrainz release's row and card"
    );
    assert_eq!(
        card_text(&tables, run).await,
        [
            "0 artist 0 [{\"name\":\"Artist Name\",\"join\":null}]",
            "0 label 0 Imprint",
            "0 title 0 Album Title",
            "1 artist 0 [{\"name\":\"Artist Name\",\"join\":null}]",
            "1 title 0 Album Title",
        ]
    );
    assert_eq!(
        statements(&tables, run).await,
        [
            "0 barcode 0 0 012345678905 []",
            "0 catalog_number 0 0 CAT 1 [\"musicbrainz\"]",
            "0 catalog_number 0 1 CAT 2 [\"discogs\"]",
            "0 label_name 0 0 Imprint []",
            "0 year 0 0 1992 []",
            "1 barcode 0 0 5051961234567 []",
            "1 year 0 0 1992 []",
        ],
        "a row lists a fact once where its records agree"
    );
}

/// A pick of a MusicBrainz release reads what its documents state its album
/// is on Discogs, and the list is laid out again with that: the two
/// albums' cards become one, though no row pairs their releases.
#[tokio::test]
async fn discogs_answer_after_a_pick_joins_the_cards_by_what_both_print() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![pressed(LookupCatalog::MusicBrainz, "mb-1", "group-x", "012345678905")]),
        )
        .await
    );
    tables
        .land_candidate_search_pick(
            key,
            &bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-1"),
            AlbumLinks::Read(vec![MetadataRef::new(Catalog::Discogs, "master-7")]),
        )
        .await
        .unwrap();
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![pressed(LookupCatalog::Discogs, "dg-1", "master-7", "5051961234567")]),
        )
        .await
    );

    assert_eq!(
        laid_out(&tables, run).await,
        [
            Laid { release_id: "mb-1".into(), pressing: 0, card: 0, record_order: 0 },
            Laid { release_id: "dg-1".into(), pressing: 1, card: 0, record_order: 0 },
        ],
        "two rows of one card: the picked album's link joined the cards, not the pressings"
    );
    let stored_links: Vec<String> = tables
        .store
        .read(move |sql| {
            Ok(sql.query(
                "SELECT found.release_id || ' ' || found.album_links || ' ' \
                        || COALESCE(link.catalog || ':' || link.key, '') \
                 FROM import_candidate_match AS found \
                 LEFT JOIN import_candidate_match_album_link AS link \
                   ON link.search_run = found.search_run AND link.position = found.position \
                 WHERE found.search_run = ? ORDER BY found.release_id",
                [run.number()],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(
        stored_links,
        ["dg-1 not_asked ", "mb-1 read discogs:master-7"],
        "the MusicBrainz release states the album its group was read to be"
    );
}

/// Switching a catalog off deletes what it returned and lays out what the
/// other catalog returned on its own: the row the two shared is the
/// remaining release alone.
#[tokio::test]
async fn switching_a_catalog_off_lays_out_the_rest_again() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    for (source, id, group) in [
        (LookupCatalog::MusicBrainz, "mb-1", "group-x"),
        (LookupCatalog::Discogs, "dg-1", "master-7"),
    ] {
        assert!(
            land(&tables, run, source, Ok(vec![pressed(source, id, group, "012345678905")])).await
        );
    }
    assert_eq!(
        laid_out(&tables, run).await.into_iter().map(|laid| laid.pressing).collect::<Vec<_>>(),
        [0, 0],
        "the two catalogs' releases share a row"
    );

    tables.switch_search_source_off(LookupCatalog::MusicBrainz).await.unwrap();

    assert_eq!(
        laid_out(&tables, run).await,
        [Laid { release_id: "dg-1".into(), pressing: 0, card: 0, record_order: 0 }]
    );
    assert_eq!(
        card_text(&tables, run).await.len(),
        2,
        "the one card that is left names its title and its artist"
    );
}

/// The cards a search reads are the cards the rules make of the releases the
/// search holds, as they were laid out when the last write changed them.
fn assert_cards_are_the_rules_cards(search: &CandidateSearch) {
    let landed: Vec<MetadataResult> = search
        .sources
        .iter()
        .flat_map(|(_, part)| match part {
            SourceSearch::Done { results } => results.clone(),
            _ => Vec::new(),
        })
        .collect();
    assert_eq!(
        search.groups,
        bae_identify::release_group::group_results(
            bae_identify::release_group::unranked(landed),
            None
        )
    );
}

/// A MusicBrainz release of the group `group` pressed in `year`.
fn pressed_in(id: &str, group: &str, code: &str, year: Option<i32>) -> MetadataResult {
    MetadataResult { year, ..pressed(LookupCatalog::MusicBrainz, id, group, code) }
}

/// The rows of a card read most-agreed-with first, which a search leaves at
/// the pressing year: the earliest year first and the pressing nobody dates
/// last, rows of one year in the order they were listed.
#[tokio::test]
async fn search_cards_statement_orders_rows_by_pressing_year() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![
                pressed_in("mb-1995", "group-x", "012345678905", Some(1995)),
                pressed_in("mb-undated", "group-x", "5051961234567", None),
                pressed_in("mb-1992", "group-x", "4006381333931", Some(1992)),
                pressed_in("mb-1995-again", "group-x", "7318599911112", Some(1995)),
            ]),
        )
        .await
    );

    let search = search_of(&tables, key).await.unwrap();

    assert_eq!(search.groups.len(), 1);
    let rows: Vec<&str> = search.groups[0].sections[0]
        .pressings
        .iter()
        .map(|row| row.lead().release_id.as_str())
        .collect();
    assert_eq!(rows, ["mb-1992", "mb-1995", "mb-1995-again", "mb-undated"]);
    assert_cards_are_the_rules_cards(&search);
}

/// Two albums of one catalog on one card — two MusicBrainz groups a Discogs
/// master's two releases pair with — split the card into a section for each
/// album, headed by it, and no row of a section is of the other's album.
#[tokio::test]
async fn search_cards_statement_splits_a_card_holding_two_albums_of_one_catalog() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    let titled = |mut release: MetadataResult, title: &str| {
        release.title = title.to_string();
        release
    };
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![
                titled(
                    pressed(LookupCatalog::MusicBrainz, "mb-1", "group-x", "012345678905"),
                    "First Album"
                ),
                titled(
                    pressed(LookupCatalog::MusicBrainz, "mb-2", "group-y", "5051961234567"),
                    "Second Album"
                ),
            ]),
        )
        .await
    );
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![
                pressed(LookupCatalog::Discogs, "dg-1", "master-7", "012345678905"),
                pressed(LookupCatalog::Discogs, "dg-2", "master-7", "5051961234567"),
            ]),
        )
        .await
    );

    let search = search_of(&tables, key).await.unwrap();

    assert_eq!(search.groups.len(), 1, "the master joins both albums' rows on one card");
    let sections = &search.groups[0].sections;
    let headed: Vec<Option<&str>> = sections
        .iter()
        .map(|section| section.album.as_ref().map(|album| album.title.as_str()))
        .collect();
    assert_eq!(headed, [Some("First Album"), Some("Second Album")]);
    assert_cards_are_the_rules_cards(&search);
}

/// Discogs landing merges into the card MusicBrainz already drew: same
/// album's barcode, so one card with one row on both catalogs, the
/// MusicBrainz record leading, and the card names both catalogs.
#[tokio::test]
async fn a_later_catalog_merges_into_the_card_already_drawn() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![pressed(LookupCatalog::MusicBrainz, "mb-1", "group-x", "012345678905")]),
        )
        .await
    );
    let drawn = search_of(&tables, key).await.unwrap();
    assert_eq!(drawn.groups.len(), 1);
    assert_eq!(drawn.groups[0].pressings().count(), 1);
    assert_cards_are_the_rules_cards(&drawn);

    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![pressed(LookupCatalog::Discogs, "dg-1", "master-7", "012345678905")]),
        )
        .await
    );

    let merged = search_of(&tables, key).await.unwrap();
    assert_eq!(merged.groups.len(), 1);
    assert_eq!(merged.groups[0].sources.len(), 2);
    let ids: Vec<&str> = merged.groups[0].sections[0].pressings[0]
        .releases
        .iter()
        .map(|release| release.release_id.as_str())
        .collect();
    assert_eq!(ids, ["mb-1", "dg-1"]);
    assert_cards_are_the_rules_cards(&merged);
}

/// A picked result whose documents link nothing is read against what the
/// landed list prints: one catalog number under one label joins the albums,
/// and that is what the group is kept as. Before it is picked the same
/// numbers join nothing.
#[tokio::test]
async fn a_picked_result_no_document_links_is_joined_by_what_the_list_prints() {
    let (tables, key, _tmp) = scanned().await;
    let key = &key;
    let run = started_search(&tables, key).await;
    let numbered = |source: LookupCatalog, id: &str, group: &str| MetadataResult {
        barcodes: Vec::new(),
        labels: vec![bae_types::pressing::ReleaseLabel::of(Some("Imprint"), Some("LB 100"))],
        source_group_id: Some(group.to_string()),
        ..search_result(source, id)
    };
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::MusicBrainz,
            Ok(vec![numbered(LookupCatalog::MusicBrainz, "mb-1", "group-x")]),
        )
        .await
    );
    assert!(
        land(
            &tables,
            run,
            LookupCatalog::Discogs,
            Ok(vec![numbered(LookupCatalog::Discogs, "dg-1", "master-7")]),
        )
        .await
    );
    assert_eq!(
        search_of(&tables, key).await.unwrap().groups.len(),
        2,
        "nothing joins before a pick"
    );

    tables
        .land_candidate_search_pick(
            key,
            &bae_catalogs::records::LookupRef::new(LookupCatalog::MusicBrainz, "mb-1"),
            AlbumLinks::Read(Vec::new()),
        )
        .await
        .unwrap();

    let search = search_of(&tables, key).await.unwrap();
    assert_eq!(search.groups.len(), 1);
    assert_cards_are_the_rules_cards(&search);
    assert_eq!(
        kept_for(&tables, "group-x").await,
        [bae_catalogs::album_links::GroupAlbum {
            group: "group-x".to_string(),
            album: MetadataRef::new(Catalog::Discogs, "master-7"),
        }],
    );
}
