//! What crosses the views: an album's credits are one ordered list
//! (`album_artists` from position 0) that every reader of an album's artists
//! reads whole, and the paged lists' windows read from a row beside them,
//! every seek of every sort an index search. The list-window helpers here
//! serve every list's tests.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::list_windows::OrderKey;
use bae_store::list_windows::{Keyed, ListOrder, Seek};
use bae_store::test_fixtures::temp_db;

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
async fn credited_library() -> (Store, tempfile::TempDir) {
    let (db, dir) = temp_db(crate::TEST_SCHEMA).await;
    let compilation_release = "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04";
    let track_credited = "d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e01";
    RecordsFixture::new()
        .artist(ARTIST_A, "Artist A")
        .artist(ARTIST_B, "Artist B")
        .artist(ARTIST_C, "Artist C")
        .artist(ARTIST_VARIOUS, "Various Artists")
        .album(ALBUM_A, "Third", None, &[ARTIST_A])
        .album(ALBUM_AB, "First", None, &[ARTIST_A, ARTIST_B])
        .album(ALBUM_AC, "Second", None, &[ARTIST_A, ARTIST_C])
        .album(ALBUM_VARIOUS, "Compilation", None, &[ARTIST_VARIOUS])
        .compilation(ALBUM_VARIOUS)
        .release(ALBUM_A, "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e01")
        .release(ALBUM_AB, "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e02")
        .release(ALBUM_AC, "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e03")
        .release(ALBUM_VARIOUS, compilation_release)
        .tracks(
            compilation_release,
            &[
                (track_credited, "Track Credited"),
                ("d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e02", "Track Uncredited"),
            ],
        )
        .track_credits(track_credited, &[ARTIST_C])
        .write(&test_records(&db))
        .await;
    (db, dir)
}

#[tokio::test]
async fn an_album_is_found_by_any_of_its_credits() {
    let (db, _dir) = credited_library().await;

    let results = LibraryViews::new(db.clone()).search_rows("Artist B", 10).await.unwrap();

    let albums: Vec<(&str, String)> = results
        .albums
        .iter()
        .map(|album| (album.id.as_str(), album.artist_line.plain_text()))
        .collect();
    assert_eq!(albums, [(ALBUM_AB, "Artist A, Artist B".to_string())]);
}

#[tokio::test]
async fn a_track_search_row_names_its_own_credits_else_its_albums() {
    let (db, _dir) = credited_library().await;

    let results = LibraryViews::new(db.clone()).search_rows("Track", 10).await.unwrap();

    let tracks: Vec<(&str, String)> = results
        .tracks
        .iter()
        .map(|track| (track.title.as_str(), track.artist_line.plain_text()))
        .collect();
    assert_eq!(
        tracks,
        [
            ("Track Credited", "Artist C".to_string()),
            ("Track Uncredited", "Various Artists".to_string()),
        ]
    );
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
    db: &Store,
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
/// `statement` builds the seek's statement. No seek scans a list's table or
/// sorts the list's rows, wholly or in part.
pub(super) async fn assert_seeks_search(
    db: &Store,
    what: &str,
    order: &ListOrder,
    key: OrderKey,
    statement: impl Fn(&Seek) -> String,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) {
    for seek in order.seeks_beside(&key) {
        let steps = plan(db, statement(&seek), seek.clone(), extra.clone()).await;
        assert!(
            !steps.iter().any(|step| {
                LIST_ALIASES.iter().any(|alias| {
                    step.starts_with(&format!("SCAN {alias} ")) || step == &format!("SCAN {alias}")
                })
            }) && !picked_window(&steps)
                .iter()
                .any(|step| step.starts_with("USE TEMP B-TREE FOR") && step.ends_with("ORDER BY")),
            "{what} {seek:?}: {steps:?}"
        );
    }
}

/// The steps of a plan that read the list's order, where the statement picks
/// its window in an inner statement and reads the picked rows' columns
/// outside it: those before the outer statement starts (`SCAN picked`, or the
/// subquery it materializes first). The outer statement orders the picked
/// rows alone, so its sorts, and those of the column subqueries, are not the
/// list's. Every step of a statement that picks nothing.
pub(super) fn picked_window(steps: &[String]) -> &[String] {
    if !steps.iter().any(|step| step == "SCAN picked") {
        return steps;
    }
    let outer = steps
        .iter()
        .position(|step| step == "SCAN picked" || step == "MATERIALIZE sending")
        .expect("a plan scanning the picked rows has an outer statement");
    &steps[..outer]
}

/// A library of 300 albums, each with an artist, a release, files and a
/// work, every release queued to download and every track in the lane.
pub(super) async fn library() -> (Store, tempfile::TempDir) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    window_cost::seed(&db, &tmp.path().join("originals"), 0, 300).await;
    (db, tmp)
}

