//! The Storage Manager list: its counts, sizes and sorts as release files
//! change, its search, and every storage sort and the download queue read
//! from an index.

use crate::tests::{both_ways, library};
use crate::*;
use bae_records::test_fixtures::aggregate_ordering::aggregate_db;
use bae_records::test_fixtures::album_listing::listed_db;
use bae_records::test_fixtures::bulk_library;
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::storage_listing::*;
use bae_store::test_fixtures::trigger_order::{
    count_refreshes, first_insert, run_trigger_first, watch_first_insert, RefreshCount,
};
// The listing's albums, not the root fixtures' of the same names.
use bae_records::test_fixtures::storage_listing::{ALBUM_A, ALBUM_B};
use bae_records::test_fixtures::*;

/// The Storage Manager list's release ids in `field` order, ascending.
async fn page(db: &Store, field: StorageSortField) -> Vec<String> {
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
    let (db, tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    write_library(&records).await;
    // The first release has two files, of 10 and 20 bytes; the second none.
    for file in [
        ("d3edb846-9b0b-4a64-b0f6-3f117b686bc0", RELEASE_EARLY, "cover.jpg", 10),
        ("161dd6f3-3bdd-4312-b5d3-8b9d83b53694", RELEASE_EARLY, "back.jpg", 20),
    ] {
        insert_file(&db, &tmp, file).await;
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

    // The second release's import stages three files, of 1, 2 and 3 bytes.
    for file in [
        ("f5008044-8ffe-4227-bdc3-238f85a6a4a6", RELEASE_LATE, "one.jpg", 1),
        ("cda4c2ec-c6fa-422c-a224-d4f56ca5f3f3", RELEASE_LATE, "two.jpg", 2),
        ("4b6e1f0a-3c2d-4e5f-8a9b-0c1d2e3f4a5b", RELEASE_LATE, "three.jpg", 3),
    ] {
        insert_file(&db, &tmp, file).await;
    }
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        36
    );
    assert_eq!(page(&db, StorageSortField::FileCount).await, ids(&[RELEASE_EARLY, RELEASE_LATE]));
    assert_eq!(page(&db, StorageSortField::TotalSize).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    // Retitling the first album sorts its release after the second.
    records.rename_album_for_test(ALBUM_A, "Album Title C").await.unwrap();
    assert_eq!(page(&db, StorageSortField::AlbumTitle).await, ids(&[RELEASE_LATE, RELEASE_EARLY]));

    delete_release(&records, RELEASE_EARLY).await;
    assert_eq!(
        crate::LibraryViews::new(db.clone()).get_storage_count(StorageFilter::All).await.unwrap(),
        1
    );
    assert_eq!(
        crate::LibraryViews::new(db.clone())
            .get_storage_total_size(StorageFilter::All)
            .await
            .unwrap(),
        6
    );
}

const FIELDS: [StorageSortField; 7] = [
    StorageSortField::AlbumTitle,
    StorageSortField::ArtistNames,
    StorageSortField::Media,
    StorageSortField::FileCount,
    StorageSortField::TotalSize,
    StorageSortField::Storage,
    StorageSortField::DateAdded,
];

/// The plan of the compiled `statement`, whose parameters stay unbound.
async fn compiled_plan(db: &Store, statement: &'static str) -> Vec<crate::tests::PlanStep> {
    db.read(move |sql| {
        let unbound: &[(&str, &dyn rusqlite::ToSql)] = &[];
        crate::tests::plan_steps(&sql, statement, unbound)
    })
    .await
    .unwrap()
}

/// Whatever the list sorts by, whole or filtered, its window from a key or
/// from the top reads the index of that sort in order: the statement sorts
/// nothing.
#[tokio::test]
async fn every_storage_sort_seeks_its_window_through_its_index() {
    use crate::storage::{kept_window_reads::*, top_reads::*, window_reads::*};
    let (db, _tmp) = library().await;
    let windows = [
        (AllWindowTitle::SQL, "title"),
        (AllWindowTitleBack::SQL, "title_back"),
        (AllWindowArtist::SQL, "artist"),
        (AllWindowArtistBack::SQL, "artist_back"),
        (AllWindowMedia::SQL, "media"),
        (AllWindowMediaBack::SQL, "media_back"),
        (AllWindowFiles::SQL, "files"),
        (AllWindowFilesBack::SQL, "files_back"),
        (AllWindowSize::SQL, "size"),
        (AllWindowSizeBack::SQL, "size_back"),
        (AllWindowStorage::SQL, "storage"),
        (AllWindowStorageBack::SQL, "storage_back"),
        (AllWindowAdded::SQL, "added"),
        (AllWindowAddedBack::SQL, "added_back"),
        (KeptWindowTitle::SQL, "stored_title"),
        (KeptWindowTitleBack::SQL, "stored_title_back"),
        (KeptWindowArtist::SQL, "stored_artist"),
        (KeptWindowArtistBack::SQL, "stored_artist_back"),
        (KeptWindowMedia::SQL, "stored_media"),
        (KeptWindowMediaBack::SQL, "stored_media_back"),
        (KeptWindowFiles::SQL, "stored_files"),
        (KeptWindowFilesBack::SQL, "stored_files_back"),
        (KeptWindowSize::SQL, "stored_size"),
        (KeptWindowSizeBack::SQL, "stored_size_back"),
        (KeptWindowStorage::SQL, "stored_storage"),
        (KeptWindowStorageBack::SQL, "stored_storage_back"),
        (KeptWindowAdded::SQL, "stored_added"),
        (KeptWindowAddedBack::SQL, "stored_added_back"),
        (AllTopTitle::SQL, "title"),
        (AllTopTitleBack::SQL, "title_back"),
        (AllTopArtist::SQL, "artist"),
        (AllTopArtistBack::SQL, "artist_back"),
        (AllTopMedia::SQL, "media"),
        (AllTopMediaBack::SQL, "media_back"),
        (AllTopFiles::SQL, "files"),
        (AllTopFilesBack::SQL, "files_back"),
        (AllTopSize::SQL, "size"),
        (AllTopSizeBack::SQL, "size_back"),
        (AllTopStorage::SQL, "storage"),
        (AllTopStorageBack::SQL, "storage_back"),
        (AllTopAdded::SQL, "added"),
        (AllTopAddedBack::SQL, "added_back"),
        (KeptTopTitle::SQL, "stored_title"),
        (KeptTopTitleBack::SQL, "stored_title_back"),
        (KeptTopArtist::SQL, "stored_artist"),
        (KeptTopArtistBack::SQL, "stored_artist_back"),
        (KeptTopMedia::SQL, "stored_media"),
        (KeptTopMediaBack::SQL, "stored_media_back"),
        (KeptTopFiles::SQL, "stored_files"),
        (KeptTopFilesBack::SQL, "stored_files_back"),
        (KeptTopSize::SQL, "stored_size"),
        (KeptTopSizeBack::SQL, "stored_size_back"),
        (KeptTopStorage::SQL, "stored_storage"),
        (KeptTopStorageBack::SQL, "stored_storage_back"),
        (KeptTopAdded::SQL, "stored_added"),
        (KeptTopAddedBack::SQL, "stored_added_back"),
    ];
    for (statement, name) in windows {
        let steps = compiled_plan(&db, statement).await;
        let own: Vec<&str> =
            steps.iter().filter(|step| step.own).map(|step| step.detail.as_str()).collect();
        let first = own.first().expect("the statement has a plan");
        let index = format!("INDEX idx_release_listing_{name}");
        let reads_index = first
            .strip_suffix(&index)
            .is_some_and(|read| read.starts_with("SCAN entry USING "))
            || (first.starts_with("SEARCH entry USING ") && first.contains(&format!("{index} (")));
        assert!(reads_index, "{steps:?}");
        assert!(!own.iter().any(|step| step.starts_with("USE TEMP B-TREE")), "{name}: {steps:?}");
    }
}

/// A searched window, from a key or the top, count of rows before a key and
/// jump never scan the listing: they reach it by the albums the search finds.
#[tokio::test]
async fn a_searched_storage_read_never_scans_the_listing() {
    use crate::storage::found_reads::*;
    let (db, _tmp) = library().await;
    for statement in
        [FoundWindow::SQL, FoundTop::SQL, FoundBefore::SQL, FoundKeyAt::SQL, FoundTotals::SQL]
    {
        let steps = compiled_plan(&db, statement).await;
        assert!(
            !steps.iter().any(|step| ["entry", "sorted", "counted", "kept"].iter().any(|alias| {
                step.detail == format!("SCAN {alias}")
                    || step.detail.starts_with(&format!("SCAN {alias} "))
            })),
            "{steps:?}"
        );
    }
}

/// The ids of every row the list under `sort`, `filter` and `search` shows,
/// in order, read as one window from its top.
async fn listed(
    views: &LibraryViews,
    sort: &StorageSortCriterion,
    filter: StorageFilter,
    search: &str,
) -> Vec<String> {
    let mut window = views.subscribe_storage_window(sort, filter, search, ListWindow::top(1_000));
    ids_of(&window.next_inline().await.expect("the query stays open").result.unwrap())
}

/// For each sort, the statement compiled for it and the searched list's,
/// which takes the sort as a parameter, show the releases in one order, and
/// the rows before a key, the jump, a window from a key, a range and the
/// positions agree with it, whole, filtered and searched.
#[tokio::test]
async fn every_storage_read_agrees_on_each_sort_s_order() {
    let (db, _tmp) = library().await;
    let views = LibraryViews::new(db.clone());
    for field in FIELDS {
        for (field, direction) in both_ways(field) {
            let sort = StorageSortCriterion { field, direction };
            let all = listed(&views, &sort, StorageFilter::All, "").await;
            assert_eq!(all.len(), 300, "{field:?} {direction:?}");
            // Every release of the library is local, and every title holds
            // "album".
            for (filter, search) in [(StorageFilter::Local, ""), (StorageFilter::All, "album")] {
                let what = format!("{field:?} {direction:?} {filter:?} {search:?}");
                assert_eq!(listed(&views, &sort, filter, search).await, all, "{what}");
            }
            for (filter, search) in [
                (StorageFilter::All, ""),
                (StorageFilter::Local, ""),
                (StorageFilter::All, "album"),
            ] {
                let what = format!("{field:?} {direction:?} {filter:?} {search:?}");
                let key = views
                    .storage_key_at(&sort, filter, search, 150)
                    .await
                    .unwrap()
                    .expect("the list holds rows");
                let mut position =
                    views.subscribe_storage_position(&sort, filter, search, key.clone());
                assert_eq!(position.next_inline().await.unwrap().result.unwrap(), 150, "{what}");
                let mut window =
                    views.subscribe_storage_window(&sort, filter, search, window_of_five_from(key));
                let read = window.next_inline().await.unwrap().result.unwrap();
                assert_eq!(ids_of(&read), all[150..155], "{what}");
                let between = views
                    .storage_ids_between(&sort, filter, search, &all[40], &all[20])
                    .await
                    .unwrap();
                assert_eq!(between, all[20..=40], "{what}");
                let last = views.storage_key_at(&sort, filter, search, 10_000).await.unwrap();
                assert_eq!(
                    last,
                    views.storage_key_at(&sort, filter, search, 299).await.unwrap(),
                    "{what}"
                );
            }
        }
    }
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    for (filter, search) in
        [(StorageFilter::Remote, ""), (StorageFilter::All, "nothing holds this")]
    {
        assert!(listed(&views, &sort, filter, search).await.is_empty());
        assert_eq!(views.storage_key_at(&sort, filter, search, 0).await.unwrap(), None);
    }
}

/// The count, the size and the list of a search under a filter agree: the
/// count is how many releases the list holds, the size is their files', for
/// a search found in titles, in the names the credits show, in a name a
/// credit prints, in titles and names at once, and in no text.
#[tokio::test]
async fn a_searched_count_and_list_find_the_same_releases() {
    let (db, _tmp) = library().await;
    // The first album's credit prints a name of its own.
    test_records(&db)
        .save_release_metadata_for_test(&bulk_library::release_id(0), |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Printed Name"));
        })
        .await
        .unwrap();
    let views = LibraryViews::new(db.clone());
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    for text in ["album 1", "artist 0", "printed", "1", "nothing holds this"] {
        for filter in [StorageFilter::All, StorageFilter::Local] {
            let mut count = views.subscribe_storage_count(filter, text);
            let counted = count.next_inline().await.expect("the query stays open").unwrap();
            let mut window =
                views.subscribe_storage_window(&sort, filter, text, ListWindow::top(1_000));
            let read = window.next_inline().await.expect("the query stays open").result.unwrap();
            assert_eq!(counted.rows as usize, read.rows.len(), "{text:?} {filter:?}: count");
            assert_eq!(
                counted.total_size,
                read.rows.iter().map(|(row, _)| row.release.total_size).sum::<u64>(),
                "{text:?} {filter:?}: size"
            );
            assert_eq!(
                read.rows.is_empty(),
                text == "nothing holds this",
                "{text:?} {filter:?} finds {}",
                read.rows.len()
            );
        }
    }
}

/// A window of five rows that starts at `from`.
fn window_of_five_from(from: bae_store::list_windows::OrderKey) -> ListWindow {
    ListWindow { from: Some(from), limit: 5 }
}

fn ids_of(read: &WindowRead<DbStorageRow>) -> Vec<String> {
    read.rows.iter().map(|(row, _key)| row.release.id.clone()).collect()
}

/// A live Storage Manager search window that starts at a key holds what a
/// fresh read from that key holds. An album's new credit name brings the
/// album into the search above the window. The window still holds the same
/// rows, because it starts at the same key.
#[tokio::test]
async fn a_search_window_from_a_key_holds_its_rows_across_a_printed_name() {
    let (db, _tmp) = library().await;
    let records = test_records(&db);
    // The first artist's albums sort by its sort name from now on, filled in
    // by a credit naming it with one, so the name its album's credit prints
    // below changes only what the search finds.
    let artist = "00000000-0001-4000-8000-000000000000";
    let met = records
        .find_or_create_artists(&[DbArtist {
            sort_name: Some("Artist 000000".to_string()),
            ..credit("Artist 000000")
        }])
        .await
        .unwrap();
    assert_eq!(met, [artist]);
    let views = crate::LibraryViews::new(db.clone());
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let from = views
        .storage_key_at(&sort, StorageFilter::All, "album 1", 10)
        .await
        .unwrap()
        .expect("the search finds more than ten releases");
    let fresh = |from: bae_store::list_windows::OrderKey| {
        let views = &views;
        let sort = &sort;
        async move {
            let mut query = views.subscribe_storage_window(
                sort,
                StorageFilter::All,
                "album 1",
                window_of_five_from(from),
            );
            ids_of(&query.next_inline().await.unwrap().result.unwrap())
        }
    };
    let mut live = views.subscribe_storage_window(
        &sort,
        StorageFilter::All,
        "album 1",
        window_of_five_from(from.clone()),
    );
    let mut count = views.subscribe_storage_count(StorageFilter::All, "album 1");
    let before = ids_of(&live.next_inline().await.unwrap().result.unwrap());
    let found = count.next_inline().await.unwrap().unwrap();
    assert!(found.rows > 20, "the search finds {} releases", found.rows);
    assert_eq!(before.len(), 5);
    assert_eq!(before, fresh(from.clone()).await);

    records
        .save_release_metadata_for_test(&bulk_library::release_id(0), |rows| {
            rows.album_artists[0].credit =
                Some(bae_records::credits::CreditName::of("Album 1 Artist"));
        })
        .await
        .unwrap();
    let counted = loop {
        let counted = count.next_inline().await.unwrap().unwrap();
        if counted.rows != found.rows {
            break counted;
        }
    };
    assert_eq!(counted.rows, found.rows + 1, "the album printing the new name joins the list");
    // The window starts at the same key, so the rows it holds are the ones it
    // held, whichever position that key has now.
    assert_eq!(fresh(from).await, before);
}

#[tokio::test]
async fn storage_page_orders_album_aggregate_columns_inside_aggregates() {
    let (db, _tmp) = aggregate_db(crate::TEST_SCHEMA).await;
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
        row.album.artist_line.plain_text(),
        "Artist Name Primary, Artist Name First, Artist Name Second"
    );
    assert_eq!(row.album.release_ids, vec![RELEASE_Z, RELEASE_A, RELEASE_B]);
}

