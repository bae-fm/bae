//! Library-lifecycle operations for [`LibraryManager`]: rename a library,
//! lock it (forget the active encryption key), and forget a local library
//! (remove its data directory, clear the active-library pointer, and drop the
//! master key). These mutate the library's on-disk presence and the
//! active-library pointer — distinct from the config-access surface in
//! `config.rs`, which only reads and writes config fields.

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

    /// Forget this library on this device: remove its data directory, clear the
    /// active-library pointer, and delete its master encryption key. The cloud copy,
    /// if any, is untouched — this drops only the device's local presence.
    ///
    /// The caller must drop this handle immediately afterward: the database lives in
    /// the directory being removed, so this has to be the handle's last operation.
    /// With the active pointer gone, the next launch re-discovers and opens another
    /// library, or onboards.
    pub async fn forget_library(&self) -> Result<(), LibraryError> {
        let (library_id, library_path, has_cloud_provider) = {
            let config = self.config_handle.config();
            (
                config.store_id.clone(),
                config.library_path().to_path_buf(),
                config.cloud_home.provider.is_some(),
            )
        };
        let registered_path = self.app_dir.registered_library(&library_id);
        if library_path != registered_path {
            return Err(LibraryError::Internal(format!(
                "library directory {} does not match library id {library_id}'s registered \
                 directory {}",
                library_path.display(),
                registered_path.display()
            )));
        }
        let removal = crate::library::local_lifecycle::prepare_local_library_removal(
            &self.app_dir,
            &library_id,
            crate::library::local_lifecycle::ActiveLibraryExpectation::MustNotNameAnotherLibrary,
        )?;
        if has_cloud_provider {
            self.database.disconnect_cloud_home().await?;
        }
        for name in [
            crate::keys::DISCOGS_API_KEY,
            crate::keys::MCP_BEARER_TOKEN,
            crate::keys::SUBSONIC_PASSWORD,
        ] {
            self.database.delete_host_secret(name)?;
        }
        self.database.forget_master_key().await?;
        removal.remove()
    }
}
