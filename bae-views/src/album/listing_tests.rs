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
use bae_records::{DbArtist, DbRelease};
use bae_store::test_fixtures::temp_db;
use bae_store::{LibraryPageWindow, LibraryPageWindows};
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

/// The grid's artist sections under `sort`, as its first delivery carries
/// them.
async fn sections_under(
    db: &Store,
    sort: &[AlbumSortCriterion],
) -> Vec<crate::LibraryBrowseSection> {
    let mut live =
        crate::LibraryViews::new(db.clone()).subscribe_album_browse(sort, true, BTreeSet::new());
    live.next()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .sections
        .expect("the first delivery carries the sections")
}

/// The grid grouped by artist: each section's artist, its first row and its
/// length, and the album ids of the rows in order.
async fn grouped(db: &Store) -> (Vec<(String, u64, u64)>, Vec<String>) {
    let windows = [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect::<BTreeSet<_>>();
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], true, windows);
    let shown = live.next().await.expect("the query stays open").result.unwrap();
    (
        sections_under(db, &[])
            .await
            .iter()
            .map(|section| (section.id.clone(), section.window.offset, section.window.limit))
            .collect(),
        shown.windows[0].rows.iter().map(|row| row.album.id.clone()).collect(),
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

    // A release arriving under the album lists it again.
    test_records(&db)
        .insert_release(&DbRelease::new_test(ALBUM_ONE, "24b929ff-1f01-4621-9891-81abea7fbe87"))
        .await
        .unwrap();
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 3);
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 2)]);
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
/// order, and the version of the album-artist links the order reads.
async fn grouped_by_title(db: &Store) -> (Vec<String>, Vec<i64>) {
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Title, direction: SortDirection::Ascending }];
    let shown = LibraryViews::new(db.clone())
        .subscribe_album_browse(&sort, true, [LibraryPageWindow { offset: 0, limit: 20 }].into())
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    let version = db
        .read(|sql| {
            Ok(bae_store::list_windows::list_versions_on(
                &sql,
                crate::versions::TABLE,
                &["album_artist_link"],
            )?)
        })
        .await
        .unwrap();
    (shown.windows[0].rows.iter().map(|row| row.album.title.clone()).collect(), version)
}

/// Renaming an album moves it among its artist's albums in the grid grouped
/// by artist, and moves the version of the links that order reads, so no
/// later read starts a window from where a row sat before the rename.
#[tokio::test]
async fn a_renamed_album_moves_among_its_artists_albums() {
    let (db, _dir) = grouped_library().await;
    let (titles, version) = grouped_by_title(&db).await;
    // The third artist sorts first, then B and A, which share a sort name,
    // by id.
    assert_eq!(titles, ["Album E", "Album B", "Album D", "Album A", "Album C"]);

    test_records(&db).rename_album_for_test(&grouped_album(0), "Album Z").await.unwrap();
    let (renamed, renamed_version) = grouped_by_title(&db).await;
    assert_eq!(renamed, ["Album E", "Album B", "Album D", "Album C", "Album Z"]);
    assert_ne!(renamed_version, version);
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
        let mut browse = LibraryViews::new(db.clone()).subscribe_album_browse(
            &sort,
            true,
            [LibraryPageWindow { offset: 2, limit: 1 }].into(),
        );
        let value = browse.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(value.windows[0].rows.len(), 1);
        assert_eq!(value.total_count, 5);
        let sections = sections_under(&db, &sort).await;
        assert_eq!(sections.len(), 3);
        assert_eq!(value.row_count, 5);
        let page = LibraryViews::new(db.clone()).get_album_page(&sort, 0, 10).await.unwrap();
        let mut offset = 0;
        for section in &sections {
            assert_eq!(section.window.offset, offset);
            let albums = &page[offset as usize..(offset + section.window.limit) as usize];
            assert!(albums.iter().all(|album| album.artist_names == section.title));
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
            offset += section.window.limit;
        }
        assert_eq!(offset, 5);
        assert_eq!(value.windows[0].rows[0].album.id, page[2].id);
    }
}

