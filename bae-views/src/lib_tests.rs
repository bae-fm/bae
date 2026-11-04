//! What crosses the views: an album's credits are one ordered list
//! (`album_artists` from position 0) that every reader of an album's artists
//! reads whole, and the paged lists' windows read from a row beside them,
//! every seek of every sort an index search. The list-window helpers here
//! serve every list's tests.

use crate::*;
use bae_db::client::list_windows::OrderKey;
use bae_db::client::list_windows::{Keyed, ListOrder, Seek};
use bae_db::test_fixtures::{exec_batch, temp_db};

const ARTIST_A: &str = "3d0f8a52-6c1e-4b8e-9a51-0f6f1e2c7a01";
const ARTIST_B: &str = "3d0f8a52-6c1e-4b8e-9a51-0f6f1e2c7a02";
const ARTIST_C: &str = "3d0f8a52-6c1e-4b8e-9a51-0f6f1e2c7a03";
const ARTIST_VARIOUS: &str = "3d0f8a52-6c1e-4b8e-9a51-0f6f1e2c7a04";
const ALBUM_A: &str = "5a7c2e10-91b3-4f6d-8e2a-1c0b9d8e7f01";
const ALBUM_AB: &str = "5a7c2e10-91b3-4f6d-8e2a-1c0b9d8e7f02";
const ALBUM_AC: &str = "5a7c2e10-91b3-4f6d-8e2a-1c0b9d8e7f03";
const ALBUM_VARIOUS: &str = "5a7c2e10-91b3-4f6d-8e2a-1c0b9d8e7f04";

/// Artists A, B, C and Various Artists, and four albums: "Third" credited to
/// A alone, "First" to A then B, "Second" to A then C (titles that do not
/// sort the albums the way their credits do), and a compilation credited to
/// Various Artists whose one track carries its own credit, C, and whose other
/// track has none. Each album holds one release.
async fn credited_library() -> (Database, tempfile::TempDir) {
    let (db, dir) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES \
                ('{ARTIST_A}', 'Artist A', 'artist a', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_B}', 'Artist B', 'artist b', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_C}', 'Artist C', 'artist c', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_VARIOUS}', 'Various Artists', 'various artists', '2026-01-01T00:00:00Z'); \
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES \
                ('{ALBUM_A}', 'Third', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_AB}', 'First', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_AC}', 'Second', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_VARIOUS}', 'Compilation', 1, '2026-01-01T00:00:00Z'); \
             INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c01', '{ALBUM_A}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c02', '{ALBUM_AB}', '{ARTIST_B}', 1, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c03', '{ALBUM_AB}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c04', '{ALBUM_AC}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c05', '{ALBUM_AC}', '{ARTIST_C}', 1, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c06', '{ALBUM_VARIOUS}', '{ARTIST_VARIOUS}', 0, '2026-01-01T00:00:00Z'); \
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e01', '{ALBUM_A}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e02', '{ALBUM_AB}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e03', '{ALBUM_AC}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', '{ALBUM_VARIOUS}', '2026-01-01T00:00:00Z', 0); \
             INSERT INTO release_mediums (id, release_id, position, side_count, created_at) VALUES \
                ('c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 1, 1, '2026-01-01T00:00:00Z'); \
             INSERT INTO tracks (id, release_id, position, title, medium_id, side, track_number, duration_ms, created_at) VALUES \
                ('d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e01', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 0, 'Track Credited', 'c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', 1, 1, 1000, '2026-01-01T00:00:00Z'), \
                ('d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e02', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 1, 'Track Uncredited', 'c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', 1, 2, 1000, '2026-01-01T00:00:00Z'); \
             INSERT INTO track_artists (id, track_id, artist_id, position, created_at) VALUES \
                ('e5c7a9b1-2d4f-4b6d-8f2b-4d6f8b0d2f01', 'd4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e01', '{ARTIST_C}', 0, '2026-01-01T00:00:00Z');"
        ),
    )
    .await;
    (db, dir)
}

