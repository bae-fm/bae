//! What one window of each paged list costs to read, at the top, the middle
//! and the end of the list, at 20,000, 50,000 and 100,000 rows: reached by a
//! jump, which reads it by its offset, and by scrolling, which reads it from
//! the row beside it (`list_windows`): the row after the window at the top,
//! and the row before it lower down. Each window is read by the reads its live
//! query runs, its rows decoded as that query decodes them. At 100,000 rows
//! the Storage Manager list is also measured narrowed by a search that finds
//! nearly every release and by one that finds few. A measurement, not
//! a check: run it in release with
//!
//! ```sh
//! scripts/cargo.sh test --release -p bae-core --features test-utils --lib -- \
//!     window_cost --ignored --nocapture --test-threads 1
//! ```

use super::super::*;
use std::time::{Duration, Instant};

/// The sizes the lists are measured at, each reached by adding rows to the
/// library the size before it built.
const SIZES: [u64; 3] = [20_000, 50_000, 100_000];

/// How many rows one window holds: the macOS list's page.
const LIMIT: u64 = 50;

/// How many times each window is read; the median is reported.
const RUNS: usize = 15;

/// A library row's id: a UUID naming its table by `kind` and its row by `i`,
/// as an SQL expression over `i`.
fn id(kind: u32) -> String {
    format!("printf('%08x-{kind:04x}-4000-8000-%012x', i, i)")
}

/// Rows `start..end` of every table the lists read: an artist, an album
/// crediting it with one release, one medium and one track, zero to three
/// files, a work the artist composed, a download queued for the release and
/// the track in the context lane. Titles and names run in an order of their
/// own, unrelated to the ids', and every sort has ties the ids break.
pub(super) fn seed_sql(start: u64, end: u64) -> String {
    let rows = format!(
        "WITH RECURSIVE k(i) AS (SELECT {start} UNION ALL SELECT i + 1 FROM k WHERE i + 1 < {end})"
    );
    let (artist, album, release, credit, medium, track, file, work, work_artist) = (
        id(1),
        id(2),
        id(3),
        id(4),
        id(5),
        id(6),
        "printf('%08x-0007-4000-8000-%012x', i, j)".to_string(),
        id(8),
        id(9),
    );
    let created = "strftime('%Y-%m-%dT%H:%M:%SZ', 1600000000 + (i * 7919) % 50000000, 'unixepoch')";
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
         {rows}, f(j) AS (VALUES (0), (1), (2)) \
             INSERT INTO release_files (id, release_id, original_filename, file_size, \
                                        content_type, created_at) \
             SELECT {file}, {release}, printf('%02d.flac', j), 1000 + (i * 7 + j) % 50000, \
                    'audio/flac', {created} FROM k JOIN f ON f.j < i % 4;
         {rows} INSERT INTO works (id, title, musicbrainz_work_id, created_at) \
             SELECT {work}, printf('Work %06d', i), {work}, {created} FROM k;
         {rows} INSERT INTO work_artists (id, work_id, artist_id, position, source, created_at) \
             SELECT {work_artist}, {work}, {artist}, 0, 'musicbrainz', {created} FROM k;
         {rows} INSERT INTO download_queue (position, release_id, created_at, state) \
             SELECT i, {release}, i, 'queued' FROM k;
         {rows} INSERT INTO playback_context (position, track_id, unshuffled) \
             SELECT i, {track}, i FROM k;
         UPDATE playback_queue SET source = 'library', shuffled = 0, cursor = -1;"
    )
}

async fn grow_to(db: &Database, from: u64, to: u64) -> Duration {
    let sql = seed_sql(from, to);
    let started = Instant::now();
    db.call(move |conn| conn.execute_batch(&sql).map_err(DbError::from))
        .await
        .unwrap();
    started.elapsed()
}

