//! The album grid's rows: how they sort, flat or grouped by artist, how a
//! release leaving or an artist merging moves them, and every sort's windows
//! read from an index.

use crate::album::album_browse_order;
use crate::tests::{assert_seeks_search, both_ways, key_at, library};
use crate::*;
use bae_records::test_fixtures::album_listing::*;
use bae_records::test_fixtures::artists_apart::{
    meet_musicbrainz_artist, merge_apart, musicbrainz_artist,
};
use bae_records::test_fixtures::library::RecordsFixture;
use bae_records::test_fixtures::{
    credit, delete_release, test_records, ARTIST_A, ARTIST_B, ARTIST_C,
};
use bae_records::DbArtist;
use bae_store::test_fixtures::temp_db;
use bae_store::{ListWindow, WindowRead};
use bae_util::test_channels::HANG_GUARD;
use std::collections::BTreeSet;

/// The grid's album ids in `field` order, `direction`.
async fn album_ids(db: &Store, field: AlbumSortField, direction: SortDirection) -> Vec<String> {
    crate::LibraryViews::new(db.clone())
        .get_album_page(&[AlbumSortCriterion { field, direction }], 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.id)
        .collect()
}

/// The artist list: each artist's id and album count, by name.
async fn artists(db: &Store) -> Vec<(String, i64)> {
    crate::LibraryViews::new(db.clone())
        .get_artist_page(&[], 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| (artist.artist.id, artist.album_count))
        .collect()
}

/// The grid's artist sections under `sort`, as their live query first
/// delivers them.
async fn sections_under(
    db: &Store,
    sort: &[AlbumSortCriterion],
) -> Vec<crate::LibraryBrowseSection> {
    crate::LibraryViews::new(db.clone())
        .subscribe_album_sections(sort)
        .next_inline()
        .await
        .expect("the query stays open")
        .unwrap()
}

/// The window of the grid's first `limit` rows.
fn top(limit: u64) -> ListWindow {
    ListWindow::top(limit)
}

/// The window of `limit` rows of the grid from the row at `key`.
fn from_key(key: OrderKey, limit: u64) -> ListWindow {
    ListWindow { from: Some(key), limit }
}

/// What the grid's live query of `window` first delivers.
async fn window_under(
    db: &Store,
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
    window: ListWindow,
) -> WindowRead<DbAlbumAppearance> {
    crate::LibraryViews::new(db.clone())
        .subscribe_album_window(sort, group_by_artist, window)
        .next_inline()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
}

/// A window's rows without their keys.
fn unkeyed(read: WindowRead<DbAlbumAppearance>) -> Vec<DbAlbumAppearance> {
    read.rows.into_iter().map(|(row, _key)| row).collect()
}

/// What the grid's live count first delivers.
async fn counts_of(db: &Store, group_by_artist: bool) -> AlbumCounts {
    crate::LibraryViews::new(db.clone())
        .subscribe_album_count(group_by_artist)
        .next_inline()
        .await
        .expect("the query stays open")
        .unwrap()
}

/// The grid's count reports whether the grid groups its rows by artist, so the
/// grid can choose its layout from the count before any rows arrive.
#[tokio::test]
async fn the_grid_count_says_whether_the_grid_groups() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    for grouped in [false, true] {
        assert_eq!(counts_of(&db, grouped).await.grouped, grouped);
    }
}

const SHUFFLE_ARTIST: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c01";
const SHUFFLE_ALBUM: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c02";
const SHUFFLE_RELEASE: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c03";
const SHUFFLE_ALBUM_WITH_TRACK: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c05";
const SHUFFLE_RELEASE_WITH_TRACK: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c06";
const SHUFFLE_TRACK: &str = "1f5e2a30-5b7e-4a52-9c3a-6a0f1d2b3c04";