pub(super) fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [(field, SortDirection::Ascending), (field, SortDirection::Descending)]
}

/// The key of row `at` of `order`, read by `rows`.
pub(super) async fn key_at<R: Send + 'static>(
    db: &Store,
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
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    let (first, second) =
        ("7e7d8df5-8292-4287-80be-7abd24f5a992", "863911c6-a6b6-40a7-8096-b85eb877f7c7");
    let (empty, extra) =
        ("f6506bc5-0b41-44de-862f-1668e72c08c6", "049faa5b-52d9-4109-832b-f6853740c876");
    RecordsFixture::new()
        .artist(first, "Artist Name First")
        .artist(second, "Artist Name Second")
        .album(empty, "Album Title Empty", Some(2026), &[])
        .album(extra, "Album Title Extra", Some(2026), &[first, second])
        .release(empty, "7c3d0881-e6d0-4252-8075-709b2282bcc1")
        .release(extra, "ba00ebe0-da50-428a-8ceb-2389d9a9f232")
        .write(&test_records(&db))
        .await;

    let album_sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let albums =
        crate::LibraryViews::new(db.clone()).get_album_page(&album_sort, 0, 10).await.unwrap();
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].artist_line.plain_text(), "");
    assert_eq!(albums[1].artist_line.plain_text(), "Artist Name First, Artist Name Second");

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
    assert_eq!(rows[0].album.artist_line.plain_text(), "");
    assert_eq!(rows[1].album.artist_line.plain_text(), "Artist Name First, Artist Name Second");
}

