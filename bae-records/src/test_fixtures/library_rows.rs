//! The records' rows other crates' tests check a library by: what the library
//! holds, and the reads two devices converge on, each handed out as a
//! [`ComparedRead`].

use bae_store::test_devices::ComparedRead;
use bae_store::Store;

// How many pins coven holds on the file `:file`.
coven::read!(
    writer PinsOfFile(file: String) { pins: i64 },
    "SELECT COUNT(*) AS pins FROM coven_pins AS pin WHERE pin.file = :file"
);

// Whether coven records the file `:file`.
coven::read!(
    writer CovenFileHeld(file: String) { held: bool },
    "SELECT EXISTS (SELECT 1 FROM coven_files AS stored WHERE stored.id = :file) AS held"
);

// How many work links and role credits the track `:track` holds.
coven::read!(
    writer TrackLinks(track: String) { works: i64, roles: i64 },
    r#"
    SELECT (SELECT COUNT(*) FROM track_works AS link WHERE link.track_id = :track) AS works,
           (SELECT COUNT(*) FROM track_artist_roles AS role WHERE role.track_id = :track) AS roles
"#
);

// Every release's id.
coven::read!(writer EveryRelease, "SELECT release.id FROM releases AS release");

// Every album's id.
coven::read!(writer EveryAlbum, "SELECT album.id FROM albums AS album");

// Every work's id.
coven::read!(writer EveryWork, "SELECT work.id FROM works AS work");

// How many tracks the library holds.
coven::read!(writer TrackCount { tracks: u64 }, "SELECT COUNT(*) AS tracks FROM tracks AS track");

// A release of the album with the lowest id.
coven::read!(
    writer FirstAlbumRelease,
    r#"
    SELECT release.id
    FROM releases AS release
    WHERE release.album_id = (SELECT album.id FROM albums AS album ORDER BY album.id LIMIT 1)
    LIMIT 1
"#
);

// The album `album`'s releases and their places among its releases.
coven::read!(
    writer AlbumReleaseOrders(album: String),
    r#"
    SELECT release.id, release.album_order
    FROM releases AS release
    WHERE release.album_id = :album
"#
);

// Every release file with its release and the coven file it names.
coven::read!(
    writer EveryReleaseFile,
    "SELECT file.id, file.release_id, file.file FROM release_files AS file"
);

// Each release file's coven file, and the hash and place coven records for
// it.
coven::read!(
    writer ReleaseFilesStored,
    r#"
    SELECT file.id, file.file, stored.hash, stored.coordinates
    FROM release_files AS file
    LEFT JOIN coven_files AS stored ON stored.id = file.file
"#
);

// Each release file that coven records, with its coven file and its place.
coven::read!(
    writer ReleaseFilesPlaced,
    r#"
    SELECT file.id, file.file, stored.coordinates
    FROM release_files AS file
    JOIN coven_files AS stored ON stored.id = file.file
"#
);

// The coven files of the release `release`: its files and its cover's copies
// and original.
coven::read!(
    writer CovenFilesOfRelease(release: String) { file: String },
    r#"
    SELECT file.file AS file FROM release_files AS file WHERE file.release_id = :release
    UNION ALL
    SELECT cover.browse_file AS file FROM covers AS cover WHERE cover.id = :release
    UNION ALL
    SELECT cover.large_file AS file FROM covers AS cover WHERE cover.id = :release
    UNION ALL
    SELECT cover.original_file AS file FROM covers AS cover
    WHERE cover.id = :release AND cover.original_file IS NOT NULL
"#
);

// Each release's cover's browse copy.
coven::read!(writer CoverBrowseFiles, "SELECT cover.id, cover.browse_file FROM covers AS cover");

// Each release's cover's browse copy, and the hash and place coven records
// for it.
coven::read!(
    writer CoverBrowseCopies,
    r#"
    SELECT cover.id, cover.browse_file, stored.hash, stored.coordinates
    FROM covers AS cover
    LEFT JOIN coven_files AS stored ON stored.id = cover.browse_file
"#
);

// Each release's media rows.
coven::read!(
    writer EveryReleaseMedia,
    r#"
    SELECT media.id, media.release_id, media.position, media.medium, media.count
    FROM release_media AS media
"#
);

// Each release's catalog records.
coven::read!(
    writer EveryCatalogRecord,
    "SELECT record.id, record.release_id, record.catalog FROM release_records AS record"
);

