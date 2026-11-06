//! The Storage Manager list: its counts, sizes and sorts as release files
//! change, its search, and every storage sort and the download queue read
//! from an index.

use crate::tests::{assert_seeks_search, both_ways, key_at, library};
use crate::*;
use bae_records::test_fixtures::aggregate_ordering::aggregate_db;
use bae_records::test_fixtures::album_listing::listed_db;
use bae_records::test_fixtures::storage_listing::*;
use bae_store::test_fixtures::exec_batch;
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
    exec_batch(&db, LIBRARY).await;
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

    exec_batch(&db, "DELETE FROM release_files WHERE id = '161dd6f3-3bdd-4312-b5d3-8b9d83b53694';")
        .await;
    for file in [
        ("f5008044-8ffe-4227-bdc3-238f85a6a4a6", RELEASE_LATE, "one.jpg", 1),
        ("cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3", RELEASE_LATE, "two.jpg", 2),
    ] {
        insert_file(&db, tmp.path(), file).await;
    }
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        13
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_EARLY, RELEASE_LATE]));
    assert_eq!(page(&db, StorageSortField::TotalSize).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    // Retitling the first album sorts its release after the second.
    exec_batch(
        &db,
        "UPDATE albums SET title = 'Album Title C' WHERE id = '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d';",
    )
    .await;
    assert_eq!(page(&db, StorageSortField::AlbumTitle).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    exec_batch(&db, "DELETE FROM releases WHERE id = 'd439555e-9883-4037-a580-48ea33c2c57b';")
        .await;
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        1
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        3
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
                !steps.iter().any(|step| step.contains("TEMP B-TREE FOR ORDER BY")),
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
    db.call(|conn| {
        conn.execute_batch(
            "INSERT INTO artist_credits (artist_id, name_key, name, created_at) \
                 VALUES ('00000000-0001-4000-8000-000000000000', 'printed', \
                         'Printed Name', '2020-01-01T00:00:00Z'); \
             UPDATE album_artists SET credit_key = 'printed' \
                 WHERE album_id = '00000000-0002-4000-8000-000000000000';",
        )
        .map_err(DbError::from)
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
    query: &mut coven::ReconfigurableLiveQuery<StorageBrowseRequest, StorageBrowseProjection>,
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
/// and so it does after renaming an artist brings an album into what the
/// search finds above the window it scrolls from, which moves no row of the
/// listing itself.
#[tokio::test]
async fn a_scrolled_search_holds_its_offsets_rows_across_a_rename() {
    let (db, _tmp) = library().await;
    // The first artist's albums sort by its sort name from now on, so its
    // rename below changes only what the search finds.
    let artist = "00000000-0001-4000-8000-000000000000";
    db.call(move |conn| {
        conn.execute("UPDATE artists SET sort_name = name WHERE id = ?1", [artist])
            .map_err(DbError::from)
    })
    .await
    .unwrap();
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
    db.call(move |conn| {
        conn.execute("UPDATE artists SET name = 'Album 1 Artist' WHERE id = ?1", [artist])
            .map_err(DbError::from)
    })
    .await
    .unwrap();
    for offset in [10, 5] {
        let read = read_storage(&mut query, request(offset)).await;
        assert_eq!(read.0, count + 1, "the renamed artist's album joins the list");
        assert_eq!(read, fresh(offset).await, "at {offset} after the rename");
    }
}

#[tokio::test]
async fn the_download_queue_seeks_its_windows_through_its_key() {
    let (db, _tmp) = library().await;
    let order = bae_transfers::tables::queue_order();
    let key = key_at(&db, &order, bae_transfers::tables::queue_window_rows_on::<()>, 150).await;
    assert_seeks_search(
        &db,
        "downloads",
        &order,
        key,
        |seek| {
            bae_transfers::tables::queue_rows_sql::<()>(
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
        row.album.artist_names,
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

/// How many rows the library's tables, the listings' triggers included,
/// change when `release` is added to Album Title One.
async fn rows_changed_adding_release(db: &Store, release: &str) -> i64 {
    let release = release.to_string();
    db.call(move |conn| {
        let before: i64 = conn.query_row("SELECT total_changes()", [], |row| row.get(0))?;
        conn.execute(
            "INSERT INTO releases (id, album_id, created_at, album_order) \
             VALUES (?1, '07d22f7d-3980-4c81-9d27-8fcd50ee3464', '2026-01-01T00:00:00Z', 1)",
            [&release],
        )?;
        let after: i64 = conn.query_row("SELECT total_changes()", [], |row| row.get(0))?;
        Ok(after - before)
    })
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
    exec_batch(
        &db,
        "UPDATE albums SET title = 'Album Title Renamed' \
         WHERE id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
    )
    .await;
    let titles: Vec<String> = db
        .call(|conn| {
            Ok(conn.query(
                "SELECT album_title FROM release_listing \
                 WHERE album_id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(titles, vec!["Album Title Renamed".to_string(); 2]);
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
}

/// Which trigger runs first as a release arrives on `conn`: whether its
/// album's listing row or its storage row is written first, in a write
/// rolled back after.
fn first_on_arrival(conn: &mut rusqlite::Connection) -> FirstOnArrival {
    let probe = conn.transaction().unwrap();
    probe
        .execute_batch(
            "CREATE TEMP TABLE arrival (step TEXT NOT NULL);
             CREATE TEMP TRIGGER arrival_listed AFTER INSERT ON main.album_listing
             BEGIN
                 INSERT INTO arrival (step) VALUES ('listing');
             END;
             CREATE TEMP TRIGGER arrival_stored AFTER INSERT ON main.release_storage
             BEGIN
                 INSERT INTO arrival (step) VALUES ('storage');
             END;
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES
                 ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES
                 ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0);",
        )
        .unwrap();
    let step: String = probe
        .query_row("SELECT step FROM arrival ORDER BY rowid LIMIT 1", [], |row| row.get(0))
        .unwrap();
    probe.rollback().unwrap();
    match step.as_str() {
        "listing" => FirstOnArrival::Listing,
        "storage" => FirstOnArrival::Storage,
        other => panic!("no trigger writes {other}"),
    }
}

/// A bare library connection on which `first`'s trigger runs first as a
/// release arrives, and that records every release whose listing row a
/// write asks to refresh, in `refreshed`.
fn counting_refreshes(first: FirstOnArrival) -> rusqlite::Connection {
    let mut conn = bae_store::schema::bare_library_connection(crate::TEST_SCHEMA);
    // SQLite runs the newest of the triggers on one event first, so creating
    // `first`'s again puts it first; the probe says whether it did.
    let created: String = conn
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'trigger' AND name = ?1",
            [first.trigger()],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute_batch(&format!("DROP TRIGGER {}; {created};", first.trigger())).unwrap();
    assert_eq!(first_on_arrival(&mut conn), first);
    conn.execute_batch(
        "CREATE TEMP TABLE refreshed (release_id TEXT NOT NULL);
         CREATE TEMP TRIGGER count_refresh INSTEAD OF INSERT ON main.release_listing_refresh
         BEGIN
             INSERT INTO refreshed (release_id) VALUES (NEW.release_id);
         END;",
    )
    .unwrap();
    conn
}

/// How many refreshes of `release`'s listing row the writes since the count
/// was last cleared asked for.
fn refreshes(conn: &rusqlite::Connection, release: &str) -> i64 {
    conn.query_row("SELECT count(*) FROM refreshed WHERE release_id = ?1", [release], |row| {
        row.get(0)
    })
    .unwrap()
}

/// The album `release`'s listing row names, None while it has none.
fn listed_album(conn: &rusqlite::Connection, release: &str) -> Option<String> {
    conn.query_row(
        "SELECT (SELECT album_id FROM release_listing WHERE release_id = ?1)",
        [release],
        |row| row.get(0),
    )
    .unwrap()
}

/// Run `sql` as one write.
fn write(conn: &mut rusqlite::Connection, sql: &str) {
    let write = conn.transaction().unwrap();
    write.execute_batch(sql).unwrap();
    write.commit().unwrap();
}

/// A release arriving writes its listing row with one refresh, in the write
/// that adds it: as a snapshot installs it, after its album, which lists that
/// album too, and as it joins an album already listed.
#[test]
fn a_release_arriving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let mut conn = counting_refreshes(first);

        write(
            &mut conn,
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
                 ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES
                 ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0);",
        );
        assert_eq!(listed_album(&conn, RELEASE_EARLY).as_deref(), Some(ALBUM_A), "{first:?} first");
        assert_eq!(refreshes(&conn, RELEASE_EARLY), 1, "{first:?} first");

        write(
            &mut conn,
            "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
                 ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 1);",
        );
        assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_A), "{first:?} first");
        assert_eq!(refreshes(&conn, RELEASE_LATE), 1, "{first:?} first");
        assert_eq!(refreshes(&conn, RELEASE_EARLY), 1, "{first:?} first");
    }
}

/// A release moving writes its listing row with one refresh: to an album the
/// same write adds, which lists that album and its releases, and to an album
/// already listed.
#[test]
fn a_release_moving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let mut conn = counting_refreshes(first);
        write(
            &mut conn,
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
                 ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES
                 ('d439555e-9883-4037-a580-48ea33c2c57b', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-01T00:00:00Z', 0),
                 ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 1);
             DELETE FROM refreshed;",
        );

        write(
            &mut conn,
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
                 ('dab6efe2-9b5c-4034-b95e-73cf49f54b9a', 'Album Title B', 0, '2026-01-03T00:00:00Z');
             UPDATE releases SET album_id = 'dab6efe2-9b5c-4034-b95e-73cf49f54b9a'
             WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
        );
        assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_B), "{first:?} first");
        assert_eq!(refreshes(&conn, RELEASE_LATE), 1, "{first:?} first");

        write(
            &mut conn,
            "DELETE FROM refreshed;
             UPDATE releases SET album_id = '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d'
             WHERE id = 'af4421de-5174-45a9-81fc-74110bc5f85d';",
        );
        assert_eq!(listed_album(&conn, RELEASE_LATE).as_deref(), Some(ALBUM_A), "{first:?} first");
        assert_eq!(refreshes(&conn, RELEASE_LATE), 1, "{first:?} first");
    }
}

/// `release`'s listing row's media columns.
fn listed_media(conn: &rusqlite::Connection, release: &str) -> (bool, Option<String>) {
    conn.query_row(
        "SELECT media_empty, media_key FROM release_listing WHERE release_id = ?1",
        [release],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

/// A release's media rows arriving, changing and leaving set its listing
/// row's media, in order, without refreshing the rest of the row.
#[test]
fn a_release_s_media_rows_set_its_listing_s_media() {
    for first in FirstOnArrival::BOTH {
        let mut conn = counting_refreshes(first);
        write(
            &mut conn,
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES
                 ('4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', 'Album Title A', 0, '2026-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES
                 ('af4421de-5174-45a9-81fc-74110bc5f85d', '4991dfdb-eb0e-4ed7-b659-47cda1dbfb7d', '2026-01-02T00:00:00Z', 0);
             INSERT INTO release_media (id, release_id, position, medium, count) VALUES
                 ('media-1', 'af4421de-5174-45a9-81fc-74110bc5f85d', 1, 'cd', 1),
                 ('media-0', 'af4421de-5174-45a9-81fc-74110bc5f85d', 0, 'vinyl', 2);",
        );
        assert_eq!(refreshes(&conn, RELEASE_LATE), 1, "{first:?} first");
        assert_eq!(
            listed_media(&conn, RELEASE_LATE),
            (false, Some("vinyl:00002,cd:00001".to_string()))
        );

        write(
            &mut conn,
            "DELETE FROM refreshed;
             UPDATE release_media SET count = 3 WHERE id = 'media-0';",
        );
        assert_eq!(
            listed_media(&conn, RELEASE_LATE),
            (false, Some("vinyl:00003,cd:00001".to_string()))
        );

        write(&mut conn, "DELETE FROM release_media;");
        assert_eq!(listed_media(&conn, RELEASE_LATE), (true, None));
        assert_eq!(refreshes(&conn, RELEASE_LATE), 0, "{first:?} first");
    }
}

/// A release's media rows written on one device sort the release in the
/// Storage Manager's listing on another: the listing's media key follows the
/// rows, in order.
#[test]
fn a_media_edit_from_another_device_sorts_its_release() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let release = "0252dedb-ee39-4547-8803-438dbeb57a64";
        devices.a().call(seed_artist_album_release).await.unwrap();
        exec_batch(
            devices.a(),
            &format!(
                "INSERT INTO release_media (id, release_id, position, medium, count) VALUES
                     ('media-1', '{release}', 1, 'cd', 1),
                     ('media-0', '{release}', 0, 'vinyl', 2);"
            ),
        )
        .await;
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
