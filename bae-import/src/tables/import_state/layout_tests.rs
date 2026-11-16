//! How the writes of a typed search lay its list out into pressing rows and
//! album cards, and the cards its read selects from them: each catalog's
//! answer, a pick and a catalog switched off.

use super::*;
use crate::candidate_search::SearchRun;
use crate::tables::import_state::search_rows::tests::{
    assert_cards_are_the_rules_cards, kept_for, land, scanned, search_of, search_result,
    started_search,
};
use bae_catalogs::catalog::{Catalog, LookupCatalog};
use bae_catalogs::records::MetadataRef;
use bae_catalogs::search::MetadataResult;

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
/// The layout write stores, with the row, the facts on which its records
/// differ.
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

/// When a person picks a MusicBrainz release, bae stores the Discogs album
/// its documents link and groups the list again: the two albums' cards
/// become one, though no row pairs their releases.
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
        "the MusicBrainz release holds the Discogs album the pick found for its group"
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

/// A MusicBrainz release of the group `group` pressed in `year`.
fn pressed_in(id: &str, group: &str, code: &str, year: Option<i32>) -> MetadataResult {
    MetadataResult { year, ..pressed(LookupCatalog::MusicBrainz, id, group, code) }
}

/// A search orders a card's rows by pressing year, earliest first and undated
/// last, and rows of one year in the order the catalogs returned them.
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
    assert_cards_are_the_rules_cards(&tables, run, &search).await;
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
    assert_cards_are_the_rules_cards(&tables, run, &search).await;
}

/// When Discogs answers later with a release that prints the same barcode,
/// bae adds it to the MusicBrainz release's row on the existing card: the
/// card has one row on both catalogs, the MusicBrainz record leading, and
/// the card names both catalogs.
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
    assert_cards_are_the_rules_cards(&tables, run, &drawn).await;

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
    assert_cards_are_the_rules_cards(&tables, run, &merged).await;
}

/// When a picked result's documents link nothing, bae compares it with the
/// catalog numbers the list prints, and stores the album they share under one
/// label as the group's. Before the pick, the same numbers join nothing.
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
    assert_cards_are_the_rules_cards(&tables, run, &search).await;
    assert_eq!(
        kept_for(&tables, "group-x").await,
        [bae_catalogs::album_links::GroupAlbum {
            group: "group-x".to_string(),
            album: MetadataRef::new(Catalog::Discogs, "master-7"),
        }],
    );
}
