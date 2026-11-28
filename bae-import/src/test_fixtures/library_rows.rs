//! Reads of the library's records that this crate's tests check, each through
//! a fragment of bae-records.

// Each catalog record of every release, as its catalog and key, in catalog
// order.
#[cfg(test)]
coven::read!(
    writer EveryReleaseRecord { record: String },
    r#"
    SELECT json_extract(held.value, '$.catalog') || ' ' || json_extract(held.value, '$.key')
        AS record
    FROM @bae_records::records::fragments::ReleaseRecords AS release,
        json_each(release.records) AS held
    ORDER BY json_extract(held.value, '$.catalog')
"#
);

// Each release file with the stored file it names and that file's hash and
// coordinates, as this device holds them.
#[cfg(test)]
coven::read!(
    writer ReleaseFilesStored { hash: Option<String> },
    r#"
    SELECT file.file_id, file.stored_file, hex(stored.hash) AS hash, stored.coordinates
    FROM @bae_records::records::spread_fragments::ReleaseFileRow AS file
    LEFT JOIN coven_files AS stored ON stored.id = file.stored_file
"#
);

// The names of the composers of the work `:work`, joined in credit order.
#[cfg(test)]
coven::read!(
    writer WorkComposers(work: String) { composers: Option<String> },
    r#"
    SELECT work.composers
    FROM @bae_records::records::browse_fragments::WorkSummary AS work
    WHERE work.work_id = :work
"#
);

// The tracks that perform the work `:work`, in id order.
#[cfg(test)]
coven::read!(
    writer TracksPerforming(work: String),
    r#"
    SELECT link.track_id
    FROM @bae_records::records::browse_fragments::TrackWorkLink AS link
    WHERE link.work_id = :work
    ORDER BY link.track_id
"#
);

// How many release role credits have the id `:role`.
#[cfg(test)]
coven::read!(
    writer ReleaseRoleCount(role: String) { held: i64 },
    r#"
    SELECT COUNT(*) AS held
    FROM @bae_records::records::browse_fragments::ReleaseRoleCredit AS role
    WHERE role.role_id = :role
"#
);

// How many track role credits have the id `:role`.
#[cfg(test)]
coven::read!(
    writer TrackRoleCount(role: String) { held: i64 },
    r#"
    SELECT COUNT(*) AS held
    FROM @bae_records::records::browse_fragments::TrackRoleCredit AS role
    WHERE role.role_id = :role
"#
);

// How many albums and releases the library holds.
#[cfg(test)]
coven::read!(
    writer LibraryCounts { albums: i64, releases: i64 },
    r#"
    SELECT (SELECT COUNT(*) FROM @bae_records::records::browse_fragments::AlbumRecord AS album)
            AS albums,
        (SELECT COUNT(*) FROM @bae_records::records::fragments::ReleaseAlbum AS release)
            AS releases
"#
);

/// How many albums and releases the library in `db` holds.
#[cfg(test)]
pub(crate) async fn library_counts(db: &bae_store::Store) -> LibraryCountsRow {
    db.read(|sql| bae_store::statement_row(&sql, &LibraryCountsParams {})).await.unwrap()
}
