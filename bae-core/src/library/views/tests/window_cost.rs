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

use crate::db::client::tests::window_cost::*;

use crate::library::views::*;
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
                crate::library::views::album::album_rows_on(
                    sql,
                    &crate::library::views::album::album_browse_order(&[], true).0,
                    true,
                    order,
                    seek,
                    limit,
                    offset,
                )
            })
        } else {
            ("albums", |sql, order, seek, limit, offset| {
                crate::library::views::album::album_rows_on(
                    sql,
                    &crate::library::views::album::album_browse_order(&[], false).0,
                    false,
                    order,
                    seek,
                    limit,
                    offset,
                )
            })
        };
        for (name, sort) in &sorts {
            let (_, order) =
                crate::library::views::album::album_browse_order(sort, group_by_artist);
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
            crate::library::views::artist::artist_order(&sort),
            crate::library::views::artist::artist_rows_on,
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
            crate::library::views::composer::composer_order(&sort),
            crate::library::views::composer::composer_rows_on,
        )
        .await;
    }
}

async fn measure_storage(db: &Database, n: u64) {
    for (filter, rows) in [
        (
            StorageFilter::All,
            (|sql, order, seek, limit, offset| {
                crate::library::views::storage::storage_rows_on(
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
            crate::library::views::storage::storage_rows_on(
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
                let order = crate::library::views::storage::storage_order(&StorageSortCriterion {
                    field,
                    direction,
                });
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
            let (count, _, _) = crate::library::views::storage::storage_totals_on(
                sql,
                StorageFilter::All,
                Some(&counted),
            )?;
            Ok(count as usize)
        })
        .await;
        report("storage_search", name, "count", "all", 0, n, timing);
        let counted = query.clone();
        let (found, search) = db
            .read(move |sql| {
                let (count, _, search) = crate::library::views::storage::storage_totals_on(
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
            let order = crate::library::views::storage::storage_order(&StorageSortCriterion {
                field,
                direction,
            });
            let search = search.clone();
            measure(
                db,
                "storage_search",
                &format!("{name}_{field:?}_{direction:?}"),
                found,
                order,
                move |sql, order, seek, limit, offset| {
                    crate::library::views::storage::storage_rows_on(
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
    let (db, tmp) = crate::db::client::tests::empty_db().await;
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
        crate::playback::tables::tests::window_cost::measure_playback_queue(&db, n).await;
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
        crate::library::queued_releases::tables::queue_order(),
        crate::library::queued_releases::tables::queue_window_rows_on::<()>,
    )
    .await;
}
