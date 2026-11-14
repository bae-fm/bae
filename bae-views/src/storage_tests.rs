//! The Storage Manager list: its counts, sizes and sorts as release files
//! change, its search, and every storage sort and the download queue read
//! from an index.

use crate::tests::{assert_seeks_search, both_ways, key_at, library};
use crate::*;
use bae_records::test_fixtures::aggregate_ordering::aggregate_db;
use bae_records::test_fixtures::album_listing::listed_db;
use bae_records::test_fixtures::bulk_library;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::storage_listing::*;
use bae_store::test_fixtures::trigger_order::{
    count_refreshes, first_insert, run_trigger_first, watch_first_insert, RefreshCount,
};
// The listing's albums, not the root fixtures' of the same names.
use bae_records::test_fixtures::storage_listing::{ALBUM_A, ALBUM_B};
use bae_records::test_fixtures::*;

/// The Storage Manager list's release ids in `field` order, ascending.
async fn page(db: &Store, field: StorageSortField) -> Vec<String> {
    crate::LibraryViews::new(db.clone())
        .get_storage_page(
            &StorageSortCriterion { field, direction: SortDirection::Ascending },
            StorageFilter::All,
            0,
            50,
        )
        .await
        .unwrap()
        .rows
        .into_iter()
        .map(|row| row.release.id)
        .collect()
}

#[tokio::test]
async fn the_storage_list_counts_and_sorts_by_files_as_they_change() {
    let (db, tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    write_library(&records).await;
    // The first release has two files, of 10 and 20 bytes; the second none.
    for file in [
        ("d3edb846-9b0b-4a64-b0f6-3f117b686bc0", RELEASE_EARLY, "cover.jpg", 10),
        ("161dd6f3-3bdd-4312-b5d3-8b9d83b53694", RELEASE_EARLY, "back.jpg", 20),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();

    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        2
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::Local).await.unwrap(),
        2
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_count(StorageFilter::Remote)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        30
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    // The second release's import stages three files, of 1, 2 and 3 bytes.
    for file in [
        ("f5008044-8ffe-4227-bdc3-238f85a6a4a6", RELEASE_LATE, "one.jpg", 1),
        ("cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3", RELEASE_LATE, "two.jpg", 2),
        ("4b6e1f0a-3c2d-4e5f-8a9b-0c1d2e3f4a5b", RELEASE_LATE, "three.jpg", 3),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        36
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_EARLY, RELEASE_LATE]));
    assert_eq!(page(&db, StorageSortField::TotalSize).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    // Retitling the first album sorts its release after the second.
    records.rename_album_for_test(ALBUM_A, "Album Title C").await.unwrap();
    assert_eq!(page(&db, StorageSortField::AlbumTitle).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    delete_release(&records, RELEASE_EARLY).await;
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        1
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        6
    );
}

/// Sorting by storage reads a window from an index of the listing in either
/// direction, under every filter: SQLite never sorts the list itself.
#[tokio::test]
async fn the_storage_sort_reads_a_window_from_an_index() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    for filter in [StorageFilter::All, StorageFilter::Local, StorageFilter::Remote] {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let sort = StorageSortCriterion { field: StorageSortField::Storage, direction };
            let order = crate::storage::storage_order(&sort);
            let steps = plan(
                &db,
                crate::storage::storage_window_query(
                    &order,
                    filter,
                    crate::storage::StorageRows::Listed,
                    &order.everything(),
                )
                .replace("LIMIT :limit OFFSET :offset", "LIMIT 50 OFFSET 0"),
            )
            .await;
            assert!(
                steps.iter().any(|step| step.contains("USING INDEX idx_release_listing_")
                    && step.contains("storage")),
                "{filter:?} {direction:?}: {steps:?}"
            );
            assert!(
                !crate::tests::picked_window(&steps)
                    .iter()
                    .any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
                "{filter:?} {direction:?}: {steps:?}"
            );
        }
    }
}

