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
use bae_store::{LibraryPageWindow, LibraryPageWindows};

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

/// The aliases a searching Storage Manager statement reads what the search
/// finds through, beside the list's: never scanned whole.
const SEARCH_ALIASES: &[&str] = &["credit", "shown", "listing", "storage", "r", "a"];

/// Whether a plan scans one of `aliases` whole.
fn scans(steps: &[String], aliases: &[&str]) -> bool {
    steps.iter().any(|step| {
        aliases.iter().any(|alias| {
            step.starts_with(&format!("SCAN {alias} ")) || step == &format!("SCAN {alias}")
        })
    })
}

/// Whether a plan reads the search tables only by entry: each album's text
/// looked up by its key, never the pattern searched for in a table's index,
/// nor a table passed over whole.
fn reads_search_text_by_entry(steps: &[String]) -> bool {
    let reads = ["title_text", "artist_text", "printed_text"].map(|alias| {
        steps
            .iter()
            .filter(|step| step.starts_with(&format!("SCAN {alias} ")))
            .map(|step| step.ends_with("VIRTUAL TABLE INDEX 0:="))
            .collect::<Vec<_>>()
    });
    reads.iter().all(|read| read == &[true])
}

/// The search pattern of `text`, as a statement binds it.
fn search_param(text: &str) -> Vec<(&'static str, rusqlite::types::Value)> {
    vec![(":search", rusqlite::types::Value::Text(bae_util::search_text::search_pattern(text)))]
}

