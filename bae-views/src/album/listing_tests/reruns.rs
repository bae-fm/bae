//! Which writes rerun the album grid's windows, each its own live query, and
//! its count.

use super::*;

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

/// Windows of the grid, flat or grouped by artist, each its own live query,
/// and the grid's count its own. A write that adds a row at or before a full
/// window's last row, and after its start, reruns that window and the count.
/// A window reruns for no other row, unless the row ties on the grid's first
/// sort column with the window's start or its last row.
#[tokio::test]
async fn a_write_reruns_only_the_count_and_the_window_holding_its_row() {
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
            // second window's first row, Album B: in no window. Grouped,
            // the artists come first, and it ties with Album 0 on its
            // artist, so it reruns the first window.
            (gap.0, gap.1, [group_by_artist, false, false]),
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
            assert_eq!(marked, holds, "{title} grouped {group_by_artist}");
            assert!(count.is_marked_for_rerun(), "{title}");
        }
        let second = window_under(&db, &sort, group_by_artist, windows[1].clone()).await;
        assert!(second.rows.iter().any(|(row, _key)| row.album.title == "Album BB"));
    }
}

/// Whatever the grid's order, flat or grouped by artist, a write adding an
/// album reruns the window that then holds it, and otherwise only a window
/// whose start or full window's last row ties with it on the grid's first
/// sort column: no window that starts after it on that column, and no full
/// window whose last row comes before it there.
#[tokio::test]
async fn every_album_sort_reruns_only_the_window_holding_the_written_row() {
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
            // Each window's last row's first sort value, when it is full.
            let mut last_values = Vec::new();
            for (query, window) in live.iter_mut().zip(&windows) {
                let shown = tokio::time::timeout(HANG_GUARD, query.next_inline())
                    .await
                    .unwrap()
                    .unwrap()
                    .result
                    .unwrap();
                let full = shown.rows.len() as u64 == window.limit;
                last_values.push(
                    full.then(|| shown.rows.last().map(|(_row, key)| key.values()[0].clone()))
                        .flatten(),
                );
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
            let all = window_under(&db, sort, group_by_artist, top(100)).await;
            // The album's first sort value under each artist that lists it.
            let written: Vec<_> = all
                .rows
                .iter()
                .filter(|(row, _key)| row.album.id == id)
                .map(|(_row, key)| key.values()[0].clone())
                .collect();
            let expected: Vec<bool> = windows
                .iter()
                .zip(&last_values)
                .zip(&holds)
                .map(|((window, last), holds)| {
                    let start = window.from.as_ref().map(|key| &key.values()[0]);
                    *holds
                        || written
                            .iter()
                            .any(|value| start == Some(value) || last.as_ref() == Some(value))
                })
                .collect();
            let marked: Vec<bool> = live.iter().map(|query| query.is_marked_for_rerun()).collect();
            if marked != expected {
                wrong.push(format!(
                    "{sort:?} grouped {group_by_artist}: reran {marked:?}, but the album is in \
                     {holds:?}, and with ties on the first sort column {expected:?}"
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "windows rerun by a write outside them:\n{}", wrong.join("\n"));
}

/// A write that adds a cover to an album, through the records domain, reruns
/// the window that shows the album, flat and grouped by artist, and no other
/// window.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cover_written_for_an_album_reruns_only_the_window_showing_it() {
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
        assert_eq!(marked, holds, "grouped {group_by_artist}");
    }
}