/// An artist window starting at a key holds the rows from that key on, the
/// rows reading by offset holds, and each statement a window can run searches
/// the order's index and sorts nothing.
#[tokio::test]
async fn an_artist_window_from_a_key_holds_the_rows_from_it_through_the_index() {
    let (db, _tmp) = library().await;
    let sort = Vec::new();
    let order = crate::artist::artist_order(&sort);
    let views = LibraryViews::new(db.clone());
    let from = views.artist_key_at(&sort, 150).await.unwrap().expect("a row at 150");
    let mut live = views.subscribe_artist_window(
        &sort,
        ListWindow::Forward { from: Some(from.clone()), until: None, limit: 50 },
    );
    let held = live.next().await.expect("the query stays open").result.unwrap();
    let by_offset = views.get_artist_page(&sort, 150, 50).await.unwrap();
    let ids =
        |rows: &[DbArtistSummary]| rows.iter().map(|row| row.artist.id.clone()).collect::<Vec<_>>();
    assert_eq!(held.rows.len(), 50);
    assert_eq!(ids(&held.rows), ids(&by_offset));
    assert_seeks_search(
        &db,
        "artists from a key",
        &order,
        from,
        |seek| crate::listing_window_query(crate::artist::artist_listing_query, &order, seek),
        Vec::new(),
    )
    .await;
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
    use bae_records::test_fixtures::bulk_library;
    use bae_store::test_fixtures::kept_store::KeptSeeds;
    use bae_store::test_fixtures::window_cost::*;

    use crate::*;

    /// Bumped when `seed` writes different rows, so the kept seeds are made
    /// again.
    const SEED_VERSION: u32 = 1;

    /// The kept seeds of `SIZES`: each size is a copy of the one before,
    /// grown to its size, so the 100,000 seed holds the rows of the 20,000
    /// and 50,000 ones.
    const SEEDS: KeptSeeds = KeptSeeds {
        schema: TEST_SCHEMA,
        version: SEED_VERSION,
        sizes: &SIZES,
        seed: |store, originals, start, end| Box::pin(seed(store, originals, start, end)),
    };

    async fn seeded_copy(n: u64) -> (Store, tempfile::TempDir) {
        SEEDS.seeded_copy(n).await
    }

    async fn measure_albums(db: &Store, n: u64) {
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
            let (list, rows, count): (&str, Rows<_>, Count) = if group_by_artist {
                (
                    "albums_by_artist",
                    |sql, order, seek, limit, offset| {
                        crate::album::album_rows_on(sql, true, order, seek, limit, offset)
                    },
                    |sql, order, seek| crate::album::album_seek_count_on(sql, true, order, seek),
                )
            } else {
                (
                    "albums",
                    |sql, order, seek, limit, offset| {
                        crate::album::album_rows_on(sql, false, order, seek, limit, offset)
                    },
                    |sql, order, seek| crate::album::album_seek_count_on(sql, false, order, seek),
                )
            };
            for (name, sort) in &sorts {
                let (_, order) = crate::album::album_browse_order(sort, group_by_artist);
                measure(db, list, name, n, order, rows, count).await;
            }
        }
    }

    async fn measure_artists(db: &Store, n: u64) {
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
                crate::artist::artist_seek_count_on,
            )
            .await;
        }
    }

    async fn measure_composers(db: &Store, n: u64) {
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
                crate::composer::composer_seek_count_on,
            )
            .await;
        }
    }

    async fn measure_storage(db: &Store, n: u64) {
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
                    let count = move |sql: &SqlReadContext<'_>, order: &_, seek: &_| {
                        crate::storage::storage_seek_count_on(sql, filter, None, order, seek)
                    };
                    measure(db, "storage", &name, n, order, rows, count).await;
                }
            }
        }
    }

    /// What a search narrowing the Storage Manager list costs: counting what it
    /// finds, which every read of it does first, and then each window, read as
    /// the search's counts decide. `album` is in every title; `album 123` is in
    /// about one title in a thousand.
    async fn measure_storage_search(db: &Store, n: u64) {
        for (name, text) in [("broad", "album"), ("rare", "album 123")] {
            let pattern = bae_util::search_text::search_pattern(text);
            let counted = pattern.clone();
            let timing = median(db, move |sql| {
                let (count, _) =
                    crate::storage::storage_counts_on(sql, StorageFilter::All, Some(&counted))?;
                Ok(count as usize)
            })
            .await;
            report("storage_search", name, "count", "all", 0, n, timing);
            let counted = pattern.clone();
            let found = db
                .read(move |sql| {
                    Ok(crate::storage::storage_counts_on(&sql, StorageFilter::All, Some(&counted))?
                        .0)
                })
                .await
                .unwrap();
            for (field, direction) in [
                (StorageSortField::AlbumTitle, SortDirection::Ascending),
                (StorageSortField::AlbumTitle, SortDirection::Descending),
                (StorageSortField::DateAdded, SortDirection::Descending),
                (StorageSortField::Storage, SortDirection::Ascending),
            ] {
                let order =
                    crate::storage::storage_order(&StorageSortCriterion { field, direction });
                let window_pattern = pattern.clone();
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
                            Some(window_pattern.as_str()),
                            seek,
                            limit,
                            offset,
                        )
                    },
                    {
                        let pattern = pattern.clone();
                        move |sql: &SqlReadContext<'_>, order: &_, seek: &_| {
                            crate::storage::storage_seek_count_on(
                                sql,
                                StorageFilter::All,
                                Some(pattern.as_str()),
                                order,
                                seek,
                            )
                        }
                    },
                )
                .await;
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "measurement: run in release with --ignored --nocapture"]
    async fn window_cost() {
        for n in SIZES {
            let (db, _copy) = seeded_copy(n).await;
            let db = &db;
            measure_albums(db, n).await;
            measure_artists(db, n).await;
            measure_composers(db, n).await;
            measure_storage(db, n).await;
            if n == 100_000 {
                measure_storage_search(db, n).await;
            }
            measure_downloads(db, n).await;
            bae_playback::tables::window_cost::measure_playback_queue(db, n).await;
        }
    }

    /// Rows `start..end` of every table the lists read: the records'
    /// [`bulk_library`] rows, a download queued for each release and each track
    /// in the context lane, then each release's zero to three files, staged on
    /// this device only from originals under `originals`, one original for
    /// each size, which every file of that size names.
    pub(crate) async fn seed(db: &Store, originals: &std::path::Path, start: u64, end: u64) {
        let records = bae_records::test_fixtures::test_records(db);
        db.write(move |sql| {
            bulk_library::insert_on(&sql, start, end)?;
            bae_transfers::tables::bulk::queue_downloads_on(
                &sql,
                start,
                end,
                &bulk_library::release_id_sql(),
            )?;
            bae_playback::tables::window_cost::fill_context_lane_on(
                &sql,
                start,
                end,
                &bulk_library::track_id_sql(),
            )?;
            Ok::<(), bae_records::IdentityWriteError>(())
        })
        .await
        .unwrap();
        std::fs::create_dir_all(originals).unwrap();
        let mut files = Vec::new();
        for i in start..end {
            let created = bulk_library::created_at(i);
            for j in 0..i % 4 {
                let size = 1000 + (i * 7 + j) % 50_000;
                let original = originals.join(format!("{size}.bin"));
                if !original.exists() {
                    std::fs::File::create_new(&original).unwrap().set_len(size).unwrap();
                }
                files.push((
                    NewReleaseFile {
                        id: bulk_library::file_id(i, j),
                        release_id: bulk_library::release_id(i),
                        original_filename: format!("{j:02}.flac"),
                        content_type: bae_util::content_type::ContentType::Flac,
                        role: bae_records::ReleaseFileRole::Other,
                        source_audio: None,
                        created_at: created,
                    },
                    original,
                ));
            }
        }
        records.insert_files_for_test(files).await.unwrap();
    }

    /// What the download queue's windows cost to read.
    async fn measure_downloads(db: &Store, n: u64) {
        measure(
            db,
            "downloads",
            "queued",
            n,
            bae_transfers::tables::queue_order(),
            bae_transfers::tables::queue_window_rows_on::<bae_transfers::DownloadRequest>,
            bae_transfers::tables::queue_seek_count_on::<bae_transfers::DownloadRequest>,
        )
        .await;
    }
}

/// Coven applies a write's rows parents first, so a row is never there before
/// a row it references and no trigger of the views waits for a parent: none
/// gives an arriving artist the listing row or the album refresh that links
/// to it made earlier, or an arriving track the rewrite of a release whose
/// links came first.
#[tokio::test]
async fn no_trigger_of_the_views_waits_for_a_parent_row() {
    let (db, _dir) = temp_db(crate::TEST_SCHEMA).await;
    let waiting: Vec<String> = db
        .read(|sql| {
            Ok(sql.query(
                "SELECT name FROM sqlite_master WHERE type = 'trigger' \
                 AND name IN ('artist_listing_added', 'album_credit_shown_added', 'track_added') \
                 ORDER BY name",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert!(waiting.is_empty(), "triggers waiting for a parent: {waiting:?}");
}
