//! The reads bae-library's tests and integration tests check a library with:
//! the releases, albums, works, tracks, catalog records and artist merges it
//! holds, through the records' fragments, and the names coven gives its
//! circles.

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

// Each release's catalog records.
coven::read!(
    writer EveryCatalogRecord,
    r#"
    SELECT record.id, record.release_id, record.catalog
    FROM @bae_records::records::release_records::CatalogRecords AS record
"#
);

// Each artist merge.
coven::read!(
    writer EveryArtistMerge,
    r#"
    SELECT merge.id, merge.into_artist_id
    FROM @bae_records::records::identity_merges::ArtistMerges AS merge
"#
);

/// Run sync on both `devices` until both hold the same catalog records.
#[cfg(desktop)]
pub async fn converge_catalog_records(devices: &crate::test_libraries::TwoDevices) {
    devices.converge_rows(|| EveryCatalogRecordParams {}).await;
}

/// Run sync on both `devices` until both hold the same artist merges.
#[cfg(desktop)]
pub async fn converge_artist_merges(devices: &crate::test_libraries::TwoDevices) {
    devices.converge_rows(|| EveryArtistMergeParams {}).await;
}

// Each release file's coven file, and the hash and place coven records for
// it.
#[cfg(test)]
coven::read!(
    writer ReleaseFilesStored,
    r#"
    SELECT file.file_id, file.stored_file, stored.hash, stored.coordinates
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    LEFT JOIN coven_files AS stored ON stored.id = file.stored_file
"#
);

// Each release file that coven records, with its coven file and its place.
#[cfg(test)]
coven::read!(
    writer ReleaseFilesPlaced,
    r#"
    SELECT file.file_id, file.stored_file, stored.coordinates
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    JOIN coven_files AS stored ON stored.id = file.stored_file
"#
);

// The album `album`'s releases and their places among its releases.
#[cfg(test)]
coven::read!(
    writer AlbumReleaseOrders(album: String),
    r#"
    SELECT release.release_id, release.album_order
    FROM @bae_records::records::fragments::ReleaseAlbumOrder AS release
    WHERE release.album_id = :album
"#
);

// Each audio segment's scrub points.
#[cfg(test)]
coven::read!(
    writer EveryScrubPoints,
    r#"
    SELECT scrubbed.audio_format_segment_id, scrubbed.points
    FROM @bae_records::records::track_audio::SegmentScrubPoints AS scrubbed
"#
);