/// How many rows the library's tables, the listings' triggers included,
/// change when `release` is added to Album Title One.
async fn rows_changed_adding_release(db: &Store, release: &str) -> i64 {
    test_records(db)
        .rows_changed_inserting_release_for_test(&DbRelease::new_test(
            bae_records::test_fixtures::album_listing::ALBUM_ONE,
            release,
        ))
        .await
        .unwrap()
}

/// Adding a release to an album lists that release and leaves the album's
/// other releases' listing rows alone: what it costs doesn't grow with the
/// releases the album already holds, as a box set's would.
#[tokio::test]
async fn adding_a_release_rewrites_no_other_release_of_its_album() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    for i in 2..22 {
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid(&format!("box-set-{i}")))
            .await;
    }
    let twenty_second =
        rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-22")).await;
    assert_eq!(twenty_second, second, "the 22nd release changes as many rows as the second");
}

/// Renaming an album renames every one of its releases' listing rows, so
/// the Storage Manager sorts them by the new title.
#[tokio::test]
async fn renaming_an_album_renames_its_releases_listing_rows() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    rows_changed_adding_release(&db, &bae_util::test_ids::test_uuid("box-set-1")).await;
    test_records(&db)
        .rename_album_for_test(
            bae_records::test_fixtures::album_listing::ALBUM_ONE,
            "Album Title Renamed",
        )
        .await
        .unwrap();
    let renamed: Vec<bool> = db
        .call(|conn| {
            Ok(conn.query(
                "SELECT album_title_sort_key = bae_name_key('Album Title Renamed', \
                                                       (SELECT language FROM name_sort)) \
                 FROM release_listing \
                 WHERE album_id = '07d22f7d-3980-4c81-9d27-8fcd50ee3464'",
                [],
                |row| row.get(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(renamed, vec![true; 2]);
}

/// The trigger that runs first of the two on a release arriving, which
/// SQLite runs in no order it names: the listing's tests hold under each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirstOnArrival {
    /// `release_added`, which lists the release's album, then the release.
    Listing,
    /// `release_storage_release_added`, which writes the release's storage
    /// row.
    Storage,
}

impl FirstOnArrival {
    const BOTH: [Self; 2] = [Self::Listing, Self::Storage];

    fn trigger(self) -> &'static str {
        match self {
            Self::Listing => "release_added",
            Self::Storage => "release_storage_release_added",
        }
    }

    /// The table the trigger writes first.
    fn writes(self) -> &'static str {
        match self {
            Self::Listing => "album_listing",
            Self::Storage => "release_storage",
        }
    }
}

/// An album and a release that tell which trigger runs first as a release
/// arrives, apart from the tests' own.
const PROBE_ALBUM: &str = "6d1f3a2b-4c5e-4f60-8a7b-9c0d1e2f3a01";
const PROBE_RELEASE: &str = "6d1f3a2b-4c5e-4f60-8a7b-9c0d1e2f3a02";

/// A library on which `first`'s trigger runs first as a release arrives,
/// with the count of every release whose listing row a write asks to
/// refresh.
async fn counting_refreshes(
    first: FirstOnArrival,
) -> (Store, bae_util::test_files::TestFolder, RefreshCount) {
    let schema = count_refreshes(
        watch_first_insert(
            run_trigger_first(crate::TEST_SCHEMA, first.trigger()),
            &["album_listing", "release_storage"],
        ),
        "release_listing_refresh",
        "release_id",
    );
    let (db, tmp) = bae_store::test_fixtures::temp_db(schema).await;
    RecordsFixture::new()
        .album(PROBE_ALBUM, "Album Title Probe", None, &[])
        .release(PROBE_ALBUM, PROBE_RELEASE)
        .write(&test_records(&db))
        .await;
    assert_eq!(first_insert(&db).await.as_deref(), Some(first.writes()));
    (db, tmp, RefreshCount::of_view("release_listing_refresh"))
}

/// The album `release`'s listing row names, None while it has none.
async fn listed_album(db: &Store, release: &str) -> Option<String> {
    let release = release.to_string();
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT (SELECT album_id FROM release_listing WHERE release_id = ?1)",
            [&release],
            |row| row.get(0),
        )?)
    })
    .await
    .unwrap()
}

