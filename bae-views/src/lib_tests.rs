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
/// `statement` builds the seek's statement. Only the grid grouped by artist
/// sorts, and only one artist's albums at a time: each artist's as it
/// reaches it (`LAST … TERMS`), or, holding the artist's sort key and id (the
/// first `sorted_within` terms) equal, that artist's alone.
pub(super) async fn assert_seeks_search(
    db: &Store,
    what: &str,
    order: &ListOrder,
    key: OrderKey,
    statement: impl Fn(&Seek) -> String,
    extra: Vec<(&'static str, rusqlite::types::Value)>,
) {
    assert_seeks_search_sorting(db, what, order, key, statement, extra, usize::MAX).await
}

pub(super) async fn assert_seeks_search_sorting(
    db: &Store,
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
    use bae_records::test_fixtures::bulk_library;
    use bae_store::test_fixtures::kept_store::{create_kept, open_kept};
    use bae_store::test_fixtures::window_cost::*;

    use crate::*;
    use std::time::Instant;

    /// Bumped when `seed` writes different rows, so the kept seeds are made
    /// again.
    const SEED_VERSION: u32 = 1;

    /// The folder holding the kept seeds for this schema, this seed and this
    /// coven: each is made once and every run opens a copy of it. A change to
    /// the schema's SQL, to `seed` (`SEED_VERSION`) or to the coven pin
    /// names another folder, and the seeds are made again there.
    fn seed_root() -> std::path::PathBuf {
        use sha2::{Digest, Sha256};
        let mut key = Sha256::new();
        for sql in TEST_SCHEMA.sql() {
            key.update(sql.len().to_le_bytes());
            key.update(sql);
        }
        key.update(SEED_VERSION.to_le_bytes());
        let mut pin: Vec<&str> = include_str!("../../Cargo.lock")
            .lines()
            .filter(|line| line.contains("bae-fm/coven"))
            .collect();
        pin.sort_unstable();
        pin.dedup();
        assert!(!pin.is_empty(), "Cargo.lock names no coven pin");
        for line in pin {
            key.update(line);
        }
        let key: String = key.finalize().iter().take(8).map(|byte| format!("{byte:02x}")).collect();
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/bench-seed").join(key)
    }

    fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                assert!(
                    kind.is_file(),
                    "{} is neither a file nor a folder",
                    entry.path().display()
                );
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    fn kept_clock() -> coven::ClockRef {
        std::sync::Arc::new(coven::FixedClock::new(bae_store::test_fixtures::fixed_now().into()))
    }

    /// The kept seed of `SIZES`' size `n`, a folder holding the closed store's
    /// folder (`app`), its keychain file and its id, made if it is not there:
    /// each size is a copy of the one before, grown to its size, so the
    /// 100,000 seed holds the rows of the 20,000 and 50,000 ones. It appears
    /// whole or not at all.
    async fn kept_seed(n: u64) -> std::path::PathBuf {
        let root = seed_root();
        let mut previous: Option<(u64, std::path::PathBuf)> = None;
        for size in SIZES.into_iter().take_while(|size| *size <= n) {
            let dir = root.join(size.to_string());
            if !dir.exists() {
                let started = Instant::now();
                let building = root.join(format!("{size}.building"));
                if building.exists() {
                    std::fs::remove_dir_all(&building).unwrap();
                }
                std::fs::create_dir_all(&building).unwrap();
                let app = building.join("app");
                let keychain = building.join("keychain");
                let (store, from) = match &previous {
                    Some((from, before)) => {
                        copy_dir(&before.join("app"), &app);
                        let id = read_store_id(before);
                        std::fs::copy(before.join("keychain"), &keychain).unwrap();
                        let store = open_kept(TEST_SCHEMA, &app, &keychain, id, kept_clock()).await;
                        (store, *from)
                    }
                    None => (create_kept(TEST_SCHEMA, &app, &keychain, kept_clock()).await, 0),
                };
                seed(&store, &root.join("originals"), from, size).await;
                store.close().await.unwrap();
                std::fs::write(building.join("store-id"), store.store_id().to_string()).unwrap();
                std::fs::rename(&building, &dir).unwrap();
                eprintln!(
                    "WINDOW_COST seeded n={size} in {:.1} s",
                    started.elapsed().as_secs_f64()
                );
            }
            previous = Some((size, dir));
        }
        previous.expect("SIZES holds n").1
    }

    fn read_store_id(seed: &std::path::Path) -> coven::StoreId {
        let text = std::fs::read_to_string(seed.join("store-id")).unwrap();
        coven::StoreId(uuid::Uuid::parse_str(text.trim()).unwrap())
    }

    /// A copy of the kept seed of size `n`, opened. The `TempDir` owns the
    /// copy, so it must outlive the store.
    async fn seeded_copy(n: u64) -> (Store, tempfile::TempDir) {
        let seed = kept_seed(n).await;
        let tmp = tempfile::TempDir::new().unwrap();
        let app = tmp.path().join("app");
        let keychain = tmp.path().join("keychain");
        copy_dir(&seed.join("app"), &app);
        std::fs::copy(seed.join("keychain"), &keychain).unwrap();
        let store =
            open_kept(TEST_SCHEMA, &app, &keychain, read_store_id(&seed), kept_clock()).await;
        (store, tmp)
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
                    measure(db, "storage", &name, n, order, rows).await;
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
        )
        .await;
    }
}