/// Shuffling the library plays tracks, so the grid's count says whether the
/// library holds one: not for an empty library, nor for albums whose releases
/// hold none, and from the first track on, flat or grouped.
#[tokio::test]
async fn the_grid_count_says_whether_the_library_has_a_track_to_shuffle() {
    let (db, _tmp) = temp_db(crate::TEST_SCHEMA).await;
    for grouped in [false, true] {
        assert!(!counts_of(&db, grouped).await.can_shuffle, "an empty library");
    }

    RecordsFixture::new()
        .artist(SHUFFLE_ARTIST, "Shuffle Artist Name")
        .album(SHUFFLE_ALBUM, "Shuffle Album Title", Some(2026), &[SHUFFLE_ARTIST])
        .release(SHUFFLE_ALBUM, SHUFFLE_RELEASE)
        .write(&test_records(&db))
        .await;
    for grouped in [false, true] {
        let counts = counts_of(&db, grouped).await;
        assert_eq!(counts.albums, 1);
        assert!(!counts.can_shuffle, "an album with no tracks");
    }

    RecordsFixture::new()
        .album(SHUFFLE_ALBUM_WITH_TRACK, "Shuffle Track Album", Some(2026), &[SHUFFLE_ARTIST])
        .release(SHUFFLE_ALBUM_WITH_TRACK, SHUFFLE_RELEASE_WITH_TRACK)
        .tracks(SHUFFLE_RELEASE_WITH_TRACK, &[(SHUFFLE_TRACK, "Shuffle Track Title")])
        .write(&test_records(&db))
        .await;
    for grouped in [false, true] {
        assert!(counts_of(&db, grouped).await.can_shuffle, "a library with a track");
    }
}

/// The key of the grid's row at `position`.
async fn key_of_row(
    db: &Store,
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
    position: u64,
) -> OrderKey {
    crate::LibraryViews::new(db.clone())
        .album_key_at(sort, group_by_artist, position)
        .await
        .unwrap()
        .expect("the grid has a row there")
}

/// The grid grouped by artist: each section's artist, its first row and its
/// length, and the album ids of the rows in order.
async fn grouped(db: &Store) -> (Vec<(String, u64, u64)>, Vec<String>) {
    let shown = window_under(db, &[], true, top(50)).await;
    (
        sections_under(db, &[])
            .await
            .iter()
            .map(|section| (section.id.clone(), section.first_position, section.album_count))
            .collect(),
        shown.rows.iter().map(|(row, _key)| row.album.id.clone()).collect(),
    )
}

#[tokio::test]
async fn album_grid_sorts_by_credits_as_their_artists_change() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    // Alpha's album, then Bravo's, then Bravo and Alpha's: the credit list
    // compares in order, so a longer list sorts after its first name alone.
    assert_eq!(
        album_ids(&db, AlbumSortField::Artist, SortDirection::Ascending).await,
        ids(&[ALBUM_TWO, ALBUM_ONE, ALBUM_THREE])
    );

    // A credit naming Alpha with a sort name gives Alpha the one it lacks.
    test_records(&db)
        .find_or_create_artists(&[DbArtist {
            sort_name: Some("Zulu Sort Name".to_string()),
            ..credit("Artist Name Alpha")
        }])
        .await
        .unwrap();
    assert_eq!(
        album_ids(&db, AlbumSortField::Artist, SortDirection::Ascending).await,
        ids(&[ALBUM_ONE, ALBUM_THREE, ALBUM_TWO])
    );
    assert_eq!(
        album_ids(&db, AlbumSortField::Artist, SortDirection::Descending).await,
        ids(&[ALBUM_TWO, ALBUM_THREE, ALBUM_ONE])
    );
}

#[tokio::test]
async fn album_grid_puts_unknown_years_last_ascending_and_first_descending() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    assert_eq!(
        album_ids(&db, AlbumSortField::Year, SortDirection::Ascending).await,
        ids(&[ALBUM_THREE, ALBUM_ONE, ALBUM_TWO])
    );
    assert_eq!(
        album_ids(&db, AlbumSortField::Year, SortDirection::Descending).await,
        ids(&[ALBUM_TWO, ALBUM_ONE, ALBUM_THREE])
    );
}

#[tokio::test]
async fn an_album_leaves_the_lists_with_its_last_release() {
    let (db, _tmp) = listed_db(crate::TEST_SCHEMA).await;
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 3);
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 2)]);

    delete_release(&test_records(&db), RELEASE_ONE).await;
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 2);
    assert!(!album_ids(&db, AlbumSortField::Title, SortDirection::Ascending)
        .await
        .contains(&ALBUM_ONE.to_string()));
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 1)]);
    assert_eq!(crate::LibraryViews::new(db.clone()).get_artist_count().await.unwrap(), 2);

    assert!(test_records(&db).find_album_by_id(ALBUM_ONE).await.unwrap().is_none());
}

