//! File-path / storage helpers and encryption-service access for
//! [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// The on-disk path of a release file that lives at the user's own path — a
    /// coven **user-provided Local** blob's external ref. `Ok(Some(path))` only
    /// for a Local release file coven holds an external ref for (the user's file
    /// in place); `Ok(None)` for a Remote file (its bytes are in coven's cache,
    /// keyed by id, with no stable bae path) or an unknown file. DB errors
    /// propagate so callers distinguish "no in-place file" from "library broken".
    ///
    /// Used where a caller needs the actual user file (the DiscID re-read of the
    /// rip's artifacts), not coven's locality-aware byte read.
    pub async fn file_local_path(&self, file_id: &str) -> Result<Option<PathBuf>, LibraryError> {
        Ok(self
            .database
            .external_blob(file_id)
            .await?
            .map(|ext| ext.path))
    }

    pub fn create_release_storage(&self) -> ReleaseStorageImpl {
        ReleaseStorageImpl::new_local(self.library_dir.clone())
    }

    pub async fn append_pending_deletions(
        &self,
        deletions: &[PendingDeletion],
    ) -> Result<(), String> {
        append_pending_deletions(self.library_dir.as_ref(), deletions)
            .await
            .map_err(|e| format!("{e}"))
    }

    // =========================================================================
    // Encryption
    // =========================================================================

    pub fn has_encryption(&self) -> bool {
        self.sync.has_encryption()
    }

    pub fn get_encryption_service(&self) -> Option<EncryptionService> {
        self.sync.get_encryption_service()
    }
}
