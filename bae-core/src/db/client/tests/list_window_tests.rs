//! The paged lists' windows read from a row beside them: every seek of
//! every sort an index search, and a live list read from its last read's rows
//! only while none of them has moved.

use super::super::list_windows::OrderKey;
use super::super::*;
use crate::library::{LibraryPageWindow, LibraryPageWindows};

/// The plan of `statement` under `seek`'s parameters and `extra`, one line
/// per step.
async fn plan(
    db: &Database,
    statement: String,
    seek: Seek,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) -> Vec<String> {
    db.read(move |sql| {
        seek.bound(50, 0, |params| {
            let mut params = params.to_vec();
            params.extend(
                extra
                    .iter()
                    .map(|(name, value)| (*name, value as &dyn rusqlite::ToSql)),
            );
            Ok(sql.query(
                &format!("EXPLAIN QUERY PLAN {statement}"),
                params.as_slice(),
                |row| row.get::<_, String>(3),
            )?)
        })
    })
    .await
    .unwrap()
}

/// The aliases the lists' statements read their rows as, which a seek reads
/// by an index search, never a scan.
const LIST_ALIASES: &[&str] = &["listing", "artist", "link", "a", "r", "q"];

/// Hold every seek beside the row at `key` of `order`'s rows to an index:
/// `statement` builds the seek's statement. Only the grid grouped by artist
/// sorts, and only one artist's albums at a time: each artist's as it
/// reaches it (`LAST … TERMS`), or, holding the artist's sort key and id (the
/// first `sorted_within` terms) equal, that artist's alone.
async fn assert_seeks_search(
    db: &Database,
    what: &str,
    order: &ListOrder,
    key: OrderKey,
    statement: impl Fn(&Seek) -> String,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) {
    assert_seeks_search_sorting(db, what, order, key, statement, extra, usize::MAX).await
}

async fn assert_seeks_search_sorting(
    db: &Database,
    what: &str,
    order: &ListOrder,
    key: OrderKey,
    statement: impl Fn(&Seek) -> String,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
    sorted_within: usize,
) {
    for seek in order.seeks_beside(&key) {
        let sorts_by_artist = sorted_within != usize::MAX;
        let sorts = seek.held_equal() >= sorted_within;
        let steps = plan(db, statement(&seek), seek.clone(), extra.clone()).await;
        // The grid grouped by artist sorts each artist's albums as it reaches
        // the artist (`RIGHT PART`); nothing sorts the list whole.
        assert!(
            !steps.iter().any(|step| {
                LIST_ALIASES.iter().any(|alias| {
                    step.starts_with(&format!("SCAN {alias} ")) || step == &format!("SCAN {alias}")
                }) || (!sorts && step.contains("TEMP B-TREE FOR ORDER BY"))
                    || (!sorts_by_artist && step.contains("TEMP B-TREE FOR LAST"))
            }),
            "{what} {seek:?}: {steps:?}"
        );
    }
}

/// A library of 300 albums, each with an artist, a release, files and a
/// work, every release queued to download and every track in the lane.
async fn library() -> (Database, tempfile::TempDir) {
    let (db, tmp) = super::temp_db().await;
    super::window_cost::seed(&db, &tmp.path().join("originals"), 0, 300).await;
    (db, tmp)
}

fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [
        (field, SortDirection::Ascending),
        (field, SortDirection::Descending),
    ]
}

