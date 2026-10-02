use super::super::*;
use super::{exec, exec_batch, temp_db, ARTIST_A, ARTIST_B, ARTIST_C};
use crate::library::LibraryPageWindow;

async fn grouped_library() -> (Database, tempfile::TempDir) {
    let (db, dir) = temp_db().await;
    for (id, name, sort_name) in [
        (ARTIST_A, "Artist Name A", "Same Sort Name"),
        (ARTIST_B, "Artist Name B", "Same Sort Name"),
        (ARTIST_C, "Artist Name C", "Last Artist"),
    ] {
        exec(
            &db,
            "INSERT INTO artists (id, name, name_key, sort_name, _updated_at, created_at) \
            VALUES (?, ?, ?, ?, 'seed', '2026-01-01T00:00:00Z')",
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
        let release = format!("10000000-0000-4000-8000-{index:012}");
        exec_batch(
            &db,
            &format!(
            "INSERT INTO albums (id, title, artist_id, is_compilation, _updated_at, created_at) \
             VALUES ('{album}', '{title}', '{artist}', 0, 'seed', '2026-01-01T00:00:00Z'); \
             INSERT INTO releases (id, album_id, remote, _updated_at, created_at) \
             VALUES ('{release}', '{album}', 1, 'seed', '2026-01-01T00:00:00Z');"
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
            AlbumSortCriterion {
                field: AlbumSortField::Artist,
                direction,
            },
            AlbumSortCriterion {
                field: AlbumSortField::Title,
                direction: SortDirection::Descending,
            },
        ];
        let mut browse = db.subscribe_album_browse(
            &sort,
            true,
            [LibraryPageWindow {
                offset: 2,
                limit: 1,
            }]
            .into(),
        );
        let value = browse.next().await.into_result().unwrap();
        assert_eq!(value.windows[0].rows.len(), 1);
        assert_eq!(value.total_count, 5);
        assert_eq!(value.sections.len(), 3);
        let page = db.get_album_page(&sort, 0, 10).await.unwrap();
        let mut offset = 0;
        for section in &value.sections {
            assert_eq!(section.window.offset, offset);
            let albums = &page[offset as usize..(offset + section.window.limit) as usize];
            assert!(albums
                .iter()
                .all(|album| album.artist_names == section.title));
            assert!(albums.windows(2).all(|pair| pair[0].title > pair[1].title));
            for (position, album) in albums.iter().enumerate() {
                assert_eq!(
                    db.get_album_index(&sort, &album.id, true).await.unwrap(),
                    Some(offset + position as u64)
                );
            }
            offset += section.window.limit;
        }
        assert_eq!(offset, 5);
        assert_eq!(value.windows[0].rows[0], page[2]);
    }
}

#[tokio::test]
async fn artist_sections_follow_renames_and_disappearing_groups() {
    let (db, _dir) = grouped_library().await;
    let sort = [AlbumSortCriterion {
        field: AlbumSortField::Artist,
        direction: SortDirection::Ascending,
    }];
    let mut browse = db.subscribe_album_browse(&sort, true, BTreeSet::new());
    let initial = browse.next().await.into_result().unwrap();
    assert_eq!(initial.sections[0].id, ARTIST_C);
    exec(
        &db,
        "UPDATE artists SET name = 'Renamed Artist', sort_name = 'Z' WHERE id = ?",
        &[ARTIST_C],
    )
    .await;
    let renamed = browse.next().await.into_result().unwrap();
    assert_eq!(renamed.sections[2].title, "Renamed Artist");
    assert_eq!(renamed.sections[2].window.offset, 4);
    exec(
        &db,
        "DELETE FROM releases WHERE album_id IN (SELECT id FROM albums WHERE artist_id = ?)",
        &[ARTIST_C],
    )
    .await;
    let removed = browse.next().await.into_result().unwrap();
    assert_eq!(removed.total_count, 4);
    assert_eq!(removed.sections.len(), 2);
    assert!(!removed
        .sections
        .iter()
        .any(|section| section.id == ARTIST_C));
    let mut flat = db.subscribe_album_browse(&sort, false, BTreeSet::new());
    assert!(flat.next().await.into_result().unwrap().sections.is_empty());
}

#[tokio::test]
async fn collaborations_appear_under_each_credit_but_count_once() {
    let (db, _dir) = grouped_library().await;
    let album_id = "00000000-0000-4000-8000-000000000000";
    exec(
        &db,
        "INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
         VALUES ('extra', ?, ?, 1, 'seed', '2026-01-01T00:00:00Z')",
        &[album_id, ARTIST_B],
    )
    .await;
    let sort = [AlbumSortCriterion {
        field: AlbumSortField::Title,
        direction: SortDirection::Descending,
    }];
    let mut browse = db.subscribe_album_browse(
        &sort,
        true,
        [LibraryPageWindow {
            offset: 0,
            limit: 20,
        }]
        .into(),
    );
    let value = browse.next().await.into_result().unwrap();
    assert_eq!(value.total_count, 5);
    assert_eq!(value.row_count, 6);
    let rows = &value.windows[0].rows;
    let mut appearances = Vec::new();
    for section in &value.sections {
        let start = section.window.offset as usize;
        let end = start + section.window.limit as usize;
        let albums = &rows[start..end];
        assert!(albums.windows(2).all(|pair| pair[0].title > pair[1].title));
        if albums.iter().any(|album| album.id == album_id) {
            appearances.push(section.id.as_str());
        }
    }
    assert_eq!(
        appearances.into_iter().collect::<BTreeSet<_>>(),
        [ARTIST_A, ARTIST_B].into()
    );
    assert_eq!(
        db.get_album_index(&sort, album_id, true).await.unwrap(),
        rows.iter()
            .position(|row| row.id == album_id)
            .map(|i| i as u64)
    );
    let mut flat = db.subscribe_album_browse(
        &sort,
        false,
        [LibraryPageWindow {
            offset: 0,
            limit: 20,
        }]
        .into(),
    );
    let flat = flat.next().await.into_result().unwrap();
    assert_eq!(flat.row_count, 5);
    assert!(flat.sections.is_empty());
    assert_eq!(
        flat.windows[0]
            .rows
            .iter()
            .filter(|row| row.id == album_id)
            .count(),
        1
    );
    exec(
        &db,
        "DELETE FROM album_artists WHERE album_id = ?",
        &[album_id],
    )
    .await;
    let removed = browse.next().await.into_result().unwrap();
    assert_eq!(removed.row_count, 5);
    assert_eq!(removed.total_count, 5);
}

#[tokio::test]
async fn merged_and_repeated_credits_make_one_appearance_per_artist() {
    let (db, _dir) = grouped_library().await;
    let album_id = "00000000-0000-4000-8000-000000000000";
    for (id, artist) in [("primary-again", ARTIST_A), ("other-credit", ARTIST_B)] {
        exec(&db,
            "INSERT INTO album_artists (id, album_id, artist_id, position, _updated_at, created_at) \
             VALUES (?, ?, ?, 1, 'seed', '2026-01-01T00:00:00Z')",
            &[id, album_id, artist]).await;
    }
    let mut browse = db.subscribe_album_browse(
        &[],
        true,
        [LibraryPageWindow {
            offset: 0,
            limit: 20,
        }]
        .into(),
    );
    let initial = browse.next().await.into_result().unwrap();
    assert_eq!(initial.row_count, 6);
    exec(
        &db,
        "INSERT INTO artist_merges (id, into_artist_id, _updated_at, created_at) \
         VALUES (?, ?, 'seed', '2026-01-01T00:00:00Z')",
        &[ARTIST_B, ARTIST_A],
    )
    .await;
    let merged = browse.next().await.into_result().unwrap();
    assert_eq!(merged.total_count, 5);
    assert_eq!(merged.row_count, 5);
    assert_eq!(merged.sections.len(), 2);
    assert!(merged.sections.iter().all(|section| section.id != ARTIST_B));
    assert_eq!(
        merged.windows[0]
            .rows
            .iter()
            .filter(|row| row.id == album_id)
            .count(),
        1
    );
}