#[tokio::test]
async fn every_storage_sort_and_filter_seeks_its_windows_through_an_index() {
    let (db, _tmp) = library().await;
    for filter in [StorageFilter::All, StorageFilter::Local, StorageFilter::Remote] {
        for field in [
            StorageSortField::AlbumTitle,
            StorageSortField::ArtistNames,
            StorageSortField::Media,
            StorageSortField::FileCount,
            StorageSortField::TotalSize,
            StorageSortField::Storage,
            StorageSortField::DateAdded,
        ] {
            for (field, direction) in both_ways(field) {
                let order =
                    crate::storage::storage_order(&StorageSortCriterion { field, direction });
                // Under Remote no release is listed, so the middle row of
                // every release stands in for one.
                let key = key_at(
                    &db,
                    &order,
                    |sql, order, seek, limit, offset| {
                        crate::storage::storage_rows_on(
                            sql,
                            order,
                            StorageFilter::All,
                            None,
                            seek,
                            limit,
                            offset,
                        )
                    },
                    150,
                )
                .await;
                assert_seeks_search(
                    &db,
                    &format!("storage {filter:?} {field:?} {direction:?}"),
                    &order,
                    key,
                    |seek| {
                        crate::storage::storage_window_query(
                            &order,
                            filter,
                            crate::storage::StorageRows::Listed,
                            seek,
                        )
                    },
                    Vec::new(),
                )
                .await;
            }
        }
    }
}

/// The search pattern of `text`, as a statement binds it.
fn search_param(text: &str) -> Vec<(&'static str, rusqlite::types::Value)> {
    vec![(":search", rusqlite::types::Value::Text(bae_util::search_text::search_pattern(text)))]
}

/// A searching Storage Manager list never scans the listing whole under
/// any sort and filter: each seek of a searched window reaches the listing
/// through its indexes, by the albums the search finds, and sorts only those.
#[tokio::test]
async fn every_storage_sort_and_filter_seeks_a_searched_window_through_an_index() {
    let (db, _tmp) = library().await;
    let search = search_param("album 1");
    for filter in [StorageFilter::All, StorageFilter::Local, StorageFilter::Remote] {
        for field in [
            StorageSortField::AlbumTitle,
            StorageSortField::ArtistNames,
            StorageSortField::Media,
            StorageSortField::FileCount,
            StorageSortField::TotalSize,
            StorageSortField::Storage,
            StorageSortField::DateAdded,
        ] {
            for (field, direction) in both_ways(field) {
                let order =
                    crate::storage::storage_order(&StorageSortCriterion { field, direction });
                let key = key_at(
                    &db,
                    &order,
                    |sql, order, seek, limit, offset| {
                        crate::storage::storage_rows_on(
                            sql,
                            order,
                            StorageFilter::All,
                            None,
                            seek,
                            limit,
                            offset,
                        )
                    },
                    150,
                )
                .await;
                for seek in order.seeks_beside(&key) {
                    let statement = crate::storage::storage_window_query(
                        &order,
                        filter,
                        crate::storage::StorageRows::Searched,
                        &seek,
                    );
                    let steps =
                        crate::tests::plan(&db, statement, seek.clone(), search.clone()).await;
                    assert!(
                        !steps.iter().any(|step| crate::tests::LIST_ALIASES.iter().any(|alias| {
                            step.starts_with(&format!("SCAN {alias} "))
                                || step == &format!("SCAN {alias}")
                        })),
                        "searched storage {filter:?} {field:?} {direction:?} {seek:?}: {steps:?}"
                    );
                }
            }
        }
    }
}

/// The count, the size and the list of a search under a filter agree: the
/// count is how many releases the list holds, the size is their files', for
/// a search found in titles, in the names the credits show, in a name a
/// credit prints, in titles and names at once, and in no text.
#[tokio::test]
async fn a_searched_count_and_list_find_the_same_releases() {
    let (db, _tmp) = library().await;
    // The first album's credit prints a name of its own.
    test_records(&db)
        .save_release_metadata_for_test(&bulk_library::release_id(0), |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Printed Name"));
        })
        .await
        .unwrap();
    let order = crate::storage::storage_order(&StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    });
    for text in ["album 1", "artist 0", "printed", "1", "nothing holds this"] {
        for filter in [StorageFilter::All, StorageFilter::Local] {
            let order = order.clone();
            let (counted, listed) = db
                .read(move |sql| {
                    let pattern = bae_util::search_text::search_pattern(text);
                    let counted = crate::storage::storage_counts_on(&sql, filter, Some(&pattern))?;
                    let statement = crate::storage::storage_window_query(
                        &order,
                        filter,
                        crate::storage::StorageRows::Searched,
                        &order.everything(),
                    );
                    let listed = order.everything().bound(u64::MAX, 0, |params| {
                        let mut params = params.to_vec();
                        params.push((":search", &pattern));
                        sql.query(&statement, params.as_slice(), |row| {
                            Ok((
                                row.get::<_, String>("release_id")?,
                                row.get::<_, i64>("total_size")?,
                            ))
                        })
                    })?;
                    Ok((counted, listed))
                })
                .await
                .unwrap();
            assert_eq!(counted.0 as usize, listed.len(), "{text:?} {filter:?}: count");
            assert_eq!(
                counted.1 as i64,
                listed.iter().map(|(_, size)| size).sum::<i64>(),
                "{text:?} {filter:?}: size"
            );
            assert_eq!(
                listed.is_empty(),
                text == "nothing holds this",
                "{text:?} {filter:?} finds {}",
                listed.len()
            );
        }
    }
}