/// A searching Storage Manager list stays on its indexes under every sort
/// and filter: read in the order's index, each seek searches the index,
/// sorts nothing and tests each album by its own search text; read from what
/// the search finds, each reaches the listing by its album and sorts only
/// those rows; and its counts and its selection read what the search finds
/// and the selected keys, never the whole list.
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
                let what = format!("searched storage {filter:?} {field:?} {direction:?}");
                assert_seeks_search(
                    &db,
                    &what,
                    &order,
                    key.clone(),
                    |seek| {
                        crate::storage::storage_window_query(
                            &order,
                            filter,
                            crate::storage::StorageRows::SearchedInOrder,
                            seek,
                        )
                    },
                    search.clone(),
                )
                .await;
                for seek in order.seeks_beside(&key) {
                    let statement = crate::storage::storage_window_query(
                        &order,
                        filter,
                        crate::storage::StorageRows::SearchedInOrder,
                        &seek,
                    );
                    let steps =
                        crate::tests::plan(&db, statement, seek.clone(), search.clone()).await;
                    assert!(
                        reads_search_text_by_entry(&steps),
                        "in order {what} {seek:?}: {steps:?}"
                    );
                    let statement = crate::storage::storage_window_query(
                        &order,
                        filter,
                        crate::storage::StorageRows::Found,
                        &seek,
                    );
                    let steps =
                        crate::tests::plan(&db, statement, seek.clone(), search.clone()).await;
                    assert!(
                        !scans(&steps, SEARCH_ALIASES)
                            && steps
                                .iter()
                                .any(|step| step.contains("USING INDEX idx_release_listing_album")),
                        "found {what} {seek:?}: {steps:?}"
                    );
                }
            }
        }
        let read = |statement: String, params: Vec<(&'static str, rusqlite::types::Value)>| {
            let db = &db;
            async move {
                db.read(move |sql| {
                    let params = params
                        .iter()
                        .map(|(name, value)| (*name, value as &dyn rusqlite::ToSql))
                        .collect::<Vec<_>>();
                    Ok(sql.query(
                        &format!("EXPLAIN QUERY PLAN {statement}"),
                        params.as_slice(),
                        |row| row.get::<_, String>(3),
                    )?)
                })
                .await
                .unwrap()
            }
        };
        let mut counted = search.clone();
        counted.push((":titles", rusqlite::types::Value::Integer(30)));
        let steps = read(crate::storage::storage_found_totals_query(filter), counted).await;
        assert!(!scans(&steps, SEARCH_ALIASES), "count {filter:?}: {steps:?}");
        // The pass over the titles reads every title once, and nothing else
        // whole.
        let steps =
            read(crate::storage::storage_unfound_totals_query(filter), search.clone()).await;
        assert!(
            !scans(&steps, SEARCH_ALIASES)
                && steps
                    .iter()
                    .filter(|step| step.ends_with("VIRTUAL TABLE INDEX 0:"))
                    .eq(["SCAN untitled VIRTUAL TABLE INDEX 0:"].iter()),
            "unfound {filter:?}: {steps:?}"
        );
        let mut selected = search.clone();
        selected.push((":selected", rusqlite::types::Value::Text(r#"["a", "b"]"#.to_string())));
        let steps = read(crate::storage::storage_selected_query(filter, true), selected).await;
        assert!(
            !scans(&steps, SEARCH_ALIASES) && reads_search_text_by_entry(&steps),
            "selection {filter:?}: {steps:?}"
        );
    }
}

/// The count of `text`'s search under `filter` read from what the indexes
/// find, unless its pattern is in the titles of `titles` albums: whether it
/// was read, the count and the size.
fn found_totals(
    sql: &SqlReadContext<'_>,
    filter: StorageFilter,
    text: &str,
    titles: i64,
) -> rusqlite::Result<(bool, i64, i64)> {
    sql.query_row(
        &crate::storage::storage_found_totals_query(filter),
        rusqlite::named_params! {
            ":search": bae_util::search_text::search_pattern(text),
            ":titles": titles,
        },
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
}

/// The count and size `statement` reads for `text`'s search.
fn pass_totals(
    sql: &SqlReadContext<'_>,
    statement: String,
    text: &str,
) -> rusqlite::Result<(i64, i64)> {
    sql.query_row(
        &statement,
        rusqlite::named_params! { ":search": bae_util::search_text::search_pattern(text) },
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
}

/// How a search is read at the top of the list: a rare one counted and read
/// from what it finds, so it never walks the whole index; one in most titles
/// left to the pass over the titles and read through the order's index.
#[tokio::test]
async fn a_rare_search_reads_what_it_finds_and_a_broad_one_the_index() {
    let (db, _tmp) = library().await;
    let read_at_top = |text: &'static str| {
        let db = &db;
        async move {
            db.read(move |sql| {
                let query = crate::LibrarySearchQuery::parse(text).unwrap();
                let titles = crate::storage::storage_found_titles(300) as i64;
                let (counted, _, _) = found_totals(&sql, StorageFilter::All, text, titles)?;
                let (found, _, search) =
                    crate::storage::storage_totals_on(&sql, StorageFilter::All, Some(&query))?;
                Ok((found, counted, search.unwrap().rows(50, 0)))
            })
            .await
            .unwrap()
        }
    };
    // The second album's title, "Album 16807", is the only text holding it.
    assert_eq!(read_at_top("16807").await, (1, true, crate::storage::StorageRows::Found));
    assert_eq!(
        read_at_top("album").await,
        (300, false, crate::storage::StorageRows::SearchedInOrder)
    );
}

/// A search is counted through the indexes while its pattern is in fewer
/// titles than a pass over every title costs, about a fifth of them.
#[test]
fn a_search_in_a_fifth_of_the_titles_is_counted_by_a_pass() {
    assert_eq!(crate::storage::storage_found_titles(100_000), 21_052);
    assert_eq!(crate::storage::storage_found_titles(0), 0);
}

/// Every way a search is counted and read finds the same releases: the
/// count from what the indexes find and the one taking what a pass over the
/// titles doesn't find off the listing's counts agree, and so do the list read from what the indexes find and the one
/// testing each album as the order's index reaches it, for a search found in
/// titles, in the names the credits show, in a name a credit prints, in
/// titles and names at once, and in no text.
#[tokio::test]
async fn every_search_read_finds_the_same_releases() {
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
            let (counts, lists) = db
                .read(move |sql| {
                    let pattern = bae_util::search_text::search_pattern(text);
                    let found = found_totals(&sql, filter, text, i64::MAX)
                        .map(|(_, count, size)| (count, size));
                    let unfound = pass_totals(
                        &sql,
                        crate::storage::storage_unfound_totals_query(filter),
                        text,
                    )
                    .map_err(DbError::from)
                    .and_then(|(unfound, unfound_size)| {
                        Ok((
                            crate::storage::storage_count_on(&sql, filter)? as i64 - unfound,
                            crate::storage::storage_total_size_on(&sql, filter)? as i64
                                - unfound_size,
                        ))
                    });
                    let lists = [
                        crate::storage::StorageRows::Found,
                        crate::storage::StorageRows::SearchedInOrder,
                    ]
                    .map(|rows| {
                        let statement = crate::storage::storage_window_query(
                            &order,
                            filter,
                            rows,
                            &order.everything(),
                        );
                        order.everything().bound(u64::MAX, 0, |params| {
                            let mut params = params.to_vec();
                            params.push((":search", &pattern));
                            sql.query(&statement, params.as_slice(), |row| {
                                row.get::<_, String>("release_id")
                            })
                        })
                    });
                    let [read, tested] = lists;
                    Ok(((found?, unfound?), (read?, tested?)))
                })
                .await
                .unwrap();
            assert_eq!(counts.0, counts.1, "{text:?} {filter:?}: counts");
            assert_eq!(lists.0, lists.1, "{text:?} {filter:?}: lists");
            assert_eq!(counts.0 .0 as usize, lists.0.len(), "{text:?} {filter:?}");
            assert_eq!(
                lists.0.is_empty(),
                text == "nothing holds this",
                "{text:?} {filter:?} finds {}",
                lists.0.len()
            );
        }
    }
}