/// Album A, holding the early release.
fn album_a_with_early() -> RecordsFixture {
    RecordsFixture::new().album(ALBUM_A, "Album Title A", None, &[]).release(ALBUM_A, RELEASE_EARLY)
}

/// The late release, after album A's early one.
fn late_on_album_a() -> DbRelease {
    DbRelease::new_test(ALBUM_A, RELEASE_LATE)
}

/// A release arriving writes its listing row with one refresh, in the write
/// that adds it: after its album, which lists that album too, and as it
/// joins an album already listed.
#[tokio::test]
async fn a_release_arriving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);

        album_a_with_early().write(&records).await;
        assert_eq!(
            listed_album(&db, RELEASE_EARLY).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_EARLY).await, 1, "{first:?} first");

        records.insert_release(&late_on_album_a()).await.unwrap();
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
        assert_eq!(refreshes.of(&db, RELEASE_EARLY).await, 1, "{first:?} first");
    }
}

/// `release` moving from album `from` to album `to` as its records file it
/// there, `to` written by the same write when `new_album` says.
async fn move_release(
    records: &bae_records::Records,
    release: &str,
    (from, to): (&str, &str),
    new_album: Option<DbAlbum>,
) {
    records
        .set_records_atomic(bae_records::RecordsChange {
            release_id: release.to_string(),
            records: Vec::new(),
            current_album_id: from.to_string(),
            target_album_id: to.to_string(),
            new_album,
        })
        .await
        .unwrap();
}