/// The median time `read` takes over [`RUNS`] reads, and how many rows its
/// last read returned.
async fn median<F>(db: &Database, read: F) -> (Duration, usize)
where
    F: Fn(&SqlReadContext<'_>) -> Result<usize, DbError> + Send + Sync + Clone + 'static,
{
    let mut runs = Vec::with_capacity(RUNS);
    let mut rows = 0;
    for _ in 0..RUNS {
        let read = read.clone();
        let started = Instant::now();
        rows = db.read(move |sql| read(&sql)).await.unwrap();
        runs.push(started.elapsed());
    }
    runs.sort();
    (runs[RUNS / 2], rows)
}

/// The window offsets measured in a list of `total` rows: its top, its middle
/// and its last window.
fn offsets(total: u64) -> [(&'static str, u64); 3] {
    [
        ("top", 0),
        ("middle", total / 2),
        ("end", total.saturating_sub(LIMIT)),
    ]
}

fn report(
    list: &str,
    order: &str,
    reach: &str,
    at: &str,
    offset: u64,
    n: u64,
    (time, rows): (Duration, usize),
) {
    eprintln!(
        "WINDOW_COST list={list} order={order} reach={reach} n={n} at={at} offset={offset} \
         rows={rows} median_ms={:.3}",
        time.as_secs_f64() * 1000.0
    );
}

/// Reads a list's rows under a seek, at most a limit from an offset, each
/// with its key: what a list's live query reads its windows with.
type Rows<R> =
    fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>;

/// Read one window at each measured offset of a list of `n` rows in `order`,
/// once by a jump and once from the row beside it.
async fn measure<R: Send + 'static>(
    db: &Database,
    list: &str,
    name: &str,
    n: u64,
    order: ListOrder,
    rows: impl Fn(&SqlReadContext<'_>, &ListOrder, &Seek, u64, u64) -> Result<Vec<Keyed<R>>, DbError>
        + Clone
        + Send
        + Sync
        + 'static,
) {
    let order = std::sync::Arc::new(order);
    for (at, offset) in offsets(n) {
        let window =
            crate::library::LibraryPageWindows::from([crate::library::LibraryPageWindow {
                offset,
                limit: LIMIT,
            }]);
        // The row beside the window: after it at the top, before it below.
        let beside = if offset == 0 {
            offset + LIMIT
        } else {
            offset - 1
        };
        let key = {
            let (order, rows) = (order.clone(), rows.clone());
            db.read(move |sql| {
                Ok(rows(&sql, &order, &order.everything(), 1, beside)?
                    .pop()
                    .expect("the row beside a window is in the list")
                    .1)
            })
            .await
            .unwrap()
        };
        for (reach, known) in [
            ("jump", super::super::list_windows::Places::new()),
            (
                "scroll",
                super::super::list_windows::Places::from([(beside, key.clone())]),
            ),
        ] {
            let (order, window, rows) = (order.clone(), window.clone(), rows.clone());
            let timing = median(db, move |sql| {
                let read = read_windows(&order, &window, known.clone(), |seek, limit, at| {
                    rows(sql, &order, seek, limit, at)
                })?;
                Ok(read.windows[0].1.len())
            })
            .await;
            report(list, name, reach, at, offset, n, timing);
        }
    }
}

fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [
        (field, SortDirection::Ascending),
        (field, SortDirection::Descending),
    ]
}

async fn measure_albums(db: &Database, n: u64) {
    let mut sorts: Vec<(String, Vec<AlbumSortCriterion>)> = vec![("default".into(), Vec::new())];
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
                super::super::album::album_rows_on(
                    sql,
                    &query::album_browse_order(&[], true).0,
                    order,
                    seek,
                    limit,
                    offset,
                )
            })
        } else {
            ("albums", |sql, order, seek, limit, offset| {
                super::super::album::album_rows_on(
                    sql,
                    &query::album_browse_order(&[], false).0,
                    order,
                    seek,
                    limit,
                    offset,
                )
            })
        };
        for (name, sort) in &sorts {
            let (_, order) = query::album_browse_order(sort, group_by_artist);
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
            artist_order(&sort),
            super::super::artist::artist_rows_on,
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
            composer_order(&sort),
            super::super::artist::composer_rows_on,
        )
        .await;
    }
}

