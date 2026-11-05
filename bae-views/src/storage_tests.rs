//! The Storage Manager list: its counts, sizes and sorts as release files
//! change, its search, and every storage sort and the download queue read
//! from an index.

use crate::tests::{assert_seeks_search, both_ways, key_at, library};
use crate::*;
use bae_db::test_fixtures::aggregate_ordering::aggregate_db;
use bae_db::test_fixtures::exec_batch;
use bae_db::test_fixtures::storage_listing::*;
use bae_db::test_fixtures::*;
use bae_db::{LibraryPageWindow, LibraryPageWindows};

/// The Storage Manager list's release ids in `field` order, ascending.
async fn page(db: &Database, field: StorageSortField) -> Vec<String> {
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
    let (db, tmp) = bae_db::test_fixtures::temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
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
    let (db, _tmp) = bae_db::test_fixtures::temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
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
    vec![(":search", rusqlite::types::Value::Text(bae_db::schema::search_pattern(text)))]
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
            ":search": bae_db::schema::search_pattern(text),
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
        rusqlite::named_params! { ":search": bae_db::schema::search_pattern(text) },
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
                    let pattern = bae_db::schema::search_pattern(text);
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
    let (db, _tmp) = aggregate_db().await;
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