/// A release moving writes its listing row with one refresh: to an album the
/// same write adds, which lists that album and its releases, and to an album
/// already listed.
#[tokio::test]
async fn a_release_moving_refreshes_its_listing_once() {
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);
        album_a_with_early().write(&records).await;
        records.insert_release(&late_on_album_a()).await.unwrap();
        refreshes.clear(&db).await;

        let album_b = DbAlbum {
            id: ALBUM_B.to_string(),
            title: "Album Title B".to_string(),
            year: None,
            is_compilation: false,
            created_at: time::UtcDateTime::now(),
        };
        move_release(&records, RELEASE_LATE, (ALBUM_A, ALBUM_B), Some(album_b)).await;
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_B),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");

        refreshes.clear(&db).await;
        move_release(&records, RELEASE_LATE, (ALBUM_B, ALBUM_A), None).await;
        assert_eq!(
            listed_album(&db, RELEASE_LATE).await.as_deref(),
            Some(ALBUM_A),
            "{first:?} first"
        );
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
    }
}

/// `release`'s listing row's media columns.
async fn listed_media(db: &Store, release: &str) -> (bool, Option<String>) {
    let release = release.to_string();
    db.read(move |sql| {
        Ok(sql.query_row(
            "SELECT media_empty, media_key FROM release_listing WHERE release_id = ?1",
            [&release],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    })
    .await
    .unwrap()
}

/// `count` of `medium`.
fn carriers(medium: bae_types::pressing::Medium, count: u32) -> bae_types::pressing::MediaCount {
    bae_types::pressing::MediaCount { medium, count }
}

/// A release's media arriving, changing and leaving, the last two by a change
/// of what its pressing is, set its listing row's media, in order, without
/// refreshing the rest of the row.
#[tokio::test]
async fn a_release_s_media_rows_set_its_listing_s_media() {
    use bae_types::pressing::Medium;
    for first in FirstOnArrival::BOTH {
        let (db, _tmp, refreshes) = counting_refreshes(first).await;
        let records = test_records(&db);
        RecordsFixture::new()
            .album(ALBUM_A, "Album Title A", None, &[])
            .release(ALBUM_A, RELEASE_LATE)
            .edit(|rows| {
                rows.releases[0].pressing.facts.media =
                    vec![carriers(Medium::Vinyl, 2), carriers(Medium::Cd, 1)];
            })
            .write(&records)
            .await;
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 1, "{first:?} first");
        assert_eq!(
            listed_media(&db, RELEASE_LATE).await,
            (false, Some("vinyl:00002,cd:00001".to_string()))
        );

        refreshes.clear(&db).await;
        let changed = records
            .change_release_pressing_facts(RELEASE_LATE, |facts| {
                facts.media = vec![carriers(Medium::Vinyl, 3), carriers(Medium::Cd, 1)];
            })
            .await
            .unwrap();
        assert!(changed, "the library holds the release");
        assert_eq!(
            listed_media(&db, RELEASE_LATE).await,
            (false, Some("vinyl:00003,cd:00001".to_string()))
        );

        let changed = records
            .change_release_pressing_facts(RELEASE_LATE, |facts| facts.media = Vec::new())
            .await
            .unwrap();
        assert!(changed, "the library holds the release");
        assert_eq!(listed_media(&db, RELEASE_LATE).await, (true, None));
        assert_eq!(refreshes.of(&db, RELEASE_LATE).await, 0, "{first:?} first");
    }
}

/// A release's media written on one device sort the release in the Storage
/// Manager's listing on another: the listing's media key follows the rows,
/// in order.
#[test]
fn a_media_edit_from_another_device_sorts_its_release() {
    use bae_types::pressing::Medium;
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let release = "0252dedb-ee39-4547-8803-438dbeb57a64";
        RecordsFixture::new()
            .album(ALBUM_A, "Album Title A", Some(2026), &[])
            .release(ALBUM_A, release)
            .write(&test_records(devices.a()))
            .await;
        let changed = test_records(devices.a())
            .change_release_pressing_facts(release, |facts| {
                facts.media = vec![carriers(Medium::Vinyl, 2), carriers(Medium::Cd, 1)];
            })
            .await
            .unwrap();
        assert!(changed, "the library holds the release");
        devices.converge("SELECT id, position, medium, count FROM release_media ORDER BY id").await;

        let key: Option<String> = devices
            .b()
            .read(move |sql| {
                Ok(sql.query_row(
                    "SELECT media_key FROM release_listing WHERE release_id = ?1",
                    [release],
                    |row| row.get(0),
                )?)
            })
            .await
            .unwrap();
        assert_eq!(key.as_deref(), Some("vinyl:00002,cd:00001"));
    });
}