/// Moving the grid to other windows delivers its rows and no sections, and
/// runs the sections query zero times; a write that changes an artist
/// delivers the new sections with the rows they fit, in one value.
#[tokio::test]
async fn moving_the_grids_windows_runs_the_artist_sections_query_zero_times() {
    let (db, _dir) = grouped_library().await;
    let mut browse =
        LibraryViews::new(db.clone()).subscribe_album_browse(&[], true, BTreeSet::new());
    let requests = browse.requests();
    let first = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(first.sections.expect("the first delivery carries them").len(), 3);

    for offset in [2, 0, 4] {
        requests.set([LibraryPageWindow { offset, limit: 1 }].into()).unwrap();
        let moved = browse.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(moved.windows[0].window.offset, offset);
        assert_eq!(moved.sections, None, "a window change delivers no artist");
    }

    delete_release(&test_records(&db), &grouped_release(4)).await;
    let changed = tokio::time::timeout(HANG_GUARD, browse.next())
        .await
        .expect("deleting an artist's last release wakes the grid")
        .expect("the query stays open")
        .result
        .unwrap();
    let sections = changed.sections.expect("the sections changed, so this delivery carries them");
    assert_eq!(sections.len(), 2);
    assert_eq!(changed.total_count, 4);
    assert_eq!(changed.row_count, 4);
    assert_eq!(
        sections.iter().map(|section| section.window.limit).sum::<u64>(),
        changed.row_count,
        "the sections fit the rows delivered with them"
    );
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
        let mut browse =
            LibraryViews::new(db.clone()).subscribe_album_browse(&sort, true, BTreeSet::new());
        let initial = browse.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(initial.sections.expect("the first delivery carries them")[0].id, third);

        meet_musicbrainz_artist(
            &test_records(devices.b()),
            RENAMED_MUSICBRAINZ_ARTIST,
            "Renamed Artist",
            Some("Z"),
        )
        .await;
        devices.resume().await;
        let renamed = loop {
            let shown = tokio::time::timeout(HANG_GUARD, browse.next())
                .await
                .expect("the other device's name reaches the sections")
                .expect("the query stays open")
                .result
                .unwrap();
            if let Some(sections) = shown.sections {
                if sections.iter().any(|section| section.title == "Renamed Artist") {
                    break sections;
                }
            }
        };
        assert_eq!(renamed[2].title, "Renamed Artist");
        assert_eq!(renamed[2].window.offset, 4);

        delete_release(&test_records(db), &grouped_release(4)).await;
        let removed = browse
            .next()
            .await
            .expect("the query stays open")
            .result
            .unwrap()
            .sections
            .expect("removing an artist's last album delivers the sections");
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
    let mut browse = LibraryViews::new(db.clone()).subscribe_album_browse(
        &sort,
        true,
        [LibraryPageWindow { offset: 0, limit: 20 }].into(),
    );
    let value = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(value.total_count, 5);
    assert_eq!(value.row_count, 6);
    let rows = &value.windows[0].rows;
    let mut appearances = Vec::new();
    let sections = sections_under(&db, &sort).await;
    for section in &sections {
        let start = section.window.offset as usize;
        let end = start + section.window.limit as usize;
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
    let mut flat = LibraryViews::new(db.clone()).subscribe_album_browse(
        &sort,
        false,
        [LibraryPageWindow { offset: 0, limit: 20 }].into(),
    );
    let flat = flat.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(flat.row_count, 5);
    assert_eq!(flat.windows[0].rows.iter().filter(|row| row.album.id == album_id).count(), 1);
    records.credit_album_for_test(album_id, &[ARTIST_A]).await.unwrap();
    let removed = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(removed.row_count, 5);
    assert_eq!(removed.total_count, 5);
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
        let mut browse = LibraryViews::new(db.clone()).subscribe_album_browse(
            &[],
            true,
            [LibraryPageWindow { offset: 0, limit: 20 }].into(),
        );
        let initial = browse.next().await.expect("the query stays open").result.unwrap();
        assert_eq!(initial.row_count, 6);

        merge_apart(devices, ("Artist Name A", "Artist Name B"), "discogs-grouped").await;
        let merged = loop {
            let shown = tokio::time::timeout(HANG_GUARD, browse.next())
                .await
                .expect("the merge reaches the browse")
                .expect("the query stays open")
                .result
                .unwrap();
            if shown.row_count == 5 {
                break shown;
            }
        };
        assert_eq!(merged.total_count, 5);
        let sections = sections_under(db, &[]).await;
        assert_eq!(sections.len(), 2);
        assert!(sections.iter().all(|section| section.id != ARTIST_A));
        assert_eq!(merged.windows[0].rows.iter().filter(|row| row.album.id == album_id).count(), 1);
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
    let windows = [LibraryPageWindow { offset: 0, limit: 20 }];
    let grouped = LibraryViews::new(db.clone())
        .subscribe_album_browse(&[], true, windows.clone().into())
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    let rows = &grouped.windows[0].rows;
    let mut credited_with = BTreeSet::new();
    let sections = sections_under(&db, &[]).await;
    for section in &sections {
        let start = section.window.offset as usize;
        let end = start + section.window.limit as usize;
        for row in &rows[start..end] {
            credited_with.insert((
                section.id.as_str(),
                row.album.title.as_str(),
                row.credited_with.as_deref(),
            ));
        }
    }
    assert_eq!(
        credited_with,
        [
            (ARTIST_A, "Album A", Some("Artist Name B")),
            (ARTIST_A, "Album C", None),
            (ARTIST_B, "Album A", Some("Artist Name A")),
            (ARTIST_B, "Album B", None),
            (ARTIST_B, "Album D", None),
            (ARTIST_C, "Album E", None),
        ]
        .into()
    );
    let flat = LibraryViews::new(db.clone())
        .subscribe_album_browse(&[], false, windows.into())
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap();
    assert!(flat.windows[0].rows.iter().all(|row| row.credited_with.is_none()));
}

/// An artist's group carries the artist's picture, and follows it as it
/// changes; an artist without one carries none.
#[tokio::test]
async fn an_artist_section_carries_the_artists_picture() {
    let (db, _dir) = grouped_library().await;
    let records = bae_records::test_fixtures::test_records(&db);
    let mut browse =
        LibraryViews::new(db.clone()).subscribe_album_browse(&[], true, BTreeSet::new());
    let initial = browse.next().await.expect("the query stays open").result.unwrap();
    let initial = initial.sections.expect("the first delivery carries them");
    assert!(initial.iter().all(|section| section.image.is_none()));
    let version = bae_records::test_fixtures::live_query::write_artist_image(
        &records,
        ARTIST_B,
        "artist image bytes",
    )
    .await;
    let pictured = browse
        .next()
        .await
        .expect("the query stays open")
        .result
        .unwrap()
        .sections
        .expect("a new picture delivers the sections");
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
                move |sql, order, seek, limit, offset| {
                    crate::album::album_rows_on(sql, group_by_artist, order, seek, limit, offset)
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

/// The album list's ids at `window`, as a read by offset finds them.
async fn albums_at(db: &Store, window: &LibraryPageWindow) -> Vec<String> {
    crate::LibraryViews::new(db.clone())
        .get_album_page(&[], window.offset, window.limit)
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.id)
        .collect()
}

/// The next value of the album list's live query once `windows` are asked
/// for, as each window's ids.
async fn read_albums(
    query: &mut coven::ReconfigurableLiveQuery<
        LibraryPageWindows,
        AlbumBrowseProjection,
        bae_store::DbError,
    >,
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
                .map(|window| window.rows.into_iter().map(|row| row.album.id).collect())
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
    let mut query = crate::LibraryViews::new(db.clone()).subscribe_album_browse(
        &[],
        false,
        LibraryPageWindows::new(),
    );
    query.next().await.unwrap().result.unwrap();
    for offset in [100, 150, 200, 150, 100] {
        assert_eq!(
            read_albums(&mut query, LibraryPageWindows::from([window(offset)])).await,
            vec![albums_at(&db, &window(offset)).await],
            "at {offset}"
        );
    }
    // The newest album sorts first in the default order.
    RecordsFixture::new()
        .at("2099-01-01T00:00:00Z")
        .album("f0000000-0000-4000-8000-000000000001", "Album Title", None, &[])
        .release("f0000000-0000-4000-8000-000000000001", "f0000000-0000-4000-8000-000000000002")
        .write(&test_records(&db))
        .await;
    for offset in [150, 50] {
        assert_eq!(
            read_albums(&mut query, LibraryPageWindows::from([window(offset)])).await,
            vec![albums_at(&db, &window(offset)).await],
            "at {offset} after the write"
        );
    }
}
