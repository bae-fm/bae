//! What this device's upload queue holds of the library's rows, read from
//! coven's `coven_uploads` beside them.

use super::*;

/// How many of the release `?1`'s files wait in this device's upload queue.
pub(super) const RELEASE_QUEUED_FILE_COUNT: &str = "SELECT COUNT(*) FROM release_files AS f \
     JOIN coven_uploads AS upload \
         ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
     WHERE f.release_id = ?1";

/// Whether any of the release `?1`'s files waits in this device's upload
/// queue.
pub(super) const RELEASE_HAS_QUEUED_FILE: &str = "SELECT EXISTS (SELECT 1 FROM release_files AS f \
     JOIN coven_uploads AS upload \
         ON upload.table_name = 'release_files' AND upload.key = coven_key(f.id) \
     WHERE f.release_id = ?1)";

/// Whether the release file `?1` waits in this device's upload queue.
#[cfg(any(test, feature = "test-utils"))]
pub(super) const RELEASE_FILE_QUEUED: &str =
    "SELECT EXISTS (SELECT 1 FROM coven_uploads AS upload \
     WHERE upload.table_name = 'release_files' AND upload.key = coven_key(?1))";

impl Database {
    /// How many of the release's files still wait to upload from this device.
    pub async fn count_pending_uploads_for_release(
        &self,
        release_id: &str,
    ) -> Result<i64, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_QUEUED_FILE_COUNT, [release_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether any of the release's files still waits to upload from this
    /// device.
    pub async fn has_pending_uploads_for_release(&self, release_id: &str) -> Result<bool, DbError> {
        let release_id = release_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_HAS_QUEUED_FILE, [release_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }

    /// Whether an attached release file still waits to upload from this
    /// device.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn has_pending_cloud_upload(&self, file_id: &str) -> Result<bool, DbError> {
        let file_id = file_id.to_string();
        self.read(move |sql| {
            sql.query_row(RELEASE_FILE_QUEUED, [file_id], |row| row.get(0))
                .map_err(DbError::from)
        })
        .await
    }
}