#[tokio::test]
async fn an_album_is_found_by_any_of_its_credits() {
    let (db, _dir) = credited_library().await;

    let results = LibraryViews::new(db.clone()).search_rows("Artist B", 10).await.unwrap();

    let albums: Vec<(&str, &str)> = results
        .albums
        .iter()
        .map(|album| (album.id.as_str(), album.artist_name.as_str()))
        .collect();
    assert_eq!(albums, [(ALBUM_AB, "Artist A, Artist B")]);
}

#[tokio::test]
async fn a_track_search_row_names_its_own_credits_else_its_albums() {
    let (db, _dir) = credited_library().await;

    let results = LibraryViews::new(db.clone()).search_rows("Track", 10).await.unwrap();

    let tracks: Vec<(&str, &str)> = results
        .tracks
        .iter()
        .map(|track| (track.title.as_str(), track.artist_name.as_str()))
        .collect();
    assert_eq!(tracks, [("Track Credited", "Artist C"), ("Track Uncredited", "Various Artists"),]);
}

/// "A" sorts before "A & B", which sorts before "A & C", whatever the titles
/// say, in the storage table and in the album grid alike.
#[tokio::test]
async fn sorting_by_artist_compares_every_credit_in_order() {
    let (db, _dir) = credited_library().await;

    let storage = LibraryViews::new(db.clone())
        .get_storage_page(
            &StorageSortCriterion {
                field: StorageSortField::ArtistNames,
                direction: SortDirection::Ascending,
            },
            StorageFilter::All,
            0,
            10,
        )
        .await
        .unwrap();
    let grid = LibraryViews::new(db.clone())
        .get_album_page(
            &[AlbumSortCriterion {
                field: AlbumSortField::Artist,
                direction: SortDirection::Ascending,
            }],
            0,
            10,
        )
        .await
        .unwrap();

    let expected = [ALBUM_A, ALBUM_AB, ALBUM_AC, ALBUM_VARIOUS];
    assert_eq!(storage.rows.iter().map(|row| row.album.id.as_str()).collect::<Vec<_>>(), expected);
    assert_eq!(grid.iter().map(|album| album.id.as_str()).collect::<Vec<_>>(), expected);
}

/// The plan of `statement` under `seek`'s parameters and `extra`, one line
/// per step.
pub(super) async fn plan(
    db: &Database,
    statement: String,
    seek: Seek,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) -> Vec<String> {
    db.read(move |sql| {
        seek.bound(50, 0, |params| {
            let mut params = params.to_vec();
            params.extend(extra.iter().map(|(name, value)| (*name, value as &dyn rusqlite::ToSql)));
            Ok(sql.query(&format!("EXPLAIN QUERY PLAN {statement}"), params.as_slice(), |row| {
                row.get::<_, String>(3)
            })?)
        })
    })
    .await
    .unwrap()
}

/// The aliases the lists' statements read their rows as, which a seek reads
/// by an index search, never a scan.
pub(super) const LIST_ALIASES: &[&str] = &["listing", "artist", "link", "a", "r", "q"];

/// Hold every seek beside the row at `key` of `order`'s rows to an index:
/// `statement` builds the seek's statement. Only the grid grouped by artist
/// sorts, and only one artist's albums at a time: each artist's as it
/// reaches it (`LAST … TERMS`), or, holding the artist's sort key and id (the
/// first `sorted_within` terms) equal, that artist's alone.
pub(super) async fn assert_seeks_search(
    db: &Database,
    what: &str,
    order: &ListOrder,
    key: OrderKey,
    statement: impl Fn(&Seek) -> String,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) {
    assert_seeks_search_sorting(db, what, order, key, statement, extra, usize::MAX).await
}

pub(super) async fn assert_seeks_search_sorting(
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
pub(super) async fn library() -> (Database, tempfile::TempDir) {
    let (db, tmp) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    window_cost::seed(&db, &tmp.path().join("originals"), 0, 300).await;
    (db, tmp)
}

pub(super) fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [(field, SortDirection::Ascending), (field, SortDirection::Descending)]
}