/// Alpha and Bravo each take one Discogs id on a device of their own, as two
/// devices apart do; the next write naming it merges Bravo into Alpha, the
/// smaller id, and Bravo's albums move under Alpha.
#[test]
fn a_merge_moves_the_absorbed_artists_albums_under_the_survivor() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        write_listed(&test_records(db)).await;
        devices
            .settle("the library", async |_, db| {
                test_records(db).find_album_by_id(ALBUM_THREE).await.unwrap().is_some()
            })
            .await;
        // Newest first under each artist; Alpha's section before Bravo's.
        assert_eq!(
            grouped(db).await,
            (
                vec![(ALPHA.to_string(), 0, 2), (BRAVO.to_string(), 2, 2)],
                vec![
                    ALBUM_THREE.to_string(),
                    ALBUM_TWO.to_string(),
                    ALBUM_THREE.to_string(),
                    ALBUM_ONE.to_string(),
                ]
            )
        );

        merge_apart(devices, ("Artist Name Alpha", "Artist Name Bravo"), "discogs-listed").await;
        assert_eq!(
            grouped(db).await,
            (
                vec![(ALPHA.to_string(), 0, 3)],
                vec![ALBUM_THREE.to_string(), ALBUM_TWO.to_string(), ALBUM_ONE.to_string(),]
            ),
            "the third album credits the survivor twice and lists once"
        );
        assert_eq!(artists(db).await, vec![(ALPHA.to_string(), 3)]);
        let survivor = crate::LibraryViews::new(db.clone())
            .find_artist_detail(ALPHA)
            .await
            .unwrap()
            .expect("listed");
        assert_eq!(survivor.albums.len(), 3);
        assert!(crate::LibraryViews::new(db.clone())
            .find_artist_detail(BRAVO)
            .await
            .unwrap()
            .is_none());
    });
}

/// Album `index` of [`write_grouped`]'s library.
fn grouped_album(index: u32) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

/// Album `index`'s one release.
fn grouped_release(index: u32) -> String {
    format!("10000000-0000-4000-8000-{index:012}")
}

/// Artists A and B, sharing a sort name, and `third`, named "Artist Name C"
/// and sorted first; A's albums A and C, B's B and D and the third artist's
/// E, each on one release.
async fn write_grouped(records: &bae_records::Records, third: &str) {
    let mut library = RecordsFixture::new()
        .sorted_artist(ARTIST_A, "Artist Name A", "Same Sort Name")
        .sorted_artist(ARTIST_B, "Artist Name B", "Same Sort Name")
        .sorted_artist(third, "Artist Name C", "Last Artist");
    for ((artist, title), index) in [
        (ARTIST_A, "Album A"),
        (ARTIST_B, "Album B"),
        (ARTIST_A, "Album C"),
        (ARTIST_B, "Album D"),
        (third, "Album E"),
    ]
    .into_iter()
    .zip(0..)
    {
        library = library
            .album(&grouped_album(index), title, None, &[artist])
            .release(&grouped_album(index), &grouped_release(index));
    }
    library.write(records).await;
}

async fn grouped_library() -> (Store, tempfile::TempDir) {
    let (db, dir) = temp_db(crate::TEST_SCHEMA).await;
    write_grouped(&test_records(&db), ARTIST_C).await;
    (db, dir)
}

/// The grid grouped by artist and sorted by title: the albums' titles in
/// order.
async fn grouped_by_title(db: &Store) -> Vec<String> {
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let shown = window_under(db, &sort, true, top(20)).await;
    shown.rows.iter().map(|(row, _key)| row.album.title.clone()).collect()
}

/// The titles of the album grid's rows a page of it delivers.
fn page_titles(rows: crate::ListRows) -> Vec<String> {
    let crate::ListRows::Albums(rows) = rows else {
        panic!("the album grid's page holds albums");
    };
    rows.into_iter().map(|row| row.album.title).collect()
}

/// A page of the grid holds the rows from the row at its first place, none
/// past the grid's end, and follows a write to a row it holds; a page opened
/// after a row moves reads the rows at its places then.
#[tokio::test]
async fn a_page_holds_the_rows_from_its_first_place() {
    let (db, _dir) = grouped_library().await;
    let views = crate::LibraryViews::new(db.clone());
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let page = |offset| bae_store::ListPage { offset, limit: 2 };
    let first = |offset| {
        let values =
            crate::test_pages::page_values(views.subscribe_album_page(&sort, page(offset)));
        async move { crate::test_pages::next_page(&values).await }
    };
    assert_eq!(page_titles(first(1).await), ["Album B", "Album C"]);
    assert_eq!(page_titles(first(4).await), ["Album E"]);
    assert_eq!(page_titles(first(5).await), Vec::<String>::new());

    let live = crate::test_pages::page_values(views.subscribe_album_page(&sort, page(1)));
    assert_eq!(page_titles(crate::test_pages::next_page(&live).await), ["Album B", "Album C"]);
    test_records(&db).rename_album_for_test(&grouped_album(2), "Album C Renamed").await.unwrap();
    assert_eq!(
        page_titles(crate::test_pages::next_page(&live).await),
        ["Album B", "Album C Renamed"]
    );
    // "Album 0" sorts first.
    test_records(&db).rename_album_for_test(&grouped_album(3), "Album 0").await.unwrap();
    assert_eq!(page_titles(first(1).await), ["Album A", "Album B"]);
}

