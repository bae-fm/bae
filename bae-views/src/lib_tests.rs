//! What crosses the views: an album's credits are one ordered list
//! (`album_artists` from position 0) that every reader of an album's artists
//! reads whole, and the paged lists' windows read from a row beside them. The
//! list-window helpers here serve every list's tests.

use crate::*;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::test_records;
use bae_store::test_fixtures::query_plan::{PlanOf, PlanStep};
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
async fn credited_library() -> (Store, bae_util::test_files::TestFolder) {
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

/// Assert that each of a list's `windows`, a statement with the index leading
/// with its first term and that term's column, seeks to its key through that
/// index, and sorts at most the rows that tie on that term, which lets SQLite
/// stop at the limit.
pub(super) async fn assert_windows_seek_their_first_term(windows: &[(PlanOf, &str, &str)]) {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(super::TEST_SCHEMA).await;
    for &(plan_of, index, column) in windows {
        let steps = db.read(move |sql| Ok(plan_of(&sql)?)).await.unwrap();
        let seek = format!("SEARCH listing USING INDEX {index} ({column}>?)");
        assert!(steps.iter().any(|step| step.detail == seek), "{index}: {steps:?}");
        assert_sorts_only_ties(index, &steps);
    }
}

/// Assert that each of a list's `tops`, a statement that reads from the
/// order's first row with the index leading with its first term, reads that
/// index in order and sorts at most the rows that tie on the first term,
/// which lets SQLite stop at the limit.
pub(super) async fn assert_tops_read_their_first_term(tops: &[(PlanOf, &str)]) {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(super::TEST_SCHEMA).await;
    for &(plan_of, index) in tops {
        let steps = db.read(move |sql| Ok(plan_of(&sql)?)).await.unwrap();
        let scan = format!("SCAN listing USING INDEX {index}");
        assert!(steps.iter().any(|step| step.detail == scan), "{index}: {steps:?}");
        assert_sorts_only_ties(index, &steps);
    }
}

/// Assert that the statement's own steps sort at most the rows that tie on
/// the first term of the order `index` leads with.
fn assert_sorts_only_ties(index: &str, steps: &[PlanStep]) {
    assert!(
        steps.iter().filter(|step| step.own).all(|step| !step.detail.contains("TEMP B-TREE")
            || step.detail.contains("TERMS OF ORDER BY")
            || step.detail.contains("RIGHT PART OF ORDER BY")),
        "{index} sorts the whole list: {steps:?}"
    );
}

/// Both ways of each of `fields`, each alone and then before the next field:
/// the sorts a list's window from the top is checked under.
pub(super) fn sorts_of<F: Copy>(fields: &[F]) -> Vec<Vec<(F, SortDirection)>> {
    let mut sorts = Vec::new();
    for (place, &field) in fields.iter().enumerate() {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            sorts.push(vec![(field, direction)]);
            let next = fields[(place + 1) % fields.len()];
            sorts.push(vec![(field, direction), (next, SortDirection::Descending)]);
        }
    }
    sorts
}

/// Asserts that a list's window of 20 rows from the top, read by `window`,
/// holds rows, starting with the row at position 0, whose key `key_at`
/// reads, and holds the rows and keys of the window from that key.
pub(super) fn assert_top_is_first_rows<R>(
    what: &str,
    key_at: impl Fn() -> Result<Option<OrderKey>, DbError>,
    window: impl Fn(&ListWindow) -> Result<WindowRead<R>, DbError>,
) -> Result<(), DbError> {
    let keys =
        |read: &WindowRead<R>| read.rows.iter().map(|(_, key)| key.clone()).collect::<Vec<_>>();
    let limit = 20;
    let first = key_at()?.unwrap_or_else(|| panic!("{what}: the list has rows"));
    let top = window(&ListWindow::top(limit))?;
    let from_first = window(&ListWindow { from: Some(first), limit })?;
    assert!(!top.rows.is_empty(), "{what}: the window from the top holds no rows");
    assert_eq!(keys(&top), keys(&from_first), "{what}");
    Ok(())
}

/// Whether `step` sorts rows for an ORDER BY.
fn sorts_for_order_by(step: &PlanStep) -> bool {
    step.detail.starts_with("USE TEMP B-TREE FOR") && step.detail.ends_with("ORDER BY")
}

/// Whether `steps` sort for an ORDER BY at any level of the statement,
/// subqueries included.
pub(super) fn sorts_at_any_level(steps: &[PlanStep]) -> bool {
    steps.iter().any(sorts_for_order_by)
}

