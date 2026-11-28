//! Where a library's files are, for [`LibraryManager`]: the original a file
//! has on this device, and this device's id in the store.

use super::*;

impl LibraryManager {
    /// The path of the original coven records on this device for a file,
    /// whether or not it has been uploaded. `Ok(None)` means there is no
    /// original here or no such row; coven's cached copies expose no path.
    /// Database errors propagate.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self.records.external_file(file_id).await?.map(|ext| ext.path))
    }
}

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

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