/// The ids in the five rows of the Storage Manager list narrowed by `text`
/// that a window starting at `from` holds.
fn window_of_five_from(from: bae_store::list_windows::OrderKey) -> ListWindow {
    ListWindow::Forward { from: Some(from), until: None, limit: 5 }
}

fn ids_of(read: &WindowRead<(DbStorageRow, bool)>) -> Vec<String> {
    read.rows.iter().map(|(row, _)| row.release.id.clone()).collect()
}

/// A live Storage Manager window narrowed by a search and started at a key
/// holds what a fresh read from that key holds, and keeps holding it after a
/// name an album's credit prints brings an album into what the search finds
/// above the window, which moves no row of the listing itself: the window is
/// a range of the order from its key, wherever that key then sits.
#[tokio::test]
async fn a_search_window_from_a_key_holds_its_rows_across_a_printed_name() {
    let (db, _tmp) = library().await;
    let records = test_records(&db);
    // The first artist's albums sort by its sort name from now on, filled in
    // by a credit naming it with one, so the name its album's credit prints
    // below changes only what the search finds.
    let artist = "00000000-0001-4000-8000-000000000000";
    let met = records
        .find_or_create_artists(&[DbArtist {
            sort_name: Some("Artist 000000".to_string()),
            ..credit("Artist 000000")
        }])
        .await
        .unwrap();
    assert_eq!(met, [artist]);
    let views = crate::LibraryViews::new(db.clone());
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let from = views
        .storage_key_at(&sort, StorageFilter::All, "album 1", 10)
        .await
        .unwrap()
        .expect("the search finds more than ten releases");
    let fresh = |from: bae_store::list_windows::OrderKey| {
        let views = &views;
        let sort = &sort;
        async move {
            let mut query = views.subscribe_storage_window(
                sort,
                StorageFilter::All,
                "album 1",
                window_of_five_from(from),
            );
            ids_of(&query.next().await.unwrap().result.unwrap())
        }
    };
    let mut live = views.subscribe_storage_window(
        &sort,
        StorageFilter::All,
        "album 1",
        window_of_five_from(from.clone()),
    );
    let mut count = views.subscribe_storage_count(StorageFilter::All, "album 1");
    let before = ids_of(&live.next().await.unwrap().result.unwrap());
    let found = count.next().await.unwrap().unwrap();
    assert!(found.rows > 20, "the search finds {} releases", found.rows);
    assert_eq!(before.len(), 5);
    assert_eq!(before, fresh(from.clone()).await);

    records
        .save_release_metadata_for_test(&bulk_library::release_id(0), |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Album 1 Artist"));
        })
        .await
        .unwrap();
    let counted = loop {
        let counted = count.next().await.unwrap().unwrap();
        if counted.rows != found.rows {
            break counted;
        }
    };
    assert_eq!(counted.rows, found.rows + 1, "the album printing the new name joins the list");
    // The window starts at the same key, so the rows it holds are the ones it
    // held, whichever position that key has now.
    assert_eq!(fresh(from).await, before);
}

#[tokio::test]
async fn the_download_queue_seeks_its_windows_through_its_key() {
    let (db, _tmp) = library().await;
    let order = bae_transfers::tables::queue_order();
    let key = key_at(
        &db,
        &order,
        bae_transfers::tables::queue_window_rows_on::<bae_transfers::DownloadRequest>,
        150,
    )
    .await;
    assert_seeks_search(
        &db,
        "downloads",
        &order,
        key,
        |seek| {
            bae_transfers::tables::queue_rows_sql::<bae_transfers::DownloadRequest>(
                Some(&order),
                &format!(
                    "WHERE {} ORDER BY {} LIMIT :limit OFFSET :offset",
                    order.condition(seek),
                    order.ordering(seek)
                ),
            )
        },
        Vec::new(),
    )
    .await;
}