/// The key of row `at` of `order`, read by `rows`.
pub(super) async fn key_at<R: Send + 'static>(
    db: &Database,
    order: &ListOrder,
    rows: impl Fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>
        + Send
        + 'static,
    at: u64,
) -> OrderKey {
    let order = order.clone();
    db.read(move |sql| Ok(rows(&sql, &order, &order.everything(), 1, at)?.pop().unwrap().1))
        .await
        .unwrap()
}

#[tokio::test]
async fn album_and_storage_pages_list_credits_in_order_and_allow_none() {
    let (db, _tmp) = bae_db::test_fixtures::temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    db.call(|conn| {
        conn.execute_batch(
            "
            INSERT INTO artists (id, name, name_key, created_at) VALUES ('7e7d8df5-8292-4287-80be-7abd24f5a992', 'Artist Name First', 'artist name first', '2026-01-01T00:00:00Z'),
             ('863911c6-a6b6-40a7-8096-b85eb877f7c7', 'Artist Name Second', 'artist name second', '2026-01-01T00:00:00Z');

            INSERT INTO albums (id, title, year, is_compilation, created_at) VALUES ('f6506bc5-0b41-44de-862f-1668e72c08c6', 'Album Title Empty', 2026, 0, '2026-01-01T00:00:00Z'),
             ('049faa5b-52d9-4109-832b-f6853740c876', 'Album Title Extra', 2026, 0, '2026-01-01T00:00:00Z');

            INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('c9966a2c-61b1-40dd-87ef-118587e57fe7', '049faa5b-52d9-4109-832b-f6853740c876', '863911c6-a6b6-40a7-8096-b85eb877f7c7', 1, '2026-01-01T00:00:00Z'),
             ('4953a209-c6c7-4b0c-82dd-cc13e47af890', '049faa5b-52d9-4109-832b-f6853740c876', '7e7d8df5-8292-4287-80be-7abd24f5a992', 0, '2026-01-01T00:00:00Z');

            INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('7c3d0881-e6d0-4252-8075-709b2282bcc1', 'f6506bc5-0b41-44de-862f-1668e72c08c6', '2026-01-01T00:00:00Z', 0),
             ('ba00ebe0-da50-428a-8ceb-2389d9a9f232', '049faa5b-52d9-4109-832b-f6853740c876', '2026-01-01T00:00:00Z', 1);
            ",
        )
        .map(|_| ())
        .map_err(DbError::from)
    })
    .await
    .unwrap();

    let album_sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let albums =
        crate::LibraryViews::new(db.clone()).get_album_page(&album_sort, 0, 10).await.unwrap();
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].artist_names, "");
    assert_eq!(albums[1].artist_names, "Artist Name First, Artist Name Second");

    let storage_sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let rows = crate::LibraryViews::new(db.clone())
        .get_storage_page(&storage_sort, StorageFilter::All, 0, 10)
        .await
        .unwrap()
        .rows;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].album.artist_names, "");
    assert_eq!(rows[1].album.artist_names, "Artist Name First, Artist Name Second");
}