/// Renaming an album moves it among its artist's albums in the grid grouped
/// by artist.
#[tokio::test]
async fn a_renamed_album_moves_among_its_artists_albums() {
    let (db, _dir) = grouped_library().await;
    let titles = grouped_by_title(&db).await;
    // The third artist sorts first, then B and A, which share a sort name,
    // by id.
    assert_eq!(titles, ["Album E", "Album B", "Album D", "Album A", "Album C"]);

    test_records(&db).rename_album_for_test(&grouped_album(0), "Album Z").await.unwrap();
    let renamed = grouped_by_title(&db).await;
    assert_eq!(renamed, ["Album E", "Album B", "Album D", "Album C", "Album Z"]);
}

#[tokio::test]
async fn artist_sections_cover_unloaded_albums_and_match_page_and_reveal_order() {
    let (db, _dir) = grouped_library().await;
    for direction in [SortDirection::Ascending, SortDirection::Descending] {
        let sort = [
            AlbumSortCriterion { field: AlbumSortField::Artist, direction },
            AlbumSortCriterion {
                field: AlbumSortField::Title,
                direction: SortDirection::Descending,
            },
        ];
        let at_two = key_of_row(&db, &sort, true, 2).await;
        let value = window_under(&db, &sort, true, from_key(at_two, 1)).await;
        assert_eq!(value.rows.len(), 1);
        let counts = counts_of(&db, true).await;
        assert_eq!(counts.rows, 5);
        assert_eq!(counts.albums, 5);
        let sections = sections_under(&db, &sort).await;
        assert_eq!(sections.len(), 3);
        let page = LibraryViews::new(db.clone()).get_album_page(&sort, 0, 10).await.unwrap();
        let mut offset = 0;
        for section in &sections {
            assert_eq!(section.first_position, offset);
            let albums = &page[offset as usize..(offset + section.album_count) as usize];
            assert!(albums.iter().all(|album| album.artist_line.plain_text() == section.title));
            assert!(albums.windows(2).all(|pair| pair[0].title > pair[1].title));
            for (position, album) in albums.iter().enumerate() {
                assert_eq!(
                    LibraryViews::new(db.clone())
                        .get_album_index(&sort, &album.id, true)
                        .await
                        .unwrap(),
                    Some(offset + position as u64)
                );
            }
            offset += section.album_count;
        }
        assert_eq!(offset, 5);
        assert_eq!(value.rows[0].0.album.id, page[2].id);
    }
}

/// Moving the grid to other windows delivers its rows; a write that changes
/// an artist delivers the new sections from their own query.
#[tokio::test]
async fn the_artist_sections_are_their_own_live_query() {
    let (db, _dir) = grouped_library().await;
    let views = LibraryViews::new(db.clone());
    let mut browse = views.subscribe_album_window(&[], true, top(1));
    let mut sections = views.subscribe_album_sections(&[]);
    let requests = browse.requests();
    browse.next_inline().await.expect("the query stays open").result.unwrap();
    let first = sections.next_inline().await.expect("the query stays open").unwrap();
    assert_eq!(first.len(), 3);

    for position in [2, 0, 4] {
        let key = key_of_row(&db, &[], true, position).await;
        let window = from_key(key, 1);
        requests.set(window.clone()).unwrap();
        let moved = browse.next_inline().await.expect("the query stays open").result.unwrap();
        assert_eq!(moved.window, window);
    }

    delete_release(&test_records(&db), &grouped_release(4)).await;
    let changed = tokio::time::timeout(HANG_GUARD, sections.next_inline())
        .await
        .expect("deleting an artist's last release wakes the sections")
        .expect("the query stays open")
        .unwrap();
    assert_eq!(changed.len(), 2);
    assert_eq!(changed.iter().map(|section| section.album_count).sum::<u64>(), 4);
}

/// The MusicBrainz artist whose library artist is the renames test's third.
const RENAMED_MUSICBRAINZ_ARTIST: &str = "mb-artist-c";