#[tokio::test]
async fn storage_page_orders_album_aggregate_columns_inside_aggregates() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = crate::LibraryViews::new(db.clone())
        .get_storage_page(&sort, StorageFilter::All, 0, 10)
        .await
        .unwrap()
        .rows;
    let row = rows.first().expect("storage row");

    assert_eq!(
        row.album.artist_line.plain_text(),
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

/// How many rows the library's tables, the listings' triggers included,
/// change when `release` is added to Album Title One.
async fn rows_changed_adding_release(db: &Store, release: &str) -> i64 {
    test_records(db)
        .rows_changed_inserting_release_for_test(&DbRelease::new_test(
            bae_records::test_fixtures::album_listing::ALBUM_ONE,
            release,
        ))
        .await
        .unwrap()
}

/// Adding a release to an album lists that release and leaves the album's
/// other releases' listing rows alone: what it costs doesn't grow with the
/// releases the album already holds, as a box set's would.
#[tokio::test]
async fn adding_a_release_rewrites_no_other_release_of_its_album() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    for i in 2..22 {
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid(&format!("box-set-{i}")))
            .await;
    }
    let twenty_second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-22")).await;
    assert_eq!(twenty_second, second, "the 22nd release changes as many rows as the second");
}

/// Renaming an album renames every one of its releases' listing rows, so
/// the Storage Manager sorts them by the new title.
#[tokio::test]
async fn renaming_an_album_renames_its_releases_listing_rows() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    test_records(&db)
        .rename_album_for_test(
            bae_records::test_fixtures::album_listing::ALBUM_ONE,
            "Album Title Renamed",
        )
        .await
        .unwrap();
    let renamed: Vec<bool> = db
        .call(|conn| {
            Ok(conn.query(
                "SELECT album_title_sort_key = bae_name_key('Album Title Renamed', \
                                                       (SELECT language FROM name_sort)) \
                 FROM release_listing \
                 WHERE album_id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(renamed, vec![true; 2]);
}

/// The trigger that runs first of the two on a release arriving, which
/// SQLite runs in no order it names: the listing's tests hold under each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstOnArrival {
    /// `release_added`, which lists the release's album, then the release.
    Listing,
    /// `release_storage_release_added`, which writes the release's storage
    /// row.
    Storage,
}

impl FirstOnArrival {
    const BOTH: [Self; 2] = [Self::Listing, Self::Storage];

    fn trigger(self) -> &'static str {
        match self {
            Self::Listing => "release_added",
            Self::Storage => "release_storage_release_added",
        }
    }

    /// The table the trigger writes first.
    fn writes(self) -> &'static str {
        match self {
            Self::Listing => "album_listing",
            Self::Storage => "release_storage",
        }
    }
}

/// An album and a release that tell which trigger runs first as a release
/// arrives, apart from the tests' own.
const PROBE_ALBUM: &str = "6d1f3a2b-4c5e-4f60-8a7b-9c0d1e2f3a01";
const PROBE_RELEASE: &str = "6d1f3a2b-4c5e-4f60-8a7b-9c0d1e2f3a02";

/// A library on which `first`'s trigger runs first as a release arrives,
/// with the count of every release whose listing row a write asks to
/// refresh.
async fn counting_refreshes(first: FirstOnArrival) -> (Store, tempfile::TempDir, RefreshCount) {
    let schema = count_refreshes(
        watch_first_insert(
            run_trigger_first(crate::TEST_SCHEMA, first.trigger()),
            &["album_listing", "release_storage"],
        ),
        "release_listing_refresh",
        "release_id",
    );
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    RecordsFixture::new()
        .album(PROBE_ALBUM, "Album Title Probe", None, &[])
        .release(PROBE_ALBUM, PROBE_RELEASE)
        .write(&test_records(&db))
        .await;
    assert_eq!(first_insert(&db).await.as_deref(), Some(first.writes()));
    (db, tmp, RefreshCount::of_view("release_listing_refresh"))
}

/// The album `release`'s listing row names, None while it has none.
async fn listed_album(db: &Store, release: &str) -> Option<String> {
    let release = release.to_string();
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT (SELECT album_id FROM release_listing WHERE release_id = ?1)",
            [&release],
            |row| row.get(0),
        )?)
    })
    .await
    .unwrap()
}

/// Album A, holding the early release.
fn album_a_with_early() -> RecordsFixture {
    RecordsFixture::new().album(ALBUM_A, "Album Title A", None, &[]).release(ALBUM_A, RELEASE_EARLY)
}

/// The late release, after album A's early one.
fn late_on_album_a() -> DbRelease {
    DbRelease::new_test(ALBUM_A, RELEASE_LATE)
}

/// A release arriving writes its listing row with one refresh, in the write
/// that adds it: after its album, which lists that album too, and as it
/// joins an album already listed.
#[tokio::test]
async fn a_release_arriving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);

        album_a_with_early().write(&records).await;
        assert_eq!(
            listed_album(&db, RELEASE_EARLY).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_EARLY).await, 1, "{first:?} first");

        records.insert_release(&late_on_album_a()).await.unwrap();
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
        assert_eq!(refreshes.of(&db, RELEASE_EARLY).await, 1, "{first:?} first");
    }
}