/// The Storage Manager's selection query returns which of the requested
/// releases the list contains, whether the list holds all of a set, and the
/// one release the list holds when the list holds only one of them: the apps
/// read these and never search the rows.
#[tokio::test]
async fn a_storage_selection_answers_which_releases_the_list_holds() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let records = test_records(&db);
    write_library(&records).await;
    let views = crate::LibraryViews::new(db.clone());
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<BTreeSet<_>>();
    let answer = |filter: StorageFilter, asked: BTreeSet<String>| {
        let views = views.clone();
        async move {
            let mut live = views.subscribe_storage_selection(filter, "", asked.clone());
            let value = live.next_inline().await.expect("the query stays open").result.unwrap();
            crate::storage::resolve_storage_selection(&asked, value)
        }
    };

    let both = answer(StorageFilter::All, ids(&[RELEASE_EARLY, RELEASE_LATE])).await;
    assert_eq!(both.selected.len(), 2);
    assert_eq!(both.sole, None, "two held releases have no sole one");

    let one = answer(StorageFilter::All, ids(&[RELEASE_EARLY])).await;
    let sole = one.sole.expect("the list holds one of the asked releases");
    assert_eq!(sole.release_id, RELEASE_EARLY);
    let title = db
        .read(|sql| {
            Ok(sql.query_row(
                "SELECT album.title FROM releases release \
                 JOIN albums album ON album.id = release.album_id WHERE release.id = ?1",
                [RELEASE_EARLY],
                |row| row.get::<_, String>(0),
            )?)
        })
        .await
        .unwrap();
    assert_eq!(sole.album_title, title, "the sole release carries its album's title");

    let unknown = answer(StorageFilter::All, ids(&[RELEASE_LATE, "no-such-release"])).await;
    assert_eq!(
        unknown.sole.map(|sole| sole.release_id).as_deref(),
        Some(RELEASE_LATE),
        "the sole release leaves out an asked release the list does not show"
    );

    let filtered_out = answer(StorageFilter::Remote, ids(&[RELEASE_EARLY])).await;
    assert_eq!(filtered_out.sole, None);
    assert!(filtered_out.selected.is_empty());
}