/// The key of row `at` of `order`, read by `rows`.
async fn key_at<R: Send + 'static>(
    db: &Database,
    order: &ListOrder,
    rows: impl Fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>
        + Send
        + 'static,
    at: u64,
) -> OrderKey {
    let order = order.clone();
    db.read(move |sql| {
        Ok(rows(&sql, &order, &order.everything(), 1, at)?
            .pop()
            .unwrap()
            .1)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn every_album_sort_seeks_its_windows_through_an_index() {
    let (db, _tmp) = library().await;
    let mut sorts = vec![Vec::new()];
    for field in [
        AlbumSortField::Title,
        AlbumSortField::Artist,
        AlbumSortField::Year,
        AlbumSortField::DateAdded,
    ] {
        for (field, direction) in both_ways(field) {
            sorts.push(vec![AlbumSortCriterion { field, direction }]);
        }
    }
    for group_by_artist in [false, true] {
        for sort in &sorts {
            let (relation, order) = album_browse_order(sort, group_by_artist);
            let read = relation.clone();
            let key = key_at(
                &db,
                &order,
                move |sql, order, seek, limit, offset| {
                    super::super::album::album_rows_on(sql, &read, order, seek, limit, offset)
                },
                150,
            )
            .await;
            // Grouped by artist, a seek holding the artist's sort key and id
            // reads that artist's albums alone.
            let sorted_within = if group_by_artist { 2 } else { usize::MAX };
            assert_seeks_search_sorting(
                &db,
                &format!("albums {sort:?} grouped {group_by_artist}"),
                &order,
                key,
                |seek| super::super::album::album_window_statement(&relation, &order, seek),
                Vec::new(),
                sorted_within,
            )
            .await;
        }
    }
}

#[tokio::test]
async fn every_artist_and_composer_sort_seeks_its_windows_through_an_index() {
    let (db, _tmp) = library().await;
    let mut artist_sorts = vec![Vec::new()];
    for (field, direction) in both_ways(ArtistSortField::Name)
        .into_iter()
        .chain(both_ways(ArtistSortField::AlbumCount))
    {
        artist_sorts.push(vec![ArtistSortCriterion { field, direction }]);
    }
    for sort in artist_sorts {
        let order = artist_order(&sort);
        let key = key_at(&db, &order, super::super::artist::artist_rows_on, 150).await;
        assert_seeks_search(
            &db,
            &format!("artists {sort:?}"),
            &order,
            key,
            |seek| listing_window_query(artist_listing_query, &order, seek),
            Vec::new(),
        )
        .await;
    }
    let mut composer_sorts = vec![Vec::new()];
    for field in [
        ComposerSortField::Name,
        ComposerSortField::WorkCount,
        ComposerSortField::LinkedReleaseCount,
    ] {
        for (field, direction) in both_ways(field) {
            composer_sorts.push(vec![ComposerSortCriterion { field, direction }]);
        }
    }
    for sort in composer_sorts {
        let order = composer_order(&sort);
        let key = key_at(&db, &order, super::super::artist::composer_rows_on, 150).await;
        assert_seeks_search(
            &db,
            &format!("composers {sort:?}"),
            &order,
            key,
            |seek| listing_window_query(composer_listing_query, &order, seek),
            Vec::new(),
        )
        .await;
    }
}

#[tokio::test]
async fn every_storage_sort_and_filter_seeks_its_windows_through_an_index() {
    let (db, _tmp) = library().await;
    for filter in [
        StorageFilter::All,
        StorageFilter::Local,
        StorageFilter::Remote,
    ] {
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
                let order = storage_order(&StorageSortCriterion { field, direction });
                // Under Remote no release is listed, so the middle row of
                // every release stands in for one.
                let key = key_at(
                    &db,
                    &order,
                    |sql, order, seek, limit, offset| {
                        storage_rows_on(sql, order, StorageFilter::All, None, seek, limit, offset)
                    },
                    150,
                )
                .await;
                assert_seeks_search(
                    &db,
                    &format!("storage {filter:?} {field:?} {direction:?}"),
                    &order,
                    key,
                    |seek| storage_window_query(&order, filter, StorageRows::Listed, seek),
                    Vec::new(),
                )
                .await;
            }
        }
    }
}

/// The aliases a searching Storage Manager statement reads what the search
/// finds through, beside the list's: never scanned whole.
const SEARCH_ALIASES: &[&str] = &["credit", "shown", "listing", "r", "a"];

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
    vec![(
        ":search",
        rusqlite::types::Value::Text(crate::library::search::search_pattern(text)),
    )]
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
    for filter in [
        StorageFilter::All,
        StorageFilter::Local,
        StorageFilter::Remote,
    ] {
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
                let order = storage_order(&StorageSortCriterion { field, direction });
                let key = key_at(
                    &db,
                    &order,
                    |sql, order, seek, limit, offset| {
                        storage_rows_on(sql, order, StorageFilter::All, None, seek, limit, offset)
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
                    |seek| storage_window_query(&order, filter, StorageRows::SearchedInOrder, seek),
                    search.clone(),
                )
                .await;
                for seek in order.seeks_beside(&key) {
                    let statement =
                        storage_window_query(&order, filter, StorageRows::SearchedInOrder, &seek);
                    let steps = plan(&db, statement, seek.clone(), search.clone()).await;
                    assert!(
                        reads_search_text_by_entry(&steps),
                        "in order {what} {seek:?}: {steps:?}"
                    );
                    let statement = storage_window_query(&order, filter, StorageRows::Found, &seek);
                    let steps = plan(&db, statement, seek.clone(), search.clone()).await;
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
        let steps = read(storage_found_totals_query(filter), counted).await;
        assert!(
            !scans(&steps, SEARCH_ALIASES),
            "count {filter:?}: {steps:?}"
        );
        // The pass over the titles reads every title once, and nothing else
        // whole.
        let steps = read(storage_unfound_totals_query(filter), search.clone()).await;
        assert!(
            !scans(&steps, SEARCH_ALIASES)
                && steps
                    .iter()
                    .filter(|step| step.ends_with("VIRTUAL TABLE INDEX 0:"))
                    .eq(["SCAN untitled VIRTUAL TABLE INDEX 0:"].iter()),
            "unfound {filter:?}: {steps:?}"
        );
        let mut selected = search.clone();
        selected.push((
            ":selected",
            rusqlite::types::Value::Text(r#"["a", "b"]"#.to_string()),
        ));
        let steps = read(storage_selected_query(filter, true), selected).await;
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
        &storage_found_totals_query(filter),
        rusqlite::named_params! {
            ":search": crate::library::search::search_pattern(text),
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
        rusqlite::named_params! { ":search": crate::library::search::search_pattern(text) },
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
                let query = crate::library::LibrarySearchQuery::parse(text).unwrap();
                let titles = storage_found_titles(300) as i64;
                let (counted, _, _) = found_totals(&sql, StorageFilter::All, text, titles)?;
                let (found, _, search) = storage_totals_on(&sql, StorageFilter::All, Some(&query))?;
                Ok((found, counted, search.unwrap().rows(50, 0)))
            })
            .await
            .unwrap()
        }
    };
    // The second album's title, "Album 16807", is the only text holding it.
    assert_eq!(read_at_top("16807").await, (1, true, StorageRows::Found));
    assert_eq!(
        read_at_top("album").await,
        (300, false, StorageRows::SearchedInOrder)
    );
}

/// A search is counted through the indexes while its pattern is in fewer
/// titles than a pass over every title costs, about a fifth of them.
#[test]
fn a_search_in_a_fifth_of_the_titles_is_counted_by_a_pass() {
    assert_eq!(storage_found_titles(100_000), 21_052);
    assert_eq!(storage_found_titles(0), 0);
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
    let order = storage_order(&StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    });
    for text in ["album 1", "artist 0", "printed", "1", "nothing holds this"] {
        for filter in [StorageFilter::All, StorageFilter::Local] {
            let order = order.clone();
            let (counts, lists) = db
                .read(move |sql| {
                    let pattern = crate::library::search::search_pattern(text);
                    let found = found_totals(&sql, filter, text, i64::MAX)
                        .map(|(_, count, size)| (count, size));
                    let unfound = pass_totals(&sql, storage_unfound_totals_query(filter), text)
                        .map_err(DbError::from)
                        .and_then(|(unfound, unfound_size)| {
                            Ok((
                                storage_count_on(&sql, filter)? as i64 - unfound,
                                storage_total_size_on(&sql, filter)? as i64 - unfound_size,
                            ))
                        });
                    let lists = [StorageRows::Found, StorageRows::SearchedInOrder].map(|rows| {
                        let statement =
                            storage_window_query(&order, filter, rows, &order.everything());
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
        conn.execute(
            "UPDATE artists SET sort_name = name WHERE id = ?1",
            [artist],
        )
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
            let mut query = db.subscribe_storage_browse(request(offset));
            read_storage(&mut query, request(offset)).await
        }
    };
    let mut query = db.subscribe_storage_browse(request(0));
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
        conn.execute(
            "UPDATE artists SET name = 'Album 1 Artist' WHERE id = ?1",
            [artist],
        )
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    for offset in [10, 5] {
        let read = read_storage(&mut query, request(offset)).await;
        assert_eq!(
            read.0,
            count + 1,
            "the renamed artist's album joins the list"
        );
        assert_eq!(read, fresh(offset).await, "at {offset} after the rename");
    }
}

#[tokio::test]
async fn the_download_queue_seeks_its_windows_through_its_key() {
    let (db, _tmp) = library().await;
    let order = super::super::release_queues::queue_order();
    let key = key_at(
        &db,
        &order,
        super::super::release_queues::queue_window_rows_on::<()>,
        150,
    )
    .await;
    assert_seeks_search(
        &db,
        "downloads",
        &order,
        key,
        |seek| {
            super::super::release_queues::queue_rows_sql::<()>(
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

/// The album list's ids at `window`, as a read by offset finds them.
async fn albums_at(db: &Database, window: &LibraryPageWindow) -> Vec<String> {
    db.get_album_page(&[], window.offset, window.limit)
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.id)
        .collect()
}

/// The next value of the album list's live query once `windows` are asked
/// for, as each window's ids.
async fn read_albums(
    query: &mut coven::ReconfigurableLiveQuery<LibraryPageWindows, AlbumBrowseProjection>,
    windows: LibraryPageWindows,
) -> Vec<Vec<String>> {
    query.requests().set(windows.clone()).unwrap();
    loop {
        let event = query.next().await.unwrap();
        if event.request == windows {
            return event
                .result
                .unwrap()
                .windows
                .into_iter()
                .map(|window| window.rows.into_iter().map(|album| album.id).collect())
                .collect();
        }
    }
}

/// A live album list scrolled a window at a time, down then back up, holds
/// what reads by offset hold at each window; and after a write adds an
/// album above the window it scrolls from, the next window still holds what
/// a read by offset does, not the rows past where the last read's row sat
/// before the write.
#[tokio::test]
async fn a_scrolled_album_list_holds_its_offsets_rows_across_a_write() {
    let (db, _tmp) = library().await;
    let window = |offset| LibraryPageWindow { offset, limit: 50 };
    let mut query = db.subscribe_album_browse(&[], false, LibraryPageWindows::new());
    query.next().await.unwrap().result.unwrap();
    for offset in [100, 150, 200, 150, 100] {
        assert_eq!(
            read_albums(&mut query, LibraryPageWindows::from([window(offset)])).await,
            vec![albums_at(&db, &window(offset)).await],
            "at {offset}"
        );
    }
    // The newest album sorts first in the default order.
    db.call(|conn| {
        conn.execute_batch(
            "INSERT INTO albums (id, title, created_at) VALUES \
                 ('f0000000-0000-4000-8000-000000000001', 'Album Title', '2099-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, album_order, created_at) VALUES \
                 ('f0000000-0000-4000-8000-000000000002', 'f0000000-0000-4000-8000-000000000001', \
                  0, '2099-01-01T00:00:00Z');",
        )
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    for offset in [150, 50] {
        assert_eq!(
            read_albums(&mut query, LibraryPageWindows::from([window(offset)])).await,
            vec![albums_at(&db, &window(offset)).await],
            "at {offset} after the write"
        );
    }
}

/// The import list's items in the windows `request` names, as one read with
/// no earlier read's rows to start from finds them.
async fn import_items(
    db: &Database,
    request: crate::import::ImportListRequest,
) -> Vec<Vec<crate::import::list::ImportListItem>> {
    db.load_import_list(request)
        .await
        .unwrap()
        .windows
        .into_iter()
        .map(|window| window.items)
        .collect()
}

/// A live import list of grouped folders, scrolled a window at a time down
/// and back up, holds at each window what a read with nothing to start from
/// holds, its members' headers included; and so it does after a scan adds
/// folders above the window it scrolls from.
#[tokio::test(flavor = "multi_thread")]
async fn a_scrolled_import_list_holds_its_offsets_items_across_a_scan() {
    use crate::import::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    let (db, tmp) = super::empty_db().await;
    super::window_cost::grow_import_list(&db, tmp.path(), 0, 300).await;
    let list = db
        .open_import_list(ImportListView {
            tab: crate::import::TriageTab::Pending,
            filter_text: String::new(),
            pending_filter: PendingFilter::All,
            // The folders a later scan adds sort first.
            order: ImportListOrder::PathDescending,
        })
        .await
        .unwrap();
    let request = |offset| ImportListRequest {
        list,
        windows: LibraryPageWindows::from([LibraryPageWindow { offset, limit: 10 }]),
    };
    let mut query = db.subscribe_import_list(request(0));
    query.next().await.unwrap().result.unwrap();
    let requests = query.requests();
    let mut scroll = async |offset: u64| {
        requests.set(request(offset)).unwrap();
        loop {
            let event = query.next().await.unwrap();
            if event.request == request(offset) {
                return event
                    .result
                    .unwrap()
                    .windows
                    .into_iter()
                    .map(|window| window.items)
                    .collect::<Vec<_>>();
            }
        }
    };
    for offset in [10, 20, 30, 40, 30, 20, 10] {
        assert_eq!(
            scroll(offset).await,
            import_items(&db, request(offset)).await,
            "at {offset}"
        );
    }
    super::window_cost::grow_import_list(&db, tmp.path(), 300, 305).await;
    for offset in [30, 20] {
        assert_eq!(
            scroll(offset).await,
            import_items(&db, request(offset)).await,
            "at {offset} after the scan"
        );
    }
}