/// Another device naming the third artist otherwise moves its section where
/// its new sort name goes; its last release deleted, its section goes.
#[test]
fn artist_sections_follow_renames_and_disappearing_groups() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        let third = musicbrainz_artist(RENAMED_MUSICBRAINZ_ARTIST);
        devices.pause().await;
        write_grouped(&test_records(db), &third).await;
        let sort = [AlbumSortCriterion {
            field: AlbumSortField::Artist,
            direction: SortDirection::Ascending,
        }];
        let mut sections = LibraryViews::new(db.clone()).subscribe_album_sections(&sort);
        let initial = sections.next_inline().await.expect("the query stays open").unwrap();
        assert_eq!(initial[0].id, third);

        meet_musicbrainz_artist(
            &test_records(devices.b()),
            RENAMED_MUSICBRAINZ_ARTIST,
            "Renamed Artist",
            Some("Z"),
        )
        .await;
        devices.resume().await;
        let renamed = loop {
            let shown = tokio::time::timeout(HANG_GUARD, sections.next_inline())
                .await
                .expect("the other device's name reaches the sections")
                .expect("the query stays open")
                .unwrap();
            if shown.iter().any(|section| section.title == "Renamed Artist") {
                break shown;
            }
        };
        assert_eq!(renamed[2].title, "Renamed Artist");
        assert_eq!(renamed[2].first_position, 4);

        delete_release(&test_records(db), &grouped_release(4)).await;
        let removed = sections.next_inline().await.expect("the query stays open").unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!removed.iter().any(|section| section.id == third));
    });
}

#[tokio::test]
async fn collaborations_appear_under_each_credit_but_count_once() {
    let (db, _dir) = grouped_library().await;
    let album_id = grouped_album(0);
    let album_id = album_id.as_str();
    let records = test_records(&db);
    records.credit_album_for_test(album_id, &[ARTIST_A, ARTIST_B]).await.unwrap();
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Descending }];
    let value = window_under(&db, &sort, true, top(20)).await;
    let counts = counts_of(&db, true).await;
    assert_eq!((counts.albums, counts.rows), (5, 6));
    let rows = &unkeyed(value);
    let mut appearances = Vec::new();
    let sections = sections_under(&db, &sort).await;
    for section in &sections {
        let start = section.first_position as usize;
        let end = start + section.album_count as usize;
        let albums = &rows[start..end];
        assert!(albums.windows(2).all(|pair| pair[0].album.title > pair[1].album.title));
        if albums.iter().any(|row| row.album.id == album_id) {
            appearances.push(section.id.as_str());
        }
    }
    assert_eq!(appearances.into_iter().collect::<BTreeSet<_>>(), [ARTIST_A, ARTIST_B].into());
    assert_eq!(
        LibraryViews::new(db.clone()).get_album_index(&sort, album_id, true).await.unwrap(),
        rows.iter().position(|row| row.album.id == album_id).map(|i| i as u64)
    );
    let flat = window_under(&db, &sort, false, top(20)).await;
    let counts = counts_of(&db, false).await;
    assert_eq!((counts.albums, counts.rows), (5, 5));
    assert_eq!(flat.rows.iter().filter(|(row, _key)| row.album.id == album_id).count(), 1);
    records.credit_album_for_test(album_id, &[ARTIST_A]).await.unwrap();
    let counts = counts_of(&db, true).await;
    assert_eq!((counts.albums, counts.rows), (5, 5));
}

/// An album crediting A twice, then B, lists once under each; once A and B
/// take one Discogs id on devices of their own and the next write naming it
/// merges A into B, the smaller id, the album lists once under B.
#[test]
fn merged_and_repeated_credits_make_one_appearance_per_artist() {
    bae_store::test_devices::run_two_store_test(crate::TEST_SCHEMA, async |devices| {
        let db = devices.a();
        let records = test_records(db);
        write_grouped(&records, ARTIST_C).await;
        let album_id = grouped_album(0);
        records.credit_album_for_test(&album_id, &[ARTIST_A, ARTIST_A, ARTIST_B]).await.unwrap();
        devices
            .settle("the library", async |_, db| {
                test_records(db).find_album_by_id(&grouped_album(4)).await.unwrap().is_some()
            })
            .await;
        let views = LibraryViews::new(db.clone());
        let mut counts = views.subscribe_album_count(true);
        assert_eq!(counts.next_inline().await.expect("the query stays open").unwrap().rows, 6);

        merge_apart(devices, ("Artist Name A", "Artist Name B"), "discogs-grouped").await;
        let merged = loop {
            let shown = tokio::time::timeout(HANG_GUARD, counts.next_inline())
                .await
                .expect("the merge reaches the count")
                .expect("the query stays open")
                .unwrap();
            if shown.rows == 5 {
                break shown;
            }
        };
        assert_eq!(merged.albums, 5);
        let sections = sections_under(db, &[]).await;
        assert_eq!(sections.len(), 2);
        assert!(sections.iter().all(|section| section.id != ARTIST_A));
        let rows = unkeyed(window_under(db, &[], true, top(20)).await);
        assert_eq!(rows.iter().filter(|row| row.album.id == album_id).count(), 1);
    });
}

