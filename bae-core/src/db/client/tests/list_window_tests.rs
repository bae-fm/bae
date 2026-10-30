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
    let sql = super::window_cost::seed_sql(0, 300);
    db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from))
        .await
        .unwrap();
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
        ] {
            for (field, direction) in both_ways(field) {
                let order = storage_order(&StorageSortCriterion { field, direction });
                // Under Remote no release is listed, so the middle row of
                // every release stands in for one.
                let key = key_at(
                    &db,
                    &order,
                    |sql, order, seek, limit, offset| {
                        storage_rows_on(sql, order, StorageFilter::All, seek, limit, offset)
                    },
                    150,
                )
                .await;
                assert_seeks_search(
                    &db,
                    &format!("storage {filter:?} {field:?} {direction:?}"),
                    &order,
                    key,
                    |seek| storage_window_query(&order, filter, seek),
                    Vec::new(),
                )
                .await;
            }
        }
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