/// A range selected with Shift and Select All take their releases from SQL:
/// every release the list shows from one release to another, both included,
/// in the list's order whichever end comes first, the one it holds when it
/// holds only one end, and every release it shows.
#[tokio::test]
async fn the_storage_list_answers_the_releases_between_two_and_all_of_them() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    db.write(|sql| {
        bulk_library::insert_on(&sql, 0, 6)?;
        Ok::<(), bae_records::IdentityWriteError>(())
    })
    .await
    .unwrap();
    let views = crate::LibraryViews::new(db.clone());
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let all = page(&db, StorageSortField::AlbumTitle).await;
    assert_eq!(all.len(), 6);
    let between = |from: &str, to: &str| {
        let (views, from, to) = (views.clone(), from.to_string(), to.to_string());
        async move {
            views.storage_ids_between(&sort, StorageFilter::All, "", &from, &to).await.unwrap()
        }
    };
    assert_eq!(between(&all[1], &all[4]).await, all[1..=4]);
    assert_eq!(between(&all[4], &all[1]).await, all[1..=4]);
    assert_eq!(between(&all[2], "no-such-release").await, all[2..=2]);
    assert!(between("no-such-release", "nor-this-one").await.is_empty());

    let mut every = views.storage_ids(StorageFilter::All, "").await.unwrap();
    every.sort();
    let mut shown = all.clone();
    shown.sort();
    assert_eq!(every, shown);
}