/// `release` moving from album `from` to album `to` as its records file it
/// there, `to` written by the same write when `new_album` says.
async fn move_release(
    records: &bae_records::Records,
    release: &str,
    (from, to): (&str, &str),
    new_album: Option<DbAlbum>,
) {
    records
        .set_records_atomic(bae_records::RecordsChange {
            release_id: release.to_string(),
            records: Vec::new(),
            current_album_id: from.to_string(),
            target_album_id: to.to_string(),
            new_album,
        })
        .await
        .unwrap();
}

/// A release moving writes its listing row with one refresh: to an album the
/// same write adds, which lists that album and its releases, and to an album
/// already listed.
#[tokio::test]
async fn a_release_moving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);
        album_a_with_early().write(&records).await;
        records.insert_release(&late_on_album_a()).await.unwrap();
        refreshes.clear(&db).await;

        let album_b = DbAlbum {
            id: ALBUM_B.to_string(),
            title: "Album Title B".to_string(),
            year: None,
            is_compilation: false,
            created_at: chrono::Utc::now(),
        };
        move_release(&records, RELEASE_LATE, (ALBUM_A, ALBUM_B), Some(album_b)).await;
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_B),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");

        refreshes.clear(&db).await;
        move_release(&records, RELEASE_LATE, (ALBUM_B, ALBUM_A), None).await;
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
    }
}

/// `release`'s listing row's media columns.
async fn listed_media(db: &Store, release: &str) -> (bool, Option<String>) {
    let release = release.to_string();
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT media_empty, media_key FROM release_listing WHERE release_id = ?1",
            [&release],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
    .await
    .unwrap()
}

/// `count` of `medium`.
fn carriers(medium: bae_types::pressing::Medium, count: u32) -> bae_types::pressing::MediaCount {
    bae_types::pressing::MediaCount { medium, count }
}

/// A release's media arriving, changing and leaving, the last two by a
/// person's metadata edit, set its listing row's media, in order, without
/// refreshing the rest of the row.
#[tokio::test]
async fn a_release_s_media_rows_set_its_listing_s_media() {
    use bae_types::pressing::Medium;
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);
        RecordsFixture::new()
            .album(ALBUM_A, "Album Title A", None, &[])
            .release(ALBUM_A, RELEASE_LATE)
            .edit(|rows| {
                rows.releases[0].pressing.facts.media =
                    vec![carriers(Medium::Vinyl, 2), carriers(Medium::Cd, 1)];
            })
            .write(&records)
            .await;
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
        assert_eq!(
            listed_media(&db, RELEASE_LATE).await,
            (false, Some("vinyl:00002,cd:00001".to_string()))
        );

        refreshes.clear(&db).await;
        records
            .save_release_metadata_for_test(RELEASE_LATE, |rows| {
                rows.release.pressing.facts.media =
                    vec![carriers(Medium::Vinyl, 3), carriers(Medium::Cd, 1)];
            })
            .await
            .unwrap();
        assert_eq!(
            listed_media(&db, RELEASE_LATE).await,
            (false, Some("vinyl:00003,cd:00001".to_string()))
        );

        records
            .save_release_metadata_for_test(RELEASE_LATE, |rows| {
                rows.release.pressing.facts.media = Vec::new();
            })
            .await
            .unwrap();
        assert_eq!(listed_media(&db, RELEASE_LATE).await, (true, None));
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 0, "{first:?} first");
    }
}

/// A release's media written on one device sort the release in the Storage
/// Manager's listing on another: the listing's media key follows the rows,
/// in order.
#[test]
fn a_media_edit_from_another_device_sorts_its_release() {
    use bae_types::pressing::Medium;
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let release = "0252dedb-ee39-4547-8803-438dbeb57a64";
        RecordsFixture::new()
            .album(ALBUM_A, "Album Title A", Some(2026), &[])
            .release(ALBUM_A, release)
            .write(&test_records(devices.a()))
            .await;
        test_records(devices.a())
            .save_release_metadata_for_test(release, |rows| {
                rows.release.pressing.facts.media =
                    vec![carriers(Medium::Vinyl, 2), carriers(Medium::Cd, 1)];
            })
            .await
            .unwrap();
        devices.converge("SELECT id, position, medium, count FROM release_media ORDER BY id").await;

        let key: Option<String> = devices
            .b()
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT media_key FROM release_listing WHERE release_id = ?1",
                    [release],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(key.as_deref(), Some("vinyl:00002,cd:00001"));
    });
}
