#![cfg(feature = "test-utils")]
mod support;
use bae_core::db::{
    AlbumSortCriterion, AlbumSortField, Database, DbAlbum, DbArtist, SortDirection,
};
use chrono::{Duration, Utc};
use tempfile::TempDir;
use uuid::Uuid;

async fn setup_db() -> (Database, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let database = Database::new_test(
        db_path.to_str().unwrap(),
        std::sync::Arc::new(coven::SystemClock),
    )
    .await
    .expect("Failed to create database");
    (database, temp_dir)
}

fn make_album(
    title: &str,
    artist_id: &str,
    year: Option<i32>,
    created_offset_hours: i64,
) -> DbAlbum {
    let now = Utc::now();
    DbAlbum {
        id: Uuid::new_v4().to_string(),
        title: title.to_string(),
        artist_id: artist_id.to_string(),
        year,
        primary_release_id: None,
        is_compilation: false,
        created_at: now + Duration::hours(created_offset_hours),
    }
}

async fn insert_default_artist(db: &Database) -> String {
    let artist = DbArtist {
        id: "default-test-artist".to_string(),
        name: "Default Artist".to_string(),
        sort_name: None,
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: Utc::now(),
    };
    db.insert_artist(&artist).await.unwrap();
    artist.id
}

fn make_artist(name: &str, sort_name: Option<&str>) -> DbArtist {
    let now = Utc::now();
    DbArtist {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        sort_name: sort_name.map(|s| s.to_string()),
        discogs_artist_id: None,
        musicbrainz_artist_id: None,
        created_at: now,
    }
}

#[tokio::test]
async fn test_default_sort_is_date_added_desc() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let old = make_album("Old Album", &aid, Some(2020), -2);
    let new = make_album("New Album", &aid, Some(2024), 0);
    db.insert_album(&old).await.unwrap();
    db.insert_album(&new).await.unwrap();

    // Empty criteria = default = created_at DESC (newest first)
    let albums = db.get_albums(&[]).await.unwrap();
    assert_eq!(albums.len(), 2);
    assert_eq!(albums[0].title, "New Album");
    assert_eq!(albums[1].title, "Old Album");
}

#[tokio::test]
async fn test_sort_by_title_ascending() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let c = make_album("Charlie", &aid, Some(2020), 0);
    let a = make_album("Alpha", &aid, Some(2021), 1);
    let b = make_album("Bravo", &aid, Some(2022), 2);
    db.insert_album(&c).await.unwrap();
    db.insert_album(&a).await.unwrap();
    db.insert_album(&b).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Title,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Alpha");
    assert_eq!(albums[1].title, "Bravo");
    assert_eq!(albums[2].title, "Charlie");
}

#[tokio::test]
async fn test_sort_by_title_descending() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let a = make_album("Alpha", &aid, Some(2020), 0);
    let b = make_album("Bravo", &aid, Some(2021), 1);
    db.insert_album(&a).await.unwrap();
    db.insert_album(&b).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Title,
            direction: SortDirection::Descending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Bravo");
    assert_eq!(albums[1].title, "Alpha");
}

#[tokio::test]
async fn test_sort_by_title_is_case_insensitive() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let lower = make_album("alpha", &aid, Some(2020), 0);
    let upper = make_album("Bravo", &aid, Some(2021), 1);
    db.insert_album(&lower).await.unwrap();
    db.insert_album(&upper).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Title,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "alpha");
    assert_eq!(albums[1].title, "Bravo");
}

#[tokio::test]
async fn test_sort_by_year_ascending_nulls_last() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let no_year = make_album("No Year", &aid, None, 0);
    let y2020 = make_album("Year 2020", &aid, Some(2020), 1);
    let y2010 = make_album("Year 2010", &aid, Some(2010), 2);
    db.insert_album(&no_year).await.unwrap();
    db.insert_album(&y2020).await.unwrap();
    db.insert_album(&y2010).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Year,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Year 2010");
    assert_eq!(albums[1].title, "Year 2020");
    assert_eq!(albums[2].title, "No Year"); // NULL last for ascending
}