/// A Storage Manager window that holds as many rows as its limit reruns for
/// a release that sorts at or before its last row, and not for one that
/// sorts after it.
#[tokio::test]
async fn a_release_after_a_full_storage_window_reruns_nothing() {
    let (db, _tmp) = bae_store::test_fixtures::temp_db(crate::TEST_SCHEMA).await;
    let album = |at: u32| format!("7c4d2e6a-8b0f-4a2c-9d3e-5f7a9b1c3d{at:02}");
    let release = |at: u32| format!("7c4d2e6a-8b0f-4a2c-9d3e-5f7a9b1c3e{at:02}");
    let mut fixture = RecordsFixture::new();
    for (at, title) in [(0, "Album B"), (1, "Album C"), (2, "Album D")] {
        fixture = fixture.album(&album(at), title, None, &[]).release(&album(at), &release(at));
    }
    fixture.write(&test_records(&db)).await;
    let sort = StorageSortCriterion {
        field: StorageSortField::AlbumTitle,
        direction: SortDirection::Ascending,
    };
    let views = crate::LibraryViews::new(db.clone());
    // Album B and Album C.
    let mut live =
        views.subscribe_storage_window(&sort, StorageFilter::All, "", ListWindow::top(2));
    live.next_inline().await.expect("the query stays open").result.unwrap();

    RecordsFixture::new()
        .album(&album(3), "Album E", None, &[])
        .release(&album(3), &release(3))
        .write(&test_records(&db))
        .await;
    assert!(!live.is_marked_for_rerun(), "Album E sorts after the window's last row");

    RecordsFixture::new()
        .album(&album(4), "Album A", None, &[])
        .release(&album(4), &release(4))
        .write(&test_records(&db))
        .await;
    assert!(live.is_marked_for_rerun(), "Album A sorts first");
}

/// A window from the top holds the list's first rows under every sort, whole,
/// filtered and narrowed by a search.
#[tokio::test]
async fn every_storage_sorts_window_from_the_top_holds_its_first_rows() {
    let (db, _tmp) = library().await;
    db.read(|sql| {
        for sort in crate::tests::sorts_of(&FIELDS) {
            let (field, direction) = sort[0];
            let criterion = StorageSortCriterion { field, direction };
            for (filter, search) in [
                (StorageFilter::All, ""),
                (StorageFilter::Local, ""),
                (StorageFilter::All, "album"),
            ] {
                let list = crate::storage::StorageList::new(&criterion, filter, search);
                crate::tests::assert_top_is_first_rows(
                    &format!("{criterion:?} {filter:?} {search:?}"),
                    || crate::storage::storage_key_at_on(&sql, &list, 0),
                    |window| crate::storage::storage_window_on(&sql, &list, window),
                )?;
            }
        }
        Ok(())
    })
    .await
    .unwrap();
}