/// The grid's selection comes back in the grid's order, whatever order its
/// albums were picked in: by title one way, by artist and then title grouped,
/// each album once, and an album the library no longer holds left out.
#[tokio::test]
async fn a_selection_is_read_in_the_grids_order() {
    let (db, _dir) = grouped_library().await;
    let album = grouped_album;
    let picked = vec![album(3), album(0), "no-such-album".to_string(), album(4), album(1)];
    let titles = |albums: Vec<crate::album_detail::AlbumSummary>| {
        albums.into_iter().map(|album| album.title).collect::<Vec<_>>()
    };
    let by_title =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Descending }];
    assert_eq!(
        titles(
            LibraryViews::new(db.clone())
                .albums_in_browse_order(&by_title, picked.clone(), false)
                .await
                .unwrap()
        ),
        ["Album E", "Album D", "Album B", "Album A"]
    );
    let by_artist = [
        AlbumSortCriterion { field: AlbumSortField::Artist, direction: SortDirection::Ascending },
        AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending },
    ];
    let grouped = LibraryViews::new(db.clone())
        .albums_in_browse_order(&by_artist, picked, true)
        .await
        .unwrap();
    let page = LibraryViews::new(db.clone()).get_album_page(&by_artist, 0, 10).await.unwrap();
    let expected: Vec<String> = page
        .into_iter()
        .filter(|album| grouped.iter().any(|picked| picked.id == album.id))
        .map(|album| album.title)
        .collect();
    assert_eq!(titles(grouped), expected);
}

/// Grouped by artist, a row names the album's other credits beside the
/// artist whose group lists it, and nothing when that artist is the only
/// credit; the flat grid names none.
#[tokio::test]
async fn a_grouped_row_names_the_albums_other_credits() {
    let (db, _dir) = grouped_library().await;
    test_records(&db)
        .credit_album_for_test(&grouped_album(0), &[ARTIST_A, ARTIST_B])
        .await
        .unwrap();
    let grouped = window_under(&db, &[], true, top(20)).await;
    let rows = &unkeyed(grouped);
    let mut credited_with = BTreeSet::new();
    let sections = sections_under(&db, &[]).await;
    for section in &sections {
        let start = section.first_position as usize;
        let end = start + section.album_count as usize;
        for row in &rows[start..end] {
            credited_with.insert((
                section.id.as_str(),
                row.album.title.as_str(),
                row.credited_with.as_ref().map(|line| line.plain_text()),
            ));
        }
    }
    assert_eq!(
        credited_with,
        [
            (ARTIST_A, "Album A", Some("Artist Name B".to_string())),
            (ARTIST_A, "Album C", None),
            (ARTIST_B, "Album A", Some("Artist Name A".to_string())),
            (ARTIST_B, "Album B", None),
            (ARTIST_B, "Album D", None),
            (ARTIST_C, "Album E", None),
        ]
        .into()
    );
    let flat = window_under(&db, &[], false, top(20)).await;
    assert!(flat.rows.iter().all(|(row, _key)| row.credited_with.is_none()));
}

/// An artist's group carries the artist's picture, and follows it as it
/// changes; an artist without one carries none.
#[tokio::test]
async fn an_artist_section_carries_the_artists_picture() {
    let (db, _dir) = grouped_library().await;
    let records = bae_records::test_fixtures::test_records(&db);
    let mut sections = LibraryViews::new(db.clone()).subscribe_album_sections(&[]);
    let initial = sections.next_inline().await.expect("the query stays open").unwrap();
    assert!(initial.iter().all(|section| section.image.is_none()));
    let version = bae_records::test_fixtures::live_query::write_artist_image(
        &records,
        ARTIST_B,
        "artist image bytes",
    )
    .await;
    let pictured = sections.next_inline().await.expect("the query stays open").unwrap();
    let images = pictured
        .iter()
        .map(|section| {
            (section.id.as_str(), section.image.as_ref().map(|image| image.version.as_str()))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        images,
        [(ARTIST_A, None), (ARTIST_B, Some(version.as_str())), (ARTIST_C, None)].into()
    );
}

/// Every sort's windows, flat and grouped by artist, are read from indexes
/// and sort nothing; grouped by artist, each way of the artists with each
/// way of the albums under them.
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
    let mut grouped_sorts = sorts.clone();
    for (artist, artists) in both_ways(AlbumSortField::Artist) {
        for field in [AlbumSortField::Title, AlbumSortField::Year, AlbumSortField::DateAdded] {
            for (field, direction) in both_ways(field) {
                grouped_sorts.push(vec![
                    AlbumSortCriterion { field: artist, direction: artists },
                    AlbumSortCriterion { field, direction },
                ]);
            }
        }
    }
    for (group_by_artist, sorts) in [(false, &sorts), (true, &grouped_sorts)] {
        for sort in sorts {
            let (_, order) = album_browse_order(sort, group_by_artist);
            let key = key_at(
                &db,
                &order,
                move |sql, order, seek, limit| {
                    crate::album::album_rows_on(sql, group_by_artist, order, seek, limit)
                },
                150,
            )
            .await;
            assert_seeks_search(
                &db,
                &format!("albums {sort:?} grouped {group_by_artist}"),
                &order,
                key,
                |seek| super::album_window_statement(group_by_artist, &order, seek),
                Vec::new(),
            )
            .await;
        }
    }
}