#[tokio::test]
async fn every_artist_and_composer_sort_seeks_its_windows_through_an_index() {
    let (db, _tmp) = library().await;
    let mut artist_sorts = vec![Vec::new()];
    for (field, direction) in
        both_ways(ArtistSortField::Name).into_iter().chain(both_ways(ArtistSortField::AlbumCount))
    {
        artist_sorts.push(vec![ArtistSortCriterion { field, direction }]);
    }
    for sort in artist_sorts {
        let order = crate::artist::artist_order(&sort);
        let key = key_at(&db, &order, crate::artist::artist_rows_on, 150).await;
        assert_seeks_search(
            &db,
            &format!("artists {sort:?}"),
            &order,
            key,
            |seek| crate::listing_window_query(crate::artist::artist_listing_query, &order, seek),
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
        let order = crate::composer::composer_order(&sort);
        let key = key_at(&db, &order, crate::composer::composer_rows_on, 150).await;
        assert_seeks_search(
            &db,
            &format!("composers {sort:?}"),
            &order,
            key,
            |seek| {
                crate::listing_window_query(crate::composer::composer_listing_query, &order, seek)
            },
            Vec::new(),
        )
        .await;
    }
}

/// What one window of each paged list costs to read, at the top, the middle
/// and the end of the list, at 20,000, 50,000 and 100,000 rows: reached by a
/// jump, which reads it by its offset, and by scrolling, which reads it from
/// the row beside it (`list_windows`): the row after the window at the top,
/// and the row before it lower down. Each window is read by the reads its live
/// query runs, its rows decoded as that query decodes them. At 100,000 rows
/// the Storage Manager list is also measured narrowed by a search that finds
/// nearly every release and by one that finds few. A measurement, not
/// a check: run it in release with
///
/// ```sh
/// scripts/cargo.sh test --release -p bae-views --features test-utils --lib -- \
///     window_cost --ignored --nocapture --test-threads 1
/// ```
mod window_cost {
    use bae_db::test_fixtures::window_cost::*;

    use crate::*;
    use std::time::{Duration, Instant};

    /// A library row's id: a UUID naming its table by `kind` and its row by `i`,
    /// as an SQL expression over `i`.
    fn id(kind: u32) -> String {
        format!("printf('%08x-{kind:04x}-4000-8000-%012x', i, i)")
    }

    /// Rows `start..end` of every table the lists read but the files: an artist,
    /// an album crediting it with one release, one medium and one track, a work
    /// the artist composed, a download queued for the release and the track in
    /// the context lane. Titles and names run in an order of their own,
    /// unrelated to the ids', and every sort has ties the ids break.
    fn seed_sql(start: u64, end: u64) -> String {
        let rows = format!(
            "WITH RECURSIVE k(i) AS (SELECT {start} UNION ALL SELECT i + 1 FROM k WHERE i + 1 < {end})"
        );
        let (artist, album, release, credit, medium, track, work, work_artist) =
            (id(1), id(2), id(3), id(4), id(5), id(6), id(8), id(9));
        let created =
            "strftime('%Y-%m-%dT%H:%M:%SZ', 1600000000 + (i * 7919) % 50000000, 'unixepoch')";
        format!(
            "{rows} INSERT INTO artists (id, name, name_key, created_at) \
                 SELECT {artist}, printf('Artist %06d', (i * 48271) % 1000003), \
                        printf('artist %06d', (i * 48271) % 1000003), {created} FROM k;
             {rows} INSERT INTO albums (id, title, year, created_at) \
                 SELECT {album}, printf('Album %05d', (i * 16807) % 99991), \
                        CASE WHEN i % 17 = 0 THEN NULL ELSE 1950 + (i * 31) % 70 END, {created} FROM k;
             {rows} INSERT INTO album_artists (id, album_id, artist_id, position, created_at) \
                 SELECT {credit}, {album}, {artist}, 0, {created} FROM k;
             {rows} INSERT INTO releases (id, album_id, album_order, created_at) \
                 SELECT {release}, {album}, 0, {created} FROM k;
             {rows} INSERT INTO release_mediums (id, release_id, position, side_count, created_at) \
                 SELECT {medium}, {release}, 1, 1, {created} FROM k;
             {rows} INSERT INTO tracks (id, release_id, title, medium_id, side, track_number, \
                                        duration_ms, created_at) \
                 SELECT {track}, {release}, printf('Track %06d', i), {medium}, 1, 1, 180000, \
                        {created} FROM k;
             {rows} INSERT INTO works (id, title, musicbrainz_work_id, created_at) \
                 SELECT {work}, printf('Work %06d', i), {work}, {created} FROM k;
             {rows} INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) \
                 SELECT {work_artist}, {work}, {artist}, 0, 'musicbrainz', {created} FROM k;
             {rows} INSERT INTO download_queue (position, release_id, created_at, state) \
                 SELECT i, {release}, i, 'queued' FROM k;
             {rows} INSERT INTO playback_context (entry, position, track_id, unshuffled) \
                 SELECT i, i, {track}, i FROM k;
             UPDATE playback_queue SET source_kind = 'library', shuffled = 0, cursor = -1;"
        )
    }

    async fn grow_to(db: &Database, originals: &std::path::Path, from: u64, to: u64) -> Duration {
        let started = Instant::now();
        seed(db, originals, from, to).await;
        started.elapsed()
    }

    async fn measure_albums(db: &Database, n: u64) {
        let mut sorts: Vec<(String, Vec<AlbumSortCriterion>)> =
            vec![("default".into(), Vec::new())];
        for field in [
            AlbumSortField::Title,
            AlbumSortField::Artist,
            AlbumSortField::Year,
            AlbumSortField::DateAdded,
        ] {
            for (field, direction) in both_ways(field) {
                sorts.push((
                    format!("{field:?}_{direction:?}"),
                    vec![AlbumSortCriterion { field, direction }],
                ));
            }
        }
        for group_by_artist in [false, true] {
            let (list, rows): (&str, Rows<_>) = if group_by_artist {
                ("albums_by_artist", |sql, order, seek, limit, offset| {
                    crate::album::album_rows_on(
                        sql,
                        &crate::album::album_browse_order(&[], true).0,
                        true,
                        order,
                        seek,
                        limit,
                        offset,
                    )
                })
            } else {
                ("albums", |sql, order, seek, limit, offset| {
                    crate::album::album_rows_on(
                        sql,
                        &crate::album::album_browse_order(&[], false).0,
                        false,
                        order,
                        seek,
                        limit,
                        offset,
                    )
                })
            };
            for (name, sort) in &sorts {
                let (_, order) = crate::album::album_browse_order(sort, group_by_artist);
                measure(db, list, name, n, order, rows).await;
            }
        }
    }

    async fn measure_artists(db: &Database, n: u64) {
        let mut sorts = vec![("default".to_string(), Vec::new())];
        for field in [ArtistSortField::Name, ArtistSortField::AlbumCount] {
            for (field, direction) in both_ways(field) {
                sorts.push((
                    format!("{field:?}_{direction:?}"),
                    vec![ArtistSortCriterion { field, direction }],
                ));
            }
        }
        for (name, sort) in sorts {
            measure(
                db,
                "artists",
                &name,
                n,
                crate::artist::artist_order(&sort),
                crate::artist::artist_rows_on,
            )
            .await;
        }
    }

    async fn measure_composers(db: &Database, n: u64) {
        let mut sorts = vec![("default".to_string(), Vec::new())];
        for field in [
            ComposerSortField::Name,
            ComposerSortField::WorkCount,
            ComposerSortField::LinkedReleaseCount,
        ] {
            for (field, direction) in both_ways(field) {
                sorts.push((
                    format!("{field:?}_{direction:?}"),
                    vec![ComposerSortCriterion { field, direction }],
                ));
            }
        }
        for (name, sort) in sorts {
            measure(
                db,
                "composers",
                &name,
                n,
                crate::composer::composer_order(&sort),
                crate::composer::composer_rows_on,
            )
            .await;
        }
    }

    async fn measure_storage(db: &Database, n: u64) {
        for (filter, rows) in [
            (
                StorageFilter::All,
                (|sql, order, seek, limit, offset| {
                    crate::storage::storage_rows_on(
                        sql,
                        order,
                        StorageFilter::All,
                        None,
                        seek,
                        limit,
                        offset,
                    )
                }) as Rows<_>,
            ),
            (StorageFilter::Local, |sql, order, seek, limit, offset| {
                crate::storage::storage_rows_on(
                    sql,
                    order,
                    StorageFilter::Local,
                    None,
                    seek,
                    limit,
                    offset,
                )
            }),
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
                    let order =
                        crate::storage::storage_order(&StorageSortCriterion { field, direction });
                    let name = format!("{filter:?}_{field:?}_{direction:?}");
                    measure(db, "storage", &name, n, order, rows).await;
                }
            }
        }
    }

    /// What a search narrowing the Storage Manager list costs: counting what it
    /// finds, which every read of it does first, and then each window, read as
    /// the search's counts decide. `album` is in every title; `album 123` is in
    /// about one title in a thousand.
    async fn measure_storage_search(db: &Database, n: u64) {
        for (name, text) in [("broad", "album"), ("rare", "album 123")] {
            let query = crate::LibrarySearchQuery::parse(text).unwrap();
            let counted = query.clone();
            let timing = median(db, move |sql| {
                let (count, _, _) =
                    crate::storage::storage_totals_on(sql, StorageFilter::All, Some(&counted))?;
                Ok(count as usize)
            })
            .await;
            report("storage_search", name, "count", "all", 0, n, timing);
            let counted = query.clone();
            let (found, search) = db
                .read(move |sql| {
                    let (count, _, search) = crate::storage::storage_totals_on(
                        &sql,
                        StorageFilter::All,
                        Some(&counted),
                    )?;
                    Ok((count, search.unwrap()))
                })
                .await
                .unwrap();
            let search = std::sync::Arc::new(search);
            for (field, direction) in [
                (StorageSortField::AlbumTitle, SortDirection::Ascending),
                (StorageSortField::AlbumTitle, SortDirection::Descending),
                (StorageSortField::DateAdded, SortDirection::Descending),
                (StorageSortField::Storage, SortDirection::Ascending),
            ] {
                let order =
                    crate::storage::storage_order(&StorageSortCriterion { field, direction });
                let search = search.clone();
                measure(
                    db,
                    "storage_search",
                    &format!("{name}_{field:?}_{direction:?}"),
                    found,
                    order,
                    move |sql, order, seek, limit, offset| {
                        crate::storage::storage_rows_on(
                            sql,
                            order,
                            StorageFilter::All,
                            Some(search.as_ref()),
                            seek,
                            limit,
                            offset,
                        )
                    },
                )
                .await;
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "measurement: run in release with --ignored --nocapture"]
    async fn window_cost() {
        let (db, tmp) = bae_db::test_fixtures::empty_db(bae_db::test_fixtures::TEST_SCHEMA).await;
        let mut held = 0;
        for n in SIZES {
            let took = grow_to(&db, &tmp.path().join("originals"), held, n).await;
            eprintln!("WINDOW_COST seeded n={n} in {:.1} s", took.as_secs_f64());
            held = n;
            measure_albums(&db, n).await;
            measure_artists(&db, n).await;
            measure_composers(&db, n).await;
            measure_storage(&db, n).await;
            if n == 100_000 {
                measure_storage_search(&db, n).await;
            }
            measure_downloads(&db, n).await;
            bae_playback::tables::window_cost::measure_playback_queue(&db, n).await;
        }
    }

    /// [`seed_sql`]'s rows `start..end`, then each release's zero to three
    /// files, staged on this device only from originals under `originals`, one
    /// original for each size, which every file of that size names.
    pub(crate) async fn seed(db: &Database, originals: &std::path::Path, start: u64, end: u64) {
        let sql = seed_sql(start, end);
        db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from)).await.unwrap();
        std::fs::create_dir_all(originals).unwrap();
        let mut files = Vec::new();
        for i in start..end {
            let created = chrono::DateTime::from_timestamp(
                1_600_000_000 + i64::try_from((i * 7919) % 50_000_000).unwrap(),
                0,
            )
            .unwrap();
            for j in 0..i % 4 {
                let size = 1000 + (i * 7 + j) % 50_000;
                let original = originals.join(format!("{size}.bin"));
                if !original.exists() {
                    std::fs::File::create_new(&original).unwrap().set_len(size).unwrap();
                }
                files.push((
                    NewReleaseFile {
                        id: format!("{i:08x}-0007-4000-8000-{j:012x}"),
                        release_id: format!("{i:08x}-0003-4000-8000-{i:012x}"),
                        original_filename: format!("{j:02}.flac"),
                        content_type: bae_util::content_type::ContentType::Flac,
                        source_audio: None,
                        created_at: created,
                    },
                    original,
                ));
            }
        }
        db.insert_files_for_test(files).await.unwrap();
    }

    /// What the download queue's windows cost to read.
    async fn measure_downloads(db: &Database, n: u64) {
        measure(
            db,
            "downloads",
            "queued",
            n,
            bae_transfers::tables::queue_order(),
            bae_transfers::tables::queue_window_rows_on::<()>,
        )
        .await;
    }
}