/// A library of 300 albums, each with an artist, a release, files and a
/// work, every release queued to download and every track in the lane.
pub(super) async fn library() -> (Store, bae_util::test_files::TestFolder) {
    let (db, tmp) = temp_db(crate::TEST_SCHEMA).await;
    window_cost::seed(&db, &tmp.path().join("originals"), 0, 300).await;
    (db, tmp)
}

/// Where core places the album `album_id`'s card under `sort`, as the
/// grid's live read of it first answers.
pub(super) async fn place_of(
    views: &LibraryViews,
    sort: &[AlbumSortCriterion],
    album_id: &str,
    group_by_artist: bool,
    section: Option<&str>,
) -> Option<crate::album::AlbumPlace> {
    views
        .subscribe_album_place(sort, album_id, group_by_artist, section)
        .next_inline()
        .await
        .expect("the place's read stays open")
        .unwrap()
}

pub(super) fn both_ways<F: Copy>(field: F) -> [(F, SortDirection); 2] {
    [(field, SortDirection::Ascending), (field, SortDirection::Descending)]
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

/// Under stacked sorts, with ties on every criterion, the artist list's page,
/// window from a key, position and jump all read one order: the criteria in
/// turn, then name, then id.
#[tokio::test]
async fn a_stacked_artist_sort_pages_windows_counts_and_jumps_in_one_order() {
    use ArtistSortField::{AlbumCount, Name};
    use SortDirection::{Ascending, Descending};

    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    // By name, then id, the artists run second, fourth, first, third; the
    // second has one album and each other one two.
    let [first, second, third, fourth] = [ARTIST_A, ARTIST_B, ARTIST_C, ARTIST_VARIOUS];
    RecordsFixture::new()
        .artist(first, "Artist Name B")
        .artist(second, "Artist Name A")
        .artist(third, "Artist Name C")
        .artist(fourth, "Artist Name A")
        .album(ALBUM_A, "Album Title A", None, &[first, second, third, fourth])
        .album(ALBUM_AB, "Album Title B", None, &[first, third, fourth])
        .release(ALBUM_A, "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e01")
        .release(ALBUM_AB, "9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e02")
        .write(&test_records(&db))
        .await;
    let views = LibraryViews::new(db.clone());
    let by = |field, direction| ArtistSortCriterion { field, direction };

    for (sort, expected) in [
        (vec![], [second, fourth, first, third]),
        (vec![by(Name, Descending)], [third, first, second, fourth]),
        (vec![by(AlbumCount, Ascending)], [second, fourth, first, third]),
        (vec![by(AlbumCount, Descending)], [fourth, first, third, second]),
        (vec![by(AlbumCount, Descending), by(Name, Descending)], [third, first, fourth, second]),
        (vec![by(AlbumCount, Ascending), by(Name, Descending)], [second, third, first, fourth]),
        (vec![by(Name, Descending), by(AlbumCount, Descending)], [third, first, fourth, second]),
        (vec![by(Name, Ascending), by(AlbumCount, Ascending)], [second, fourth, first, third]),
    ] {
        let ids = |rows: Vec<DbArtistSummary>| {
            rows.into_iter().map(|row| row.artist.id).collect::<Vec<_>>()
        };
        for (position, _) in expected.iter().enumerate() {
            let key = views.artist_key_at(&sort, position as u64).await.unwrap().expect("a key");
            let window = ListWindow { from: Some(key.clone()), limit: 10 };
            let mut live = views.subscribe_artist_window(&sort, window);
            let held = live.next_inline().await.expect("the query stays open").result.unwrap();
            let held = held.rows.into_iter().map(|(row, _key)| row).collect();
            assert_eq!(ids(held), expected[position..], "{sort:?} from {position}");
            let before = views
                .subscribe_artist_position(&sort, key)
                .next_inline()
                .await
                .expect("the query stays open")
                .result
                .unwrap();
            assert_eq!(before, position as u64, "{sort:?}");
        }
        assert_eq!(
            views.artist_key_at(&sort, 10).await.unwrap(),
            views.artist_key_at(&sort, 3).await.unwrap(),
            "{sort:?}: a jump past the end finds the last row"
        );
    }
}

/// Three stacked composer criteria read a window, from the top and from a key,
/// that holds the rows a page reads at those places, and the position of the
/// key is the place.
#[tokio::test]
async fn a_composer_window_under_three_criteria_holds_the_rows_at_its_places() {
    let (db, _tmp) = library().await;
    let views = LibraryViews::new(db.clone());
    let sort = [
        (ComposerSortField::WorkCount, SortDirection::Descending),
        (ComposerSortField::LinkedReleaseCount, SortDirection::Ascending),
        (ComposerSortField::Name, SortDirection::Descending),
    ]
    .map(|(field, direction)| ComposerSortCriterion { field, direction });
    let ids = |rows: &[DbComposerSummary]| {
        rows.iter().map(|row| row.artist.id.clone()).collect::<Vec<_>>()
    };
    for position in [0, 150] {
        let from = match position {
            0 => None,
            at => views.composer_key_at(&sort, at).await.unwrap(),
        };
        let mut live =
            views.subscribe_composer_window(&sort, ListWindow { from: from.clone(), limit: 50 });
        let held = live.next_inline().await.expect("the query stays open").result.unwrap();
        let held: Vec<DbComposerSummary> = held.rows.into_iter().map(|(row, _key)| row).collect();
        assert_eq!(held.len(), 50);
        assert_eq!(ids(&held), ids(&views.get_composer_page(&sort, position, 50).await.unwrap()));
        let Some(key) = from else { continue };
        let before = views
            .subscribe_composer_position(&sort, key)
            .next_inline()
            .await
            .expect("the query stays open")
            .result
            .unwrap();
        assert_eq!(before, position);
    }
}

/// What one window of each paged list costs to read, at the top, the middle
/// and the end of the list, at 20,000, 50,000 and 100,000 rows: reached by a
/// jump, which reads it by its offset, and by scrolling, which reads it from
/// the row beside it: the row after the window at the top,
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

    async fn seeded_copy(n: u64) -> (Store, bae_util::test_files::TestFolder) {
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
        // Every field stacked behind one that many albums tie on.
        sorts.push((
            "Year_stacked".into(),
            [
                (AlbumSortField::Year, SortDirection::Ascending),
                (AlbumSortField::Artist, SortDirection::Descending),
                (AlbumSortField::Title, SortDirection::Ascending),
                (AlbumSortField::DateAdded, SortDirection::Descending),
            ]
            .map(|(field, direction)| AlbumSortCriterion { field, direction })
            .into(),
        ));
        for (group_by_artist, list) in [(false, "albums"), (true, "albums_by_artist")] {
            for (name, sort) in &sorts {
                let order = crate::album::AlbumOrder::new(sort, group_by_artist);
                measure_reads(
                    db,
                    list,
                    name,
                    n,
                    move |sql, position| crate::album::album_key_at_on(sql, &order, position),
                    move |sql, window| {
                        Ok(sql.reads(&crate::album::album_window_read(&order, window)?)?.len())
                    },
                    move |sql, key| crate::album::albums_before_on(sql, &order, key),
                )
                .await;
            }
        }
    }

    /// Every sort of the artist list by one criterion, and one stacking both.
    fn artist_sorts() -> Vec<(String, Vec<ArtistSortCriterion>)> {
        let mut sorts = vec![("default".to_string(), Vec::new())];
        for field in [ArtistSortField::Name, ArtistSortField::AlbumCount] {
            for (field, direction) in both_ways(field) {
                sorts.push((
                    format!("{field:?}_{direction:?}"),
                    vec![ArtistSortCriterion { field, direction }],
                ));
            }
        }
        let stacked = [ArtistSortField::AlbumCount, ArtistSortField::Name]
            .map(|field| ArtistSortCriterion { field, direction: SortDirection::Descending });
        sorts.push(("stacked".to_string(), stacked.to_vec()));
        sorts
    }

    /// Every sort of the composer list by one criterion, and one stacking all
    /// three.
    fn composer_sorts() -> Vec<(String, Vec<ComposerSortCriterion>)> {
        let fields = [
            ComposerSortField::WorkCount,
            ComposerSortField::LinkedReleaseCount,
            ComposerSortField::Name,
        ];
        let mut sorts = vec![("default".to_string(), Vec::new())];
        for field in fields {
            for (field, direction) in both_ways(field) {
                sorts.push((
                    format!("{field:?}_{direction:?}"),
                    vec![ComposerSortCriterion { field, direction }],
                ));
            }
        }
        let stacked = fields
            .map(|field| ComposerSortCriterion { field, direction: SortDirection::Descending });
        sorts.push(("stacked".to_string(), stacked.to_vec()));
        sorts
    }

    async fn measure_artists(db: &Store, n: u64) {
        for (name, sort) in artist_sorts() {
            let sort = crate::artist::artist_sort(&sort);
            measure_reads(
                db,
                "artists",
                &name,
                n,
                move |sql, position| crate::artist::artist_key_at_on(sql, &sort, position),
                move |sql, window| {
                    Ok(crate::artist::artist_window_on(sql, &sort, window)?.rows.len())
                },
                move |sql, key| crate::artist::artists_before_on(sql, &sort, key),
            )
            .await;
        }
    }

    async fn measure_composers(db: &Store, n: u64) {
        for (name, sort) in composer_sorts() {
            let sort = crate::composer::composer_sort(&sort);
            measure_reads(
                db,
                "composers",
                &name,
                n,
                move |sql, position| crate::composer::composer_key_at_on(sql, &sort, position),
                move |sql, window| {
                    Ok(crate::composer::composer_window_on(sql, &sort, window)?.rows.len())
                },
                move |sql, key| crate::composer::composers_before_on(sql, &sort, key),
            )
            .await;
        }
    }

    /// Time the reads of the Storage Manager list under `sort`, `filter` and
    /// `search`, which holds `n` releases.
    async fn measure_storage_list(
        db: &Store,
        list: &str,
        name: &str,
        n: u64,
        sort: &StorageSortCriterion,
        filter: StorageFilter,
        search: &str,
    ) {
        let order = crate::storage::StorageList::new(sort, filter, search);
        let (jumped, windowed, counted) = (order.clone(), order.clone(), order);
        measure_reads(
            db,
            list,
            name,
            n,
            move |sql, position| crate::storage::storage_key_at_on(sql, &jumped, position),
            move |sql, window| {
                Ok(crate::storage::storage_window_on(sql, &windowed, window)?.rows.len())
            },
            move |sql, key| crate::storage::storage_rows_before_on(sql, &counted, key),
        )
        .await;
    }

    async fn measure_storage(db: &Store, n: u64) {
        for filter in [StorageFilter::All, StorageFilter::Local] {
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
                    let sort = StorageSortCriterion { field, direction };
                    let name = format!("{filter:?}_{field:?}_{direction:?}");
                    measure_storage_list(db, "storage", &name, n, &sort, filter, "").await;
                }
            }
        }
    }

    /// What a search narrowing the Storage Manager list costs: counting what it
    /// finds, and each read of the narrowed list. `album` is in every title;
    /// `album 123` is in about one title in a thousand.
    async fn measure_storage_search(db: &Store, n: u64) {
        for (name, text) in [("broad", "album"), ("rare", "album 123")] {
            let scope = crate::storage::StorageScope::new(StorageFilter::All, text);
            let counted = scope.clone();
            let timing = median(db, move |sql| {
                let (count, _) = crate::storage::storage_counts_on(sql, &counted)?;
                Ok(count as usize)
            })
            .await;
            report("storage_search", name, "count", "all", 0, n, timing);
            let found = db
                .read(move |sql| Ok(crate::storage::storage_counts_on(&sql, &scope)?.0))
                .await
                .unwrap();
            for (field, direction) in [
                (StorageSortField::AlbumTitle, SortDirection::Ascending),
                (StorageSortField::AlbumTitle, SortDirection::Descending),
                (StorageSortField::DateAdded, SortDirection::Descending),
                (StorageSortField::Storage, SortDirection::Ascending),
            ] {
                let sort = StorageSortCriterion { field, direction };
                let name = format!("{name}_{field:?}_{direction:?}");
                let filter = StorageFilter::All;
                measure_storage_list(db, "storage_search", &name, found, &sort, filter, text).await;
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
    #[expect(
        clippy::disallowed_methods,
        reason = "file system: seeds the kept originals under target/bench-seed, files no library holds"
    )]
    pub(crate) async fn seed(db: &Store, originals: &std::path::Path, start: u64, end: u64) {
        let records = bae_records::test_fixtures::test_records(db);
        db.write(move |sql| {
            bulk_library::insert_on(&sql, start, end)?;
            let releases: Vec<String> = (start..end).map(bulk_library::release_id).collect();
            bae_transfers::tables::bulk::queue_downloads_on(&sql, start, &releases)?;
            let tracks: Vec<String> = (start..end).map(bulk_library::track_id).collect();
            bae_playback::tables::fill_context_lane_on(&sql, start, &tracks)?;
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

    /// What the downloads list's reads cost.
    async fn measure_downloads(db: &Store, n: u64) {
        measure_reads(
            db,
            "downloads",
            "queued",
            n,
            bae_transfers::download_list::download_key_at_on,
            |sql, window| {
                Ok(bae_transfers::download_list::download_window_on(sql, window)?.rows.len())
            },
            bae_transfers::download_list::downloads_before_on,
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
    // The schema's own text, which only a bare connection reads.
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    crate::TEST_SCHEMA.open_bare_library(&connection);
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'trigger' \
             AND name IN ('artist_listing_added', 'album_credit_shown_added', 'track_added') \
             ORDER BY name",
        )
        .unwrap();
    let waiting: Vec<String> =
        statement.query_map([], |row| row.get(0)).unwrap().map(Result::unwrap).collect();
    assert!(waiting.is_empty(), "triggers waiting for a parent: {waiting:?}");
}