#[tokio::test]
async fn test_sort_by_year_descending_nulls_first() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let no_year = make_album("No Year", &aid, None, 0);
    let y2020 = make_album("Year 2020", &aid, Some(2020), 1);
    let y2010 = make_album("Year 2010", &aid, Some(2010), 2);
    db.insert_album(&no_year).await.unwrap();
    db.insert_album(&y2020).await.unwrap();
    db.insert_album(&y2010).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Year,
            direction: SortDirection::Descending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "No Year"); // NULL first for descending
    assert_eq!(albums[1].title, "Year 2020");
    assert_eq!(albums[2].title, "Year 2010");
}

#[tokio::test]
async fn test_sort_by_date_added_ascending() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let oldest = make_album("Oldest", &aid, Some(2024), -3);
    let newest = make_album("Newest", &aid, Some(2024), 0);
    let middle = make_album("Middle", &aid, Some(2024), -1);
    db.insert_album(&oldest).await.unwrap();
    db.insert_album(&newest).await.unwrap();
    db.insert_album(&middle).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::DateAdded,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Oldest");
    assert_eq!(albums[1].title, "Middle");
    assert_eq!(albums[2].title, "Newest");
}

#[tokio::test]
async fn test_sort_by_artist_ascending() {
    let (db, _dir) = setup_db().await;

    let artist_z = make_artist("Zebra", None);
    let artist_a = make_artist("Alpha", None);
    db.insert_artist(&artist_z).await.unwrap();
    db.insert_artist(&artist_a).await.unwrap();

    let album_z = make_album("Some Album", &artist_z.id, Some(2020), 0);
    let album_a = make_album("Another Album", &artist_a.id, Some(2021), 1);
    db.insert_album(&album_z).await.unwrap();
    db.insert_album(&album_a).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Artist,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Another Album"); // artist Alpha
    assert_eq!(albums[1].title, "Some Album"); // artist Zebra
}

#[tokio::test]
async fn test_sort_by_artist_uses_sort_name() {
    let (db, _dir) = setup_db().await;

    let artist_the = make_artist("The Zebras", Some("Zebras, The"));
    let artist_a = make_artist("Alpha", None);
    db.insert_artist(&artist_the).await.unwrap();
    db.insert_artist(&artist_a).await.unwrap();

    let album_the = make_album("Album By The Zebras", &artist_the.id, Some(2020), 0);
    let album_a = make_album("Album By Alpha", &artist_a.id, Some(2021), 1);
    db.insert_album(&album_the).await.unwrap();
    db.insert_album(&album_a).await.unwrap();

    let albums = db
        .get_albums(&[AlbumSortCriterion {
            field: AlbumSortField::Artist,
            direction: SortDirection::Ascending,
        }])
        .await
        .unwrap();

    // Alpha sorts before "Zebras, The" (sort_name)
    assert_eq!(albums[0].title, "Album By Alpha");
    assert_eq!(albums[1].title, "Album By The Zebras");
}

#[tokio::test]
async fn test_multi_criteria_artist_then_year() {
    let (db, _dir) = setup_db().await;

    let artist = make_artist("Same Artist", None);
    db.insert_artist(&artist).await.unwrap();

    let album_2020 = make_album("Album 2020", &artist.id, Some(2020), 0);
    let album_2024 = make_album("Album 2024", &artist.id, Some(2024), 1);
    let album_2022 = make_album("Album 2022", &artist.id, Some(2022), 2);

    for album in [&album_2020, &album_2024, &album_2022] {
        db.insert_album(album).await.unwrap();
    }

    // Sort by artist ASC, then year DESC
    let albums = db
        .get_albums(&[
            AlbumSortCriterion {
                field: AlbumSortField::Artist,
                direction: SortDirection::Ascending,
            },
            AlbumSortCriterion {
                field: AlbumSortField::Year,
                direction: SortDirection::Descending,
            },
        ])
        .await
        .unwrap();

    // All same artist, so secondary sort by year descending
    assert_eq!(albums[0].title, "Album 2024");
    assert_eq!(albums[1].title, "Album 2022");
    assert_eq!(albums[2].title, "Album 2020");
}

