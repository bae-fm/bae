//! The reads bae-library's tests check a library with: the releases, albums,
//! works and tracks it holds, each release's storage and listing counts
//! against those probed from its files, and coven's circles, operations, pins
//! and uploads. Each reads another domain's rows through that domain's
//! fragments.

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

// Each release whose storage row's pinned or file count differs from the one
// probed from its files, or that has no storage row, and each whose listing
// row is missing or holds a pinned or file count other than its storage
// row's.
#[cfg(test)]
coven::read!(
    writer PinDisagreements { disagreement: String },
    r#"
    SELECT release.release_id || ': stored '
               || COALESCE(stored.pinned_count || ' of ' || stored.file_count, 'nothing')
               || ', listed '
               || COALESCE(listed.pinned_count || ' of ' || listed.file_count, 'nothing')
               || ', probed ' || probe.pinned || ' of ' || probe.files AS disagreement
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    LEFT JOIN @bae_records::release_storage::ReleaseStorageCounts AS stored
        ON stored.release_id = release.release_id
    LEFT JOIN @bae_views::storage::ListedStorageCounts AS listed
        ON listed.release_id = release.release_id
    JOIN (SELECT each.release_id,
                 (SELECT COUNT(*)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = each.release_id) AS files,
                 (SELECT COUNT(*)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = each.release_id
                    AND EXISTS (SELECT 1 FROM coven_pins AS pin
                                WHERE pin.file = file.stored_file)) AS pinned
          FROM @bae_records::records::fragments::ReleaseAlbum AS each) AS probe
        ON probe.release_id = release.release_id
    WHERE stored.release_id IS NULL OR stored.pinned_count <> probe.pinned
       OR stored.file_count <> probe.files
       OR listed.release_id IS NULL OR listed.pinned_count <> stored.pinned_count
       OR listed.file_count <> stored.file_count
"#
);

// The releases with files whose every file `coven_pins` lists, probed file by
// file.
#[cfg(test)]
coven::read!(
    writer ProbedPinnedReleases,
    r#"
    SELECT release.release_id
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    WHERE EXISTS (SELECT 1 FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  WHERE file.release_id = release.release_id)
      AND NOT EXISTS (SELECT 1
                      FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                      WHERE file.release_id = release.release_id
                        AND NOT EXISTS (SELECT 1 FROM coven_pins AS pin
                                        WHERE pin.file = file.stored_file))
"#
);

// The release `release`'s storage row: its file count, total size, whether it
// is stored, its pinned and queued counts, and whether its first queued place
// is the first place coven's queue holds for its files.
#[cfg(test)]
coven::read!(
    writer StorageRowText(release: String) { row: String },
    r#"
    SELECT stored.file_count || ' files, ' || stored.total_size || ' bytes, stored '
               || stored.stored || ', ' || stored.pinned_count || ' pinned, '
               || stored.queued_count || ' queued, '
               || CASE WHEN stored.first_queued_position IS (
                      SELECT MIN(queued.position)
                      FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                      JOIN coven_upload_queue AS queued ON queued.file = file.stored_file
                      WHERE file.release_id = stored.release_id)
                  THEN 'first place as queued' ELSE 'first place not as queued' END AS row
    FROM @bae_records::release_storage::ReleaseStorageCounts AS stored
    WHERE stored.release_id = :release
"#
);

// Each release whose storage row's queued count or first place differs from
// the ones probed from its files in coven's queue, or that has no storage
// row, and each whose listing row is missing or holds a queued count other
// than its storage row's.
#[cfg(test)]
coven::read!(
    writer QueueDisagreements { disagreement: String },
    r#"
    SELECT release.release_id || ': stored '
               || COALESCE(stored.queued_count || ' from '
                       || COALESCE(stored.first_queued_position, 'nowhere'), 'nothing')
               || ', listed ' || COALESCE(listed.queued_count, 'nothing')
               || ', probed ' || probe.queued || ' from ' || COALESCE(probe.first, 'nowhere')
                   AS disagreement
    FROM @bae_records::records::fragments::ReleaseAlbum AS release
    LEFT JOIN @bae_records::release_storage::ReleaseStorageCounts AS stored
        ON stored.release_id = release.release_id
    LEFT JOIN @bae_views::storage::ListedStorageCounts AS listed
        ON listed.release_id = release.release_id
    JOIN (SELECT each.release_id,
                 (SELECT COUNT(*)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  JOIN coven_uploads AS upload ON upload.file = file.stored_file
                  WHERE file.release_id = each.release_id) AS queued,
                 (SELECT MIN(upload.position)
                  FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
                  JOIN coven_uploads AS upload ON upload.file = file.stored_file
                  WHERE file.release_id = each.release_id) AS first
          FROM @bae_records::records::fragments::ReleaseAlbum AS each) AS probe
        ON probe.release_id = release.release_id
    WHERE stored.release_id IS NULL OR stored.queued_count <> probe.queued
       OR stored.first_queued_position IS NOT probe.first
       OR listed.release_id IS NULL OR listed.queued_count <> stored.queued_count
"#
);

