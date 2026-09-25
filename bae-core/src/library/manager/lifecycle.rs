//! Library-lifecycle operations for [`LibraryManager`]: rename a library and
//! lock it (forget the active encryption key). Removing a library from this
//! device happens once it is closed ([`LibraryManager::close`]), through
//! [`crate::library::remove_local_library`].

use super::*;

impl LibraryManager {
    /// Rename a library by id. The active library renames through the reactive
    /// `ConfigHandle`, so current subscribers see it; any other library isn't loaded
    /// in memory, so its `config.yaml` is edited on disk instead. The name is
    /// already validated non-blank by its type.
    pub fn rename_library(
        &self,
        library_id: &str,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), LibraryError> {
        if library_id == self.config_handle.config().store_id {
            self.config_handle.rename_library(name)?;
            return Ok(());
        }
        crate::config::rename_inactive_library(&self.app_dir, library_id, name)?;
        Ok(())
    }

    /// Lock the active library by asking Coven to stop every operation retaining
    /// the master key before removing it from custody.
    pub async fn forget_encryption_key(&self) -> Result<(), LibraryError> {
        self.sync.forget_master_key().await
    }
}
