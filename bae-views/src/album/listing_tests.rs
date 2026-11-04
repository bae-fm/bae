//! The album grid's rows: how they sort, flat or grouped by artist, how a
//! release leaving or an artist merging moves them, and every sort's windows
//! read from an index.

use crate::album::album_browse_order;
use crate::tests::{assert_seeks_search_sorting, both_ways, key_at, library};
use crate::*;
use bae_db::test_fixtures::album_listing::*;
use bae_db::test_fixtures::{exec, exec_batch, temp_db, ARTIST_A, ARTIST_B, ARTIST_C};
use bae_db::{LibraryPageWindow, LibraryPageWindows};
use std::collections::BTreeSet;

/// The grid's album ids in `field` order, `direction`.
async fn album_ids(db: &Database, field: AlbumSortField, direction: SortDirection) -> Vec<String> {
    crate::LibraryViews::new(db.clone())
        .get_album_page(&[AlbumSortCriterion { field, direction }], 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.id)
        .collect()
}

/// The artist list: each artist's id and album count, by name.
async fn artists(db: &Database) -> Vec<(String, i64)> {
    crate::LibraryViews::new(db.clone())
        .get_artist_page(&[], 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|artist| (artist.raw.artist.id, artist.raw.album_count))
        .collect()
}

/// The grid grouped by artist: each section's artist, its first row and its
/// length, and the album ids of the rows in order.
async fn grouped(db: &Database) -> (Vec<(String, u64, u64)>, Vec<String>) {
    let windows = [LibraryPageWindow { offset: 0, limit: 50 }].into_iter().collect::<BTreeSet<_>>();
    let mut live = crate::LibraryViews::new(db.clone()).subscribe_album_browse(&[], true, windows);
    let shown = live.next().await.expect("the query stays open").result.unwrap();
    (
        shown
            .sections
            .iter()
            .map(|section| (section.id.clone(), section.window.offset, section.window.limit))
            .collect(),
        shown.windows[0].rows.iter().map(|row| row.album.id.clone()).collect(),
    )
}

#[tokio::test]
async fn album_grid_sorts_by_credits_as_their_artists_change() {
    let (db, _tmp) = listed_db().await;
    let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
    // Alpha's album, then Bravo's, then Bravo and Alpha's: the credit list
    // compares in order, so a longer list sorts after its first name alone.
    assert_eq!(
        album_ids(&db, AlbumSortField::Artist, SortDirection::Ascending).await,
        ids(&[ALBUM_TWO, ALBUM_ONE, ALBUM_THREE])
    );

    exec_batch(
        &db,
        "UPDATE artists SET sort_name = 'Zulu Sort Name' WHERE id = '4309b580-a6bf-455e-92ea-ec58ee7129e7';",
    )
    .await;
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
    let (db, _tmp) = listed_db().await;
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
    let (db, _tmp) = listed_db().await;
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 3);
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 2)]);

    exec_batch(&db, "DELETE FROM releases WHERE id = '5a905d5d-d400-42f3-a9a8-b858f18dcbc3';")
        .await;
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 2);
    assert!(!album_ids(&db, AlbumSortField::Title, SortDirection::Ascending)
        .await
        .contains(&ALBUM_ONE.to_string()));
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 1)]);
    assert_eq!(crate::LibraryViews::new(db.clone()).get_artist_count().await.unwrap(), 2);

    // A release arriving under the album lists it again.
    exec_batch(
        &db,
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
            ('24b929ff-1f01-4621-9891-81abea7fbe87', '07d22f7d-3980-4c81-9d27-8fcd50ee3464', '2026-01-01T00:00:00Z', 0);",
    )
    .await;
    assert_eq!(crate::LibraryViews::new(db.clone()).get_album_count().await.unwrap(), 3);
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 2)]);
}