/// Three windows of the grid, each its own live query: the first row, the
/// second and third, and the fourth on. The first two hold as many rows as
/// their limits, and the last reaches the grid's end.
async fn three_windows(
    db: &Store,
    sort: &[AlbumSortCriterion],
    group_by_artist: bool,
) -> [ListWindow; 3] {
    let second = key_of_row(db, sort, group_by_artist, 1).await;
    let fourth = key_of_row(db, sort, group_by_artist, 3).await;
    [top(1), from_key(second, 2), from_key(fourth, 50)]
}

/// Windows of the grid, each its own live query, and the grid's count its
/// own. A write that adds a row at or before a full window's last row, and
/// after its start, reruns that window and the count. In the flat grid it
/// reruns no other window, and a row after a full window's last row reruns
/// no window. The grid grouped by artist sorts by the artist's sort key and
/// then the album's values, columns of two tables, so coven can't bound a
/// full window's rerun at its last row there, and other windows rerun too.
#[tokio::test]
async fn a_write_reruns_only_the_count_and_the_window_holding_its_row() {
    use bae_util::test_channels::HANG_GUARD;
    for group_by_artist in [false, true] {
        let (db, _dir) = grouped_library().await;
        let views = LibraryViews::new(db.clone());
        let sort = [AlbumSortCriterion {
            field: AlbumSortField::Title,
            direction: SortDirection::Ascending,
        }];
        // Flat: A, then B and C, then D and E. Grouped: E, then B and D,
        // then A and C.
        let windows = three_windows(&db, &sort, group_by_artist).await;
        let (first_artist, gap) = if group_by_artist {
            (ARTIST_C, ("Album F", ARTIST_C))
        } else {
            (ARTIST_A, ("Album AA", ARTIST_A))
        };
        // Each album, by title and artist, with the windows that hold it.
        let writes = [
            // B's, between Album B and Album D grouped or flat: in the
            // second window.
            ("Album BB", ARTIST_B, [false, true, false]),
            // First in the grid: in the first window.
            ("Album 0", first_artist, [true, false, false]),
            // After the first window's last row, Album 0, and before the
            // second window's first row, Album B: in no window.
            (gap.0, gap.1, [false, false, false]),
            // Last in the grid: in the last window, which reaches the end.
            ("Album Z", ARTIST_A, [false, false, true]),
        ];
        for (at, (title, artist, holds)) in writes.into_iter().enumerate() {
            // Each write meets windows that have read the grid as it is.
            let mut live: Vec<_> = windows
                .iter()
                .map(|window| views.subscribe_album_window(&sort, group_by_artist, window.clone()))
                .collect();
            let mut count = views.subscribe_album_count(group_by_artist);
            for query in &mut live {
                tokio::time::timeout(HANG_GUARD, query.next_inline())
                    .await
                    .unwrap()
                    .unwrap()
                    .result
                    .unwrap();
            }
            count.next_inline().await.expect("the query stays open").unwrap();
            let id = grouped_album(5 + at as u32);
            RecordsFixture::new()
                .album(&id, title, None, &[artist])
                .release(&id, &grouped_release(5 + at as u32))
                .write(&test_records(&db))
                .await;
            let marked: Vec<bool> = live.iter().map(|query| query.is_marked_for_rerun()).collect();
            if group_by_artist {
                assert!(
                    marked.iter().zip(&holds).all(|(marked, holds)| *marked || !*holds),
                    "{title}: reran {marked:?}, but the album is in {holds:?}"
                );
            } else {
                assert_eq!(marked, holds, "{title}");
            }
            assert!(count.is_marked_for_rerun(), "{title}");
        }
        let second = window_under(&db, &sort, group_by_artist, windows[1].clone()).await;
        assert!(second.rows.iter().any(|(row, _key)| row.album.title == "Album BB"));
    }
}