// Each artist merge, by absorbed artist: the absorbed artist and the artist
// it was merged into.
coven::read!(
    writer EveryArtistMerge,
    "SELECT merge.id, merge.into_artist_id FROM artist_merges AS merge ORDER BY merge.id"
);

// Each audio segment's scrub points.
coven::read!(
    writer EveryScrubPoints,
    r#"
    SELECT scrubbed.audio_format_segment_id, scrubbed.points
    FROM audio_segment_scrub_points AS scrubbed
"#
);

/// Every release `store` holds.
pub async fn release_ids(store: &Store) -> Vec<String> {
    let rows = store.read_rows(EveryReleaseParams {}).await.unwrap();
    rows.into_iter().map(|row| row.id).collect()
}

/// Every album `store` holds.
pub async fn album_ids(store: &Store) -> Vec<String> {
    let rows = store.read_rows(EveryAlbumParams {}).await.unwrap();
    rows.into_iter().map(|row| row.id).collect()
}

/// Every work `store` holds.
pub async fn work_ids(store: &Store) -> Vec<String> {
    let rows = store.read_rows(EveryWorkParams {}).await.unwrap();
    rows.into_iter().map(|row| row.id).collect()
}

/// How many tracks `store` holds.
pub async fn track_count(store: &Store) -> u64 {
    store.read_row(TrackCountParams {}).await.unwrap().tracks
}

/// A release of the album with the lowest id in `store`.
pub async fn first_album_release(store: &Store) -> String {
    store.read_row(FirstAlbumReleaseParams {}).await.unwrap().id
}

/// The coven files of the releases `releases` in `store`: their files and
/// their covers' copies and originals.
pub async fn coven_files_of_releases(store: &Store, releases: &[&str]) -> Vec<String> {
    let mut files = Vec::new();
    for release in releases {
        let rows = store
            .read_rows(CovenFilesOfReleaseParams { release: release.to_string() })
            .await
            .unwrap();
        files.extend(rows.into_iter().map(|row| row.file));
    }
    files
}

/// How many pins coven holds on the file `file` in `store`.
pub async fn pins_of_file(store: &Store, file: &str) -> i64 {
    store.read_row(PinsOfFileParams { file: file.to_string() }).await.unwrap().pins
}

/// Whether coven records the file `file` in `store`.
pub async fn coven_file_held(store: &Store, file: &str) -> bool {
    store.read_row(CovenFileHeldParams { file: file.to_string() }).await.unwrap().held
}

/// How many work links and role credits the track `track` holds in `store`.
pub async fn track_links(store: &Store, track: &str) -> (i64, i64) {
    let row = store.read_row(TrackLinksParams { track: track.to_string() }).await.unwrap();
    (row.works, row.roles)
}

/// Every release's id.
pub fn every_release() -> impl ComparedRead {
    EveryReleaseParams {}
}

/// Every album's id.
pub fn every_album() -> impl ComparedRead {
    EveryAlbumParams {}
}

/// The album `album`'s releases and their places among its releases.
pub fn album_release_orders(album: &str) -> impl ComparedRead {
    AlbumReleaseOrdersParams { album: album.to_string() }
}

/// Every release file with its release and the coven file it names.
pub fn every_release_file() -> impl ComparedRead {
    EveryReleaseFileParams {}
}

/// Each release file's coven file, and the hash and place coven records for
/// it.
pub fn release_files_stored() -> impl ComparedRead {
    ReleaseFilesStoredParams {}
}

/// Each release file that coven records, with its coven file and its place.
pub fn release_files_placed() -> impl ComparedRead {
    ReleaseFilesPlacedParams {}
}

/// Each release's cover's browse copy.
pub fn cover_browse_files() -> impl ComparedRead {
    CoverBrowseFilesParams {}
}

/// Each release's cover's browse copy, and the hash and place coven records
/// for it.
pub fn cover_browse_copies() -> impl ComparedRead {
    CoverBrowseCopiesParams {}
}

/// Each release's media rows.
pub fn every_release_media() -> impl ComparedRead {
    EveryReleaseMediaParams {}
}

/// Each release's catalog records.
pub fn every_catalog_record() -> impl ComparedRead {
    EveryCatalogRecordParams {}
}

/// Each artist merge.
pub fn every_artist_merge() -> impl ComparedRead {
    EveryArtistMergeParams {}
}

/// Each audio segment's scrub points.
pub fn every_scrub_points() -> impl ComparedRead {
    EveryScrubPointsParams {}
}
