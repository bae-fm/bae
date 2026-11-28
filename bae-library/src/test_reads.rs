//! The reads bae-library's tests and integration tests check a library with:
//! the releases, albums, works and tracks it holds, through the records'
//! fragments, and the names coven gives its circles.

use bae_store::Store;

// Every release's id.
coven::read!(
    writer EveryRelease,
    r#"
    SELECT release.release_id
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
"#
);

// Every album's id.
coven::read!(
    writer EveryAlbum,
    r#"
    SELECT album.album_id
    FROM @bae_records::records::fragments::AlbumTitle AS album
"#
);

// Every work's id.
coven::read!(
    writer EveryWork,
    r#"
    SELECT work.work_id
    FROM @bae_records::records::browse_fragments::WorkSummary AS work
"#
);

// How many tracks the library holds.
coven::read!(
    writer TrackCount { tracks: u64 },
    r#"
    SELECT COUNT(*) AS tracks
    FROM @bae_records::records::spread_fragments::TrackRow AS track
"#
);

// A release of the album with the lowest id.
coven::read!(
    writer FirstAlbumRelease,
    r#"
    SELECT release.release_id
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    WHERE release.album_id = (SELECT album.album_id
                              FROM @bae_records::records::fragments::AlbumTitle AS album
                              ORDER BY album.album_id
                              LIMIT 1)
    LIMIT 1
"#
);

// The name coven gives the circle `circle`.
coven::read!(
    writer CircleName(circle: coven::CircleId),
    r#"
    SELECT circle.name
    FROM coven_circles AS circle
    WHERE circle.circle = :circle
"#
);

/// Every release `store` holds.
pub async fn release_ids(store: &Store) -> Vec<String> {
    store
        .read_rows(EveryReleaseParams {})
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.release_id)
        .collect()
}

/// Every album `store` holds.
pub async fn album_ids(store: &Store) -> Vec<String> {
    store
        .read_rows(EveryAlbumParams {})
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.album_id)
        .collect()
}

/// Every work `store` holds.
pub async fn work_ids(store: &Store) -> Vec<String> {
    store.read_rows(EveryWorkParams {}).await.unwrap().into_iter().map(|row| row.work_id).collect()
}

/// How many tracks `store` holds.
pub async fn track_count(store: &Store) -> u64 {
    store.read_row(TrackCountParams {}).await.unwrap().tracks
}

/// A release of the album with the lowest id in `store`.
pub async fn first_album_release(store: &Store) -> String {
    store.read_row(FirstAlbumReleaseParams {}).await.unwrap().release_id
}

/// The names coven gives the circle `circle` in `store`: one, or none when
/// it lists no such circle.
pub async fn circle_names(store: &Store, circle: coven::CircleId) -> Vec<String> {
    store
        .read_rows(CircleNameParams { circle })
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.name)
        .collect()
}
