//! The album grid and the artist list over one small library: how they
//! sort, and how a release leaving or an artist merging moves what they list.

use crate::db::client::tests::album_listing_tests::*;
use crate::db::LibraryPageWindow;
use crate::library::views::*;
use std::collections::BTreeSet;

/// The grid's album ids in `field` order, `direction`.
async fn album_ids(db: &Database, field: AlbumSortField, direction: SortDirection) -> Vec<String> {
    crate::library::views::LibraryViews::new(db.clone())
        .get_album_page(&[AlbumSortCriterion { field, direction }], 0, 50)
        .await
        .unwrap()
        .into_iter()
        .map(|album| album.id)
        .collect()
}

/// The artist list: each artist's id and album count, by name.
async fn artists(db: &Database) -> Vec<(String, i64)> {
    crate::library::views::LibraryViews::new(db.clone())
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
    let mut live = crate::library::views::LibraryViews::new(db.clone()).subscribe_album_browse(
        &[],
        true,
        windows,
    );
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

    exec(
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
    assert_eq!(
        crate::library::views::LibraryViews::new(db.clone()).get_album_count().await.unwrap(),
        3
    );
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 2)]);

    exec(&db, "DELETE FROM releases WHERE id = '5a905d5d-d400-42f3-a9a8-b858f18dcbc3';").await;
    assert_eq!(
        crate::library::views::LibraryViews::new(db.clone()).get_album_count().await.unwrap(),
        2
    );
    assert!(!album_ids(&db, AlbumSortField::Title, SortDirection::Ascending)
        .await
        .contains(&ALBUM_ONE.to_string()));
    assert_eq!(artists(&db).await, vec![(ALPHA.to_string(), 2), (BRAVO.to_string(), 1)]);
    assert_eq!(
        crate::library::views::LibraryViews::new(db.clone()).get_artist_count().await.unwrap(),
        2
    );

    // A release arriving under the album lists it again.
    exec(
        &db,
        "INSERT INTO releases (id, album_id, created_at, album_order) VALUES
            ('24b929ff-1f01-4621-9891-81abea7fbe87', '07d22f7d-3980-4c81-9d27-8fcd50ee3464', '2026-01-01T00:00:00Z', 0);",
    )
    .await;
    assert_eq!(
        crate::library::views::LibraryViews::new(db.clone()).get_album_count().await.unwrap(),
        3
    );
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

    exec(
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
    let survivor = crate::library::views::LibraryViews::new(db.clone())
        .find_artist_detail(BRAVO)
        .await
        .unwrap()
        .expect("listed");
    assert_eq!(survivor.albums.len(), 3);
    assert!(crate::library::views::LibraryViews::new(db.clone())
        .find_artist_detail(ALPHA)
        .await
        .unwrap()
        .is_none());
}
