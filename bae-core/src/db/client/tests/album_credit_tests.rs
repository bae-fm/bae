//! An album's credits are one ordered list (`album_artists` from position 0):
//! every reader of an album's artists reads the whole list.

use super::super::*;
use super::{exec_batch, temp_db};

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
async fn credited_library() -> (Database, tempfile::TempDir) {
    let (db, dir) = temp_db().await;
    exec_batch(
        &db,
        &format!(
            "INSERT INTO artists (id, name, name_key, created_at) VALUES \
                ('{ARTIST_A}', 'Artist A', 'artist a', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_B}', 'Artist B', 'artist b', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_C}', 'Artist C', 'artist c', '2026-01-01T00:00:00Z'), \
                ('{ARTIST_VARIOUS}', 'Various Artists', 'various artists', '2026-01-01T00:00:00Z'); \
             INSERT INTO albums (id, title, is_compilation, created_at) VALUES \
                ('{ALBUM_A}', 'Third', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_AB}', 'First', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_AC}', 'Second', 0, '2026-01-01T00:00:00Z'), \
                ('{ALBUM_VARIOUS}', 'Compilation', 1, '2026-01-01T00:00:00Z'); \
             INSERT INTO album_artists (id, album_id, artist_id, position, created_at) VALUES \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c01', '{ALBUM_A}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c02', '{ALBUM_AB}', '{ARTIST_B}', 1, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c03', '{ALBUM_AB}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c04', '{ALBUM_AC}', '{ARTIST_A}', 0, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c05', '{ALBUM_AC}', '{ARTIST_C}', 1, '2026-01-01T00:00:00Z'), \
                ('7b1e0c3a-2d4f-4a6b-9c8d-0e1f2a3b4c06', '{ALBUM_VARIOUS}', '{ARTIST_VARIOUS}', 0, '2026-01-01T00:00:00Z'); \
             INSERT INTO releases (id, album_id, created_at, album_order) VALUES \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e01', '{ALBUM_A}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e02', '{ALBUM_AB}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e03', '{ALBUM_AC}', '2026-01-01T00:00:00Z', 0), \
                ('9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', '{ALBUM_VARIOUS}', '2026-01-01T00:00:00Z', 0); \
             INSERT INTO release_mediums (id, release_id, position, side_count, created_at) VALUES \
                ('c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 1, 1, '2026-01-01T00:00:00Z'); \
             INSERT INTO tracks (id, release_id, position, title, medium_id, side, track_number, duration_ms, created_at) VALUES \
                ('d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e01', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 0, 'Track Credited', 'c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', 1, 1, 1000, '2026-01-01T00:00:00Z'), \
                ('d4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e02', '9e2d4c6b-8a0f-4e1d-b3c5-7a9b1d3f5e04', 1, 'Track Uncredited', 'c1a3e5f7-0b2d-4f6a-8c0e-2a4c6e8a0c01', 1, 2, 1000, '2026-01-01T00:00:00Z'); \
             INSERT INTO track_artists (id, track_id, artist_id, position, created_at) VALUES \
                ('e5c7a9b1-2d4f-4b6d-8f2b-4d6f8b0d2f01', 'd4b6f8a0-1c3e-4a5c-9e1a-3c5e7a9c1e01', '{ARTIST_C}', 0, '2026-01-01T00:00:00Z');"
        ),
    )
    .await;
    (db, dir)
}

#[tokio::test]
async fn an_album_is_found_by_any_of_its_credits() {
    let (db, _dir) = credited_library().await;

    let results = db.search_library("Artist B", 10).await.unwrap();

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

    let results = db.search_library("Track", 10).await.unwrap();

    let tracks: Vec<(&str, &str)> = results
        .tracks
        .iter()
        .map(|track| (track.title.as_str(), track.artist_name.as_str()))
        .collect();
    assert_eq!(
        tracks,
        [
            ("Track Credited", "Artist C"),
            ("Track Uncredited", "Various Artists"),
        ]
    );
}

/// "A" sorts before "A & B", which sorts before "A & C", whatever the titles
/// say, in the storage table and in the album grid alike.
#[tokio::test]
async fn sorting_by_artist_compares_every_credit_in_order() {
    let (db, _dir) = credited_library().await;

    let storage = db
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
    let grid = db
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
    assert_eq!(
        storage
            .iter()
            .map(|row| row.album.id.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        grid.iter()
            .map(|album| album.id.as_str())
            .collect::<Vec<_>>(),
        expected
    );
}