// The releases with files in coven's queue, in the order their first files
// wait, probed file by file.
#[cfg(test)]
coven::read!(
    writer ProbedUploadingReleases,
    r#"
    SELECT file.release_id
    FROM coven_uploads AS upload
    JOIN @bae_records::records::spread_fragments::ReleaseFileRow AS file
        ON upload.file = file.stored_file
    GROUP BY file.release_id
    ORDER BY MIN(upload.position)
"#
);

// How many of the release `release`'s files coven lists no pin for.
#[cfg(test)]
coven::read!(
    writer UnpinnedFiles(release: String) { unpinned: i64 },
    r#"
    SELECT COUNT(*) AS unpinned
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    WHERE file.release_id = :release
      AND NOT EXISTS (SELECT 1 FROM coven_pins AS pin WHERE pin.file = file.stored_file)
"#
);

// The release `release`'s whole size, and the files its failed move names and
// its move waits for the original of, as its storage row holds them.
#[cfg(test)]
coven::read!(
    writer StoredSizeAndMove(release: String) { total_size: u64 },
    r#"
    SELECT stored.total_size, stored.failed_move_file, stored.missing_original_file
    FROM @bae_records::release_storage::ReleaseStorageCounts AS stored
    WHERE stored.release_id = :release
"#
);

// How many moves the release `release`'s queued files wait under.
#[cfg(test)]
coven::read!(
    writer MovesOfRelease(release: String) { moves: i64 },
    r#"
    SELECT COUNT(DISTINCT upload.move) AS moves
    FROM coven_uploads AS upload
    JOIN @bae_records::records::spread_fragments::ReleaseFileRow AS file
        ON file.stored_file = upload.file
    WHERE file.release_id = :release
"#
);

// The coven files of the release `release`: its files and its cover's copies
// and original.
#[cfg(test)]
coven::read!(
    writer CovenFilesOfRelease(release: String) { file: String },
    r#"
    SELECT file.stored_file AS file
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    WHERE file.release_id = :release
    UNION ALL
    SELECT cover.browse_file AS file
    FROM @bae_records::records::files::CoverFiles AS cover
    WHERE cover.release_id = :release
    UNION ALL
    SELECT cover.large_file AS file
    FROM @bae_records::records::files::CoverFiles AS cover
    WHERE cover.release_id = :release
    UNION ALL
    SELECT cover.original_file AS file
    FROM @bae_records::records::files::CoverFiles AS cover
    WHERE cover.release_id = :release AND cover.original_file IS NOT NULL
"#
);

// The bytes of the catalog image cache's rows.
#[cfg(all(test, desktop))]
coven::read!(
    writer CachedImageTotal { bytes: u64 },
    r#"
    SELECT COALESCE(SUM(image.bytes), 0) AS bytes
    FROM @bae_catalogs::remote_image::cached_rows::CachedImageBytes AS image
"#
);

// The kinds of the operations coven lists of the kind `kind`.
#[cfg(test)]
coven::read!(
    writer OperationsOfKind(kind: String),
    r#"
    SELECT operation.kind
    FROM coven_operations AS operation
    WHERE operation.kind = :kind
"#
);

// The outcome of each storage setup coven lists.
#[cfg(test)]
coven::read!(
    writer StorageSetupOutcomes { outcome: Option<coven::StorageSetupOutcome> },
    r#"
    SELECT operation.outcome
    FROM coven_operations AS operation
    WHERE operation.kind = 'StorageSetup'
"#
);
