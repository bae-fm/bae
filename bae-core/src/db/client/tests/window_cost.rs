//! What one window of each paged list costs to read, at the top, the middle
//! and the end of the list, at 20,000, 50,000 and 100,000 rows. Each window is
//! read with the statement its live query runs, its rows decoded as that query
//! decodes them. A measurement, not a check: run it in release with
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
fn seed_sql(start: u64, end: u64) -> String {
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

fn report(list: &str, order: &str, at: &str, offset: u64, n: u64, (time, rows): (Duration, usize)) {
    eprintln!(
        "WINDOW_COST list={list} order={order} n={n} at={at} offset={offset} rows={rows} \
         median_ms={:.3}",
        time.as_secs_f64() * 1000.0
    );
}

/// Read one window with `read`, given its offset, at each measured offset.
async fn measure<F>(db: &Database, list: &str, order: &str, n: u64, read: F)
where
    F: Fn(&SqlReadContext<'_>, u64) -> Result<usize, DbError> + Send + Sync + Clone + 'static,
{
    for (at, offset) in offsets(n) {
        let read = read.clone();
        let timing = median(db, move |sql| read(sql, offset)).await;
        report(list, order, at, offset, n, timing);
    }
}

/// [`measure`] for a statement ending `LIMIT ? OFFSET ?`, each row decoded
/// with `decode`.
async fn measure_statement(
    db: &Database,
    list: &str,
    order: &str,
    n: u64,
    statement: String,
    decode: fn(&Row<'_>) -> rusqlite::Result<()>,
) {
    measure(db, list, order, n, move |sql, offset| {
        Ok(sql
            .query(&statement, params![LIMIT as i64, offset as i64], decode)?
            .len())
    })
    .await;
}

fn album_row(row: &Row<'_>) -> rusqlite::Result<()> {
    query::AlbumSummaryRow::read(row).map(|_| ())
}

fn artist_row(row: &Row<'_>) -> rusqlite::Result<()> {
    row_to_artist_summary(row).map(|_| ())
}

fn composer_row(row: &Row<'_>) -> rusqlite::Result<()> {
    row_to_composer_summary(row).map(|_| ())
}

fn ignore_row(_: &Row<'_>) -> rusqlite::Result<()> {
    Ok(())
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
        let list = if group_by_artist {
            "albums_by_artist"
        } else {
            "albums"
        };
        for (name, sort) in &sorts {
            let (relation, order_by) = query::album_browse_order(sort, group_by_artist);
            let statement = format!(
                "{} {relation} ORDER BY {order_by} LIMIT ? OFFSET ?",
                query::album_summary_select()
            );
            measure_statement(db, list, name, n, statement, album_row).await;
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
        let tail = format!("ORDER BY {} LIMIT ? OFFSET ?", artist_order_by(&sort));
        let statement = artist_listing_query(None, Some(&tail));
        measure_statement(db, "artists", &name, n, statement, artist_row).await;
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
        let tail = format!("ORDER BY {} LIMIT ? OFFSET ?", composer_order_by(&sort));
        let statement = composer_listing_query(None, Some(&tail));
        measure_statement(db, "composers", &name, n, statement, composer_row).await;
    }
}

async fn measure_storage(db: &Database, n: u64) {
    for filter in [StorageFilter::All, StorageFilter::Local] {
        for field in [
            StorageSortField::AlbumTitle,
            StorageSortField::ArtistNames,
            StorageSortField::Media,
            StorageSortField::FileCount,
            StorageSortField::TotalSize,
            StorageSortField::Storage,
        ] {
            for (field, direction) in both_ways(field) {
                let query =
                    query::storage_page_query(&StorageSortCriterion { field, direction }, filter);
                let order = format!("{filter:?}_{field:?}_{direction:?}");
                measure(db, "storage", &order, n, move |sql, offset| {
                    Ok(super::super::release_projection::storage_page_on(
                        sql, &query, offset, LIMIT,
                    )?
                    .len())
                })
                .await;
            }
        }
    }
}

async fn measure_queues(db: &Database, n: u64) {
    measure_statement(
        db,
        "downloads",
        "queued",
        n,
        super::super::release_queues::queue_rows_sql::<()>(
            "ORDER BY q.position LIMIT ?1 OFFSET ?2",
        ),
        ignore_row,
    )
    .await;
    measure_statement(
        db,
        "queue_upcoming",
        "lane",
        n,
        format!(
            "SELECT e.entry, {columns} \
             FROM (SELECT 'c' || track_id AS entry, track_id, position FROM playback_context \
                   WHERE position > -1 ORDER BY position LIMIT ?1 OFFSET ?2) e \
             JOIN tracks t ON t.id = e.track_id \
             {joins} \
             ORDER BY e.position",
            columns = super::super::playback::track_display_columns(),
            joins = super::super::playback::TRACK_DISPLAY_JOINS,
        ),
        ignore_row,
    )
    .await;
}

/// Scan `start..end` more folders into the import list, each its own album
/// folder under one of 100 artist folders, in a watched folder of their own.
async fn grow_import_list(db: &Database, tmp: &std::path::Path, start: u64, end: u64) -> Duration {
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
/// offset, in each order: the counts, the window's items and their rows.
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
        for (at, offset) in offsets(n) {
            let mut runs = Vec::with_capacity(RUNS);
            let mut rows = 0;
            for _ in 0..RUNS {
                let request = ImportListRequest {
                    list,
                    windows: crate::library::LibraryPageWindows::from([
                        crate::library::LibraryPageWindow {
                            offset,
                            limit: LIMIT,
                        },
                    ]),
                };
                let started = Instant::now();
                let read = db.load_import_list(request).await.unwrap();
                runs.push(started.elapsed());
                rows = read.windows[0].items.len();
            }
            runs.sort();
            report(
                "import_list",
                &format!("{order:?}"),
                at,
                offset,
                n,
                (runs[RUNS / 2], rows),
            );
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
        measure_queues(&db, n).await;
        measure_import_list(&db, n).await;
    }
}