#[tokio::test]
async fn a_merge_moves_the_absorbed_artists_albums_under_the_survivor() {
    let (db, _tmp) = listed_db().await;
    // Newest first under each artist; Alpha's section before Bravo's.
    assert_eq!(
        grouped(&db).await,
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

    exec_batch(
        &db,
        "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES
            ('4309b580-a6bf-455e-92ea-ec58ee7129e7', 'f392a38b-f264-41a0-a225-c6935b8a8262', '2026-01-01T00:00:00Z');",
    )
    .await;
    assert_eq!(
        grouped(&db).await,
        (
            vec![(BRAVO.to_string(), 0, 3)],
            vec![ALBUM_THREE.to_string(), ALBUM_TWO.to_string(), ALBUM_ONE.to_string(),]
        ),
        "the third album credits the survivor twice and lists once"
    );
    assert_eq!(artists(&db).await, vec![(BRAVO.to_string(), 3)]);
    let survivor = crate::LibraryViews::new(db.clone())
        .find_artist_detail(BRAVO)
        .await
        .unwrap()
        .expect("listed");
    assert_eq!(survivor.albums.len(), 3);
    assert!(crate::LibraryViews::new(db.clone())
        .find_artist_detail(ALPHA)
        .await
        .unwrap()
        .is_none());
}

async fn grouped_library() -> (Database, tempfile::TempDir) {
    let (db, dir) = temp_db(bae_db::test_fixtures::TEST_SCHEMA).await;
    for (id, name, sort_name) in [
        (ARTIST_A, "Artist Name A", "Same Sort Name"),
        (ARTIST_B, "Artist Name B", "Same Sort Name"),
        (ARTIST_C, "Artist Name C", "Last Artist"),
    ] {
        exec(
            &db,
            "INSERT INTO artists (id, name, name_key, sort_name, created_at) VALUES (?, ?, ?, ?, '2026-01-01T00:00:00Z')",
            &[id, name, name, sort_name],
        )
        .await;
    }
    for (index, (artist, title)) in [
        (ARTIST_A, "Album A"),
        (ARTIST_B, "Album B"),
        (ARTIST_A, "Album C"),
        (ARTIST_B, "Album D"),
        (ARTIST_C, "Album E"),
    ]
    .into_iter()
    .enumerate()
    {
        let album = format!("00000000-0000-4000-8000-{index:012}");
        let credit = format!("20000000-0000-4000-8000-{index:012}");
        let release = format!("10000000-0000-4000-8000-{index:012}");
        exec_batch(
            &db,
            &format!(
            "INSERT INTO albums (id, title, is_compilation, created_at) VALUES ('{album}', '{title}', 0, '2026-01-01T00:00:00Z'); \
             INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('{credit}', '{album}', '{artist}', 0, '2026-01-01T00:00:00Z'); \
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES ('{release}', '{album}', '2026-01-01T00:00:00Z', 0);"
        ),
        )
        .await;
    }
    (db, dir)
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
        assert_eq!(value.sections.len(), 3);
        let page = LibraryViews::new(db.clone()).get_album_page(&sort, 0, 10).await.unwrap();
        let mut offset = 0;
        for section in &value.sections {
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

#[tokio::test]
async fn artist_sections_follow_renames_and_disappearing_groups() {
    let (db, _dir) = grouped_library().await;
    let sort =
        [AlbumSortCriterion { field: AlbumSortField::Artist, direction: SortDirection::Ascending }];
    let mut browse =
        LibraryViews::new(db.clone()).subscribe_album_browse(&sort, true, BTreeSet::new());
    let initial = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.sections[0].id, ARTIST_C);
    exec(
        &db,
        "UPDATE artists SET name = 'Renamed Artist', sort_name = 'Z' WHERE id = ?",
        &[ARTIST_C],
    )
    .await;
    let renamed = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(renamed.sections[2].title, "Renamed Artist");
    assert_eq!(renamed.sections[2].window.offset, 4);
    exec(
        &db,
        "DELETE FROM releases WHERE album_id IN (SELECT album_id FROM album_artists WHERE artist_id = ?)",
        &[ARTIST_C],
    )
    .await;
    let removed = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(removed.total_count, 4);
    assert_eq!(removed.sections.len(), 2);
    assert!(!removed.sections.iter().any(|section| section.id == ARTIST_C));
    let mut flat =
        LibraryViews::new(db.clone()).subscribe_album_browse(&sort, false, BTreeSet::new());
    assert!(flat.next().await.expect("the query stays open").result.unwrap().sections.is_empty());
}

#[tokio::test]
async fn collaborations_appear_under_each_credit_but_count_once() {
    let (db, _dir) = grouped_library().await;
    let album_id = "00000000-0000-4000-8000-000000000000";
    exec(
        &db,
        "INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('extra', ?, ?, 1, '2026-01-01T00:00:00Z')",
        &[album_id, ARTIST_B],
    )
    .await;
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
    for section in &value.sections {
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
    assert!(flat.sections.is_empty());
    assert_eq!(flat.windows[0].rows.iter().filter(|row| row.album.id == album_id).count(), 1);
    exec(&db, "DELETE FROM album_artists WHERE album_id = ? AND position = 1", &[album_id]).await;
    let removed = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(removed.row_count, 5);
    assert_eq!(removed.total_count, 5);
}

#[tokio::test]
async fn merged_and_repeated_credits_make_one_appearance_per_artist() {
    let (db, _dir) = grouped_library().await;
    let album_id = "00000000-0000-4000-8000-000000000000";
    for (id, artist, position) in
        [("primary-again", ARTIST_A, "1"), ("other-credit", ARTIST_B, "2")]
    {
        exec(&db,
            "INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES (?, ?, ?, ?, '2026-01-01T00:00:00Z')",
            &[id, album_id, artist, position]).await;
    }
    let mut browse = LibraryViews::new(db.clone()).subscribe_album_browse(
        &[],
        true,
        [LibraryPageWindow { offset: 0, limit: 20 }].into(),
    );
    let initial = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(initial.row_count, 6);
    exec(
        &db,
        "INSERT INTO artist_merges (id, into_artist_id, created_at) VALUES (?, ?, '2026-01-01T00:00:00Z')",
        &[ARTIST_B, ARTIST_A],
    )
    .await;
    let merged = browse.next().await.expect("the query stays open").result.unwrap();
    assert_eq!(merged.total_count, 5);
    assert_eq!(merged.row_count, 5);
    assert_eq!(merged.sections.len(), 2);
    assert!(merged.sections.iter().all(|section| section.id != ARTIST_B));
    assert_eq!(merged.windows[0].rows.iter().filter(|row| row.album.id == album_id).count(), 1);
}

/// The grid's selection comes back in the grid's order, whatever order its
/// albums were picked in: by title one way, by artist and then title grouped,
/// each album once, and an album the library no longer holds left out.
#[tokio::test]
async fn a_selection_is_read_in_the_grids_order() {
    let (db, _dir) = grouped_library().await;
    let album = |index: u32| format!("00000000-0000-4000-8000-{index:012}");
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
    let album_id = "00000000-0000-4000-8000-000000000000";
    exec(
        &db,
        "INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES ('extra', ?, ?, 1, '2026-01-01T00:00:00Z')",
        &[album_id, ARTIST_B],
    )
    .await;
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
    for section in &grouped.sections {
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
    let mut browse =
        LibraryViews::new(db.clone()).subscribe_album_browse(&[], true, BTreeSet::new());
    let initial = browse.next().await.expect("the query stays open").result.unwrap();
    assert!(initial.sections.iter().all(|section| section.image.is_none()));
    let version =
        bae_db::test_fixtures::live_query::write_artist_image(&db, ARTIST_B, "artist image bytes")
            .await;
    let pictured = browse.next().await.expect("the query stays open").result.unwrap();
    let images = pictured
        .sections
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
    for group_by_artist in [false, true] {
        for sort in &sorts {
            let (relation, order) = album_browse_order(sort, group_by_artist);
            let read = relation.clone();
            let key = key_at(
                &db,
                &order,
                move |sql, order, seek, limit, offset| {
                    crate::album::album_rows_on(
                        sql,
                        &read,
                        group_by_artist,
                        order,
                        seek,
                        limit,
                        offset,
                    )
                },
                150,
            )
            .await;
            // Grouped by artist, a seek holding the artist's sort key and id
            // reads that artist's albums alone.
            let sorted_within = if group_by_artist { 2 } else { usize::MAX };
            assert_seeks_search_sorting(
                &db,
                &format!("albums {sort:?} grouped {group_by_artist}"),
                &order,
                key,
                |seek| super::album_window_statement(&relation, group_by_artist, &order, seek),
                Vec::new(),
                sorted_within,
            )
            .await;
        }
    }
}

/// The album list's ids at `window`, as a read by offset finds them.
async fn albums_at(db: &Database, window: &LibraryPageWindow) -> Vec<String> {
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
    query: &mut coven::ReconfigurableLiveQuery<LibraryPageWindows, AlbumBrowseProjection>,
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
    db.call(|conn| {
        conn.execute_batch(
            "INSERT INTO albums (id, title, created_at) VALUES \
                 ('f0000000-0000-4000-8000-000000000001', 'Album Title', '2099-01-01T00:00:00Z');
             INSERT INTO releases (id, album_id, album_order, created_at) VALUES \
                 ('f0000000-0000-4000-8000-000000000002', 'f0000000-0000-4000-8000-000000000001', \
                  0, '2099-01-01T00:00:00Z');",
        )
        .map_err(DbError::from)
    })
    .await
    .unwrap();
    for offset in [150, 50] {
        assert_eq!(
            read_albums(&mut query, LibraryPageWindows::from([window(offset)])).await,
            vec![albums_at(&db, &window(offset)).await],
            "at {offset} after the write"
        );
    }
}