#[tokio::test]
async fn test_album_index_matches_page_position() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    // A handful of albums with distinct titles so a title sort is total.
    let titles = ["Delta", "alpha", "Charlie", "Bravo", "echo"];
    for (i, title) in titles.iter().enumerate() {
        db.insert_album(&make_album(title, &aid, Some(2000), i as i64))
            .await
            .unwrap();
    }

    let sort = [AlbumSortCriterion {
        field: AlbumSortField::Title,
        direction: SortDirection::Ascending,
    }];

    // The paged list is the ground truth; the index query must agree with it.
    let page = db.get_album_page(&sort, 0, 100).await.unwrap();
    assert_eq!(page.len(), titles.len());

    for (position, summary) in page.iter().enumerate() {
        let index = db.get_album_index(&sort, &summary.id).await.unwrap();
        assert_eq!(index, Some(position as u64), "album {}", summary.title);
    }

    // An id that isn't in the library has no position.
    let missing = db.get_album_index(&sort, "no-such-album").await.unwrap();
    assert_eq!(missing, None);
}

#[tokio::test]
async fn test_album_index_matches_page_position_default_sort() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    for i in 0..4 {
        db.insert_album(&make_album(&format!("Album {i}"), &aid, Some(2000), i))
            .await
            .unwrap();
    }

    // Empty sort exercises the default `created_at DESC` order, which needs no
    // artist join — same path `get_album_page` takes for the default sort.
    let page = db.get_album_page(&[], 0, 100).await.unwrap();
    for (position, summary) in page.iter().enumerate() {
        let index = db.get_album_index(&[], &summary.id).await.unwrap();
        assert_eq!(index, Some(position as u64));
    }
}

#[tokio::test]
async fn test_album_index_matches_page_position_on_ties() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    // Many albums sharing one title and one created_at offset: the sort value
    // ties on every row, so only the `a.id` tiebreaker gives a total order.
    // The window-function index query and the LIMIT/OFFSET page query must
    // still agree row-for-row.
    for _ in 0..8 {
        db.insert_album(&make_album("Same Title", &aid, Some(2000), 0))
            .await
            .unwrap();
    }

    let sort = [AlbumSortCriterion {
        field: AlbumSortField::Title,
        direction: SortDirection::Ascending,
    }];
    let page = db.get_album_page(&sort, 0, 100).await.unwrap();
    assert_eq!(page.len(), 8);
    for (position, summary) in page.iter().enumerate() {
        let index = db.get_album_index(&sort, &summary.id).await.unwrap();
        assert_eq!(index, Some(position as u64), "album {}", summary.id);
    }
}

#[tokio::test]
async fn test_multi_criteria_year_asc_then_title_asc() {
    let (db, _dir) = setup_db().await;
    let aid = insert_default_artist(&db).await;

    let a = make_album("Bravo", &aid, Some(2020), 0);
    let b = make_album("Alpha", &aid, Some(2020), 1);
    let c = make_album("Charlie", &aid, Some(2019), 2);
    db.insert_album(&a).await.unwrap();
    db.insert_album(&b).await.unwrap();
    db.insert_album(&c).await.unwrap();

    let albums = db
        .get_albums(&[
            AlbumSortCriterion {
                field: AlbumSortField::Year,
                direction: SortDirection::Ascending,
            },
            AlbumSortCriterion {
                field: AlbumSortField::Title,
                direction: SortDirection::Ascending,
            },
        ])
        .await
        .unwrap();

    assert_eq!(albums[0].title, "Charlie"); // 2019
    assert_eq!(albums[1].title, "Alpha"); // 2020, "Alpha" < "Bravo"
    assert_eq!(albums[2].title, "Bravo"); // 2020
}