/// The ids in each window of the Storage Manager list `request` reads, as
/// the next value of `query` once it is pointed there.
async fn read_storage(
    query: &mut coven::ReconfigurableLiveQuery<
        StorageBrowseRequest,
        StorageBrowseProjection,
        bae_store::DbError,
    >,
    request: StorageBrowseRequest,
) -> (u64, Vec<Vec<String>>) {
    query.requests().set(request.clone()).unwrap();
    loop {
        let event = query.next().await.unwrap();
        if event.request == request {
            let value = event.result.unwrap();
            return (
                value.total_count,
                value
                    .windows
                    .into_iter()
                    .map(|window| window.rows.into_iter().map(|row| row.release.id).collect())
                    .collect(),
            );
        }
    }
}

/// A live Storage Manager list narrowed by a search and scrolled a window at
/// a time holds what a read with nothing to start from holds at each window;
/// and so it does after a name an album's credit prints brings the album into
/// what the search finds above the window it scrolls from, which moves no row
/// of the listing itself.
#[tokio::test]
async fn a_scrolled_search_holds_its_offsets_rows_across_a_printed_name() {
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
    let request = |offset| StorageBrowseRequest {
        sort: StorageSortCriterion {
            field: StorageSortField::AlbumTitle,
            direction: SortDirection::Ascending,
        },
        filter: StorageFilter::All,
        search: "album 1".to_string(),
        windows: LibraryPageWindows::from([LibraryPageWindow { offset, limit: 5 }]),
        selected: Default::default(),
    };
    let fresh = |offset| {
        let db = &db;
        async move {
            let mut query =
                crate::LibraryViews::new(db.clone()).subscribe_storage_browse(request(offset));
            read_storage(&mut query, request(offset)).await
        }
    };
    let mut query = crate::LibraryViews::new(db.clone()).subscribe_storage_browse(request(0));
    query.next().await.unwrap().result.unwrap();
    let (count, _) = fresh(0).await;
    assert!(count > 20, "the search finds {count} releases");
    for offset in [5, 10, 15, 10, 5] {
        assert_eq!(
            read_storage(&mut query, request(offset)).await,
            fresh(offset).await,
            "at {offset}"
        );
    }
    records
        .save_release_metadata_for_test(&bulk_library::release_id(0), |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Album 1 Artist"));
        })
        .await
        .unwrap();
    for offset in [10, 5] {
        let read = read_storage(&mut query, request(offset)).await;
        assert_eq!(read.0, count + 1, "the album printing the new name joins the list");
        assert_eq!(read, fresh(offset).await, "at {offset} after the new name");
    }
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