/// Whatever the grid's order, flat or grouped by artist, a write adding an
/// album reruns the window that then holds it. Where every term of the
/// order is a column of the grid's listing table, the write reruns no other
/// window: not a window that starts after it, and not a full window whose
/// last row comes before it. A write that changes
/// what a row shows, its cover, reruns only the windows showing that row.
#[tokio::test]
async fn every_album_sort_reruns_only_the_window_holding_the_written_row() {
    use bae_util::test_channels::HANG_GUARD;
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
    let mut wrong = Vec::new();
    for group_by_artist in [false, true] {
        for sort in &sorts {
            let (db, _dir) = grouped_library().await;
            let views = LibraryViews::new(db.clone());
            let windows = three_windows(&db, sort, group_by_artist).await;
            let mut live: Vec<_> = windows
                .iter()
                .map(|window| views.subscribe_album_window(sort, group_by_artist, window.clone()))
                .collect();
            for query in &mut live {
                tokio::time::timeout(HANG_GUARD, query.next_inline())
                    .await
                    .unwrap()
                    .unwrap()
                    .result
                    .unwrap();
            }
            let id = grouped_album(5);
            RecordsFixture::new()
                .album(&id, "Album M", None, &[ARTIST_B])
                .release(&id, &grouped_release(5))
                .write(&test_records(&db))
                .await;
            let mut holds = Vec::new();
            for window in &windows {
                let rows = window_under(&db, sort, group_by_artist, window.clone()).await.rows;
                holds.push(rows.iter().any(|(row, _key)| row.album.id == id));
            }
            // The album lands in at most one window: between the first
            // window's one row and the second's start it lands in none.
            assert!(holds.iter().filter(|holds| **holds).count() <= 1, "{sort:?}: {holds:?}");
            let marked: Vec<bool> = live.iter().map(|query| query.is_marked_for_rerun()).collect();
            // A year sorts by whether it is NULL first, an expression, and
            // the grouped grid by columns of two tables: coven bounds no
            // full window's rerun at its last row there, so each window from
            // the album's row up reruns.
            let year = sort.iter().any(|criterion| criterion.field == AlbumSortField::Year);
            let bounded = !group_by_artist && !year;
            let reran_right = if bounded {
                marked == holds
            } else {
                marked.iter().zip(&holds).all(|(marked, holds)| *marked || !*holds)
            };
            if !reran_right {
                wrong.push(format!(
                    "{sort:?} grouped {group_by_artist}: reran {marked:?}, but the album is in \
                     {holds:?}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "windows rerun by a write outside them:\n{}", wrong.join("\n"));
}

/// A write that adds a cover to an album, through the records domain, reruns
/// the window that shows the album, flat and grouped by artist, and in the
/// flat grid no other window.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cover_written_for_an_album_reruns_only_the_window_showing_it() {
    use bae_util::test_channels::HANG_GUARD;
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    for group_by_artist in [false, true] {
        let (db, _dir) = grouped_library().await;
        let views = LibraryViews::new(db.clone());
        let windows = three_windows(&db, &sort, group_by_artist).await;
        let mut live: Vec<_> = windows
            .iter()
            .map(|window| views.subscribe_album_window(&sort, group_by_artist, window.clone()))
            .collect();
        for query in &mut live {
            tokio::time::timeout(HANG_GUARD, query.next_inline())
                .await
                .unwrap()
                .unwrap()
                .result
                .unwrap();
        }
        let album = grouped_album(2);
        let mut holds = Vec::new();
        for window in &windows {
            let rows = window_under(&db, &sort, group_by_artist, window.clone()).await.rows;
            holds.push(rows.iter().any(|(row, _key)| row.album.id == album));
        }
        assert_eq!(holds.iter().filter(|holds| **holds).count(), 1, "{holds:?}");

        bae_records::test_fixtures::write_cover(&test_records(&db), &grouped_release(2), "a cover")
            .await;

        let marked: Vec<bool> = live.iter().map(|query| query.is_marked_for_rerun()).collect();
        if group_by_artist {
            // The grouped grid's order spans two tables, so coven can't
            // bound the first window's rerun at its last row.
            assert!(marked.iter().zip(&holds).all(|(marked, holds)| *marked || !*holds));
        } else {
            assert_eq!(marked, holds);
        }
    }
}