async fn measure_storage(db: &Database, n: u64) {
    for (filter, rows) in [
        (
            StorageFilter::All,
            (|sql, order, seek, limit, offset| {
                storage_rows_on(sql, order, StorageFilter::All, None, seek, limit, offset)
            }) as Rows<_>,
        ),
        (StorageFilter::Local, |sql, order, seek, limit, offset| {
            storage_rows_on(sql, order, StorageFilter::Local, None, seek, limit, offset)
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
                let order = query::storage_order(&StorageSortCriterion { field, direction });
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
        let query = crate::library::LibrarySearchQuery::parse(text).unwrap();
        let counted = query.clone();
        let timing = median(db, move |sql| {
            let (count, _, _) = storage_totals_on(sql, StorageFilter::All, Some(&counted))?;
            Ok(count as usize)
        })
        .await;
        report("storage_search", name, "count", "all", 0, n, timing);
        let counted = query.clone();
        let (found, search) = db
            .read(move |sql| {
                let (count, _, search) =
                    storage_totals_on(&sql, StorageFilter::All, Some(&counted))?;
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
            let order = query::storage_order(&StorageSortCriterion { field, direction });
            let search = search.clone();
            measure(
                db,
                "storage_search",
                &format!("{name}_{field:?}_{direction:?}"),
                found,
                order,
                move |sql, order, seek, limit, offset| {
                    storage_rows_on(
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

async fn measure_queues(db: &Database, n: u64) {
    measure(
        db,
        "downloads",
        "queued",
        n,
        super::super::release_queues::queue_order(),
        super::super::release_queues::queue_window_rows_on::<()>,
    )
    .await;
    // The context lane's upcoming windows are read by offset alone: the
    // offset steps over the lane's positions before any track is read.
    let lane = format!(
        "SELECT e.entry, {columns} \
         FROM (SELECT 'c' || track_id AS entry, track_id, position FROM playback_context \
               WHERE position > -1 ORDER BY position LIMIT ?1 OFFSET ?2) e \
         JOIN tracks t ON t.id = e.track_id \
         {joins} \
         ORDER BY e.position",
        columns = super::super::playback::track_display_columns(),
        joins = super::super::playback::TRACK_DISPLAY_JOINS,
    );
    for (at, offset) in offsets(n) {
        let lane = lane.clone();
        let timing = median(db, move |sql| {
            Ok(sql
                .query(&lane, params![LIMIT as i64, offset as i64], |_| Ok(()))?
                .len())
        })
        .await;
        report("queue_upcoming", "lane", "jump", at, offset, n, timing);
    }
}

/// Scan `start..end` more folders into the import list, each its own album
/// folder under one of 100 artist folders, in a watched folder of their own.
pub(super) async fn grow_import_list(
    db: &Database,
    tmp: &std::path::Path,
    start: u64,
    end: u64,
) -> Duration {
    let root = tmp.join(format!("watched-{start}"));
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(&root)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let started = Instant::now();
    db.add_watched_import_folder(&root).await.unwrap();
    let generation = db
        .begin_folder_scan(&root, crate::import::VolumeKind::Local)
        .await
        .unwrap();
    for i in start..end {
        let name = format!(
            "Artist {:03}/Album {:06}",
            (i * 7) % 100,
            (i * 16807) % 999_983
        );
        let candidate = super::candidate(&root, &name);
        db.save_folder_scan_item(
            &root,
            generation,
            &crate::import::folder_scanner::ScanItem::Valid(candidate),
        )
        .await
        .unwrap()
        .unwrap();
    }
    db.finish_folder_scan(&root, generation, None)
        .await
        .unwrap()
        .unwrap();
    started.elapsed()
}

/// One read of the import list's Found tab with one window, at each measured
/// offset, in each order, through the list's live query: the counts, the
/// window's items and their rows. A jump moves the window between the offset
/// and one far from it, so no read knows a row beside its window; a scroll
/// moves it between the offset and the window after it.
async fn measure_import_list(db: &Database, n: u64) {
    use crate::import::{ImportListOrder, ImportListRequest, ImportListView, PendingFilter};
    for order in [ImportListOrder::PathAscending, ImportListOrder::NewestFirst] {
        let list = db
            .open_import_list(ImportListView {
                tab: crate::import::TriageTab::Pending,
                filter_text: String::new(),
                pending_filter: PendingFilter::All,
                order,
            })
            .await
            .unwrap();
        let request = |offset| ImportListRequest {
            list,
            windows: crate::library::LibraryPageWindows::from([
                crate::library::LibraryPageWindow {
                    offset,
                    limit: LIMIT,
                },
            ]),
        };
        for (at, offset) in offsets(n) {
            let away = if offset >= n / 2 {
                offset - n / 4
            } else {
                offset + n / 4
            };
            let next = if offset + 2 * LIMIT <= n {
                offset + LIMIT
            } else {
                offset - LIMIT
            };
            for (reach, other) in [("jump", away), ("scroll", next)] {
                let mut query = db.subscribe_import_list(request(other));
                query.next().await.unwrap().result.unwrap();
                let requests = query.requests();
                let mut runs = Vec::with_capacity(RUNS);
                let mut rows = 0;
                for run in 0..2 * RUNS {
                    let target = if run % 2 == 0 { offset } else { other };
                    let started = Instant::now();
                    requests.set(request(target)).unwrap();
                    let read = query.next().await.unwrap().result.unwrap();
                    if target == offset {
                        runs.push(started.elapsed());
                        rows = read.windows[0].items.len();
                    }
                }
                runs.sort();
                report(
                    "import_list",
                    &format!("{order:?}"),
                    reach,
                    at,
                    offset,
                    n,
                    (runs[RUNS / 2], rows),
                );
            }
        }
        db.close_import_list(list).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "measurement: run in release with --ignored --nocapture"]
async fn window_cost() {
    let (db, tmp) = super::empty_db().await;
    let mut held = 0;
    for n in SIZES {
        let took = grow_to(&db, held, n).await;
        eprintln!("WINDOW_COST seeded n={n} in {:.1} s", took.as_secs_f64());
        let took = grow_import_list(&db, tmp.path(), held, n).await;
        eprintln!("WINDOW_COST scanned n={n} in {:.1} s", took.as_secs_f64());
        held = n;
        measure_albums(&db, n).await;
        measure_artists(&db, n).await;
        measure_composers(&db, n).await;
        measure_storage(&db, n).await;
        if n == 100_000 {
            measure_storage_search(&db, n).await;
        }
        measure_queues(&db, n).await;
        measure_import_list(&db, n).await;
    }
}
