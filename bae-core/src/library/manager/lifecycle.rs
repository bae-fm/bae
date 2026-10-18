//! Names and removal follow coven's shared store lifecycle.

use super::*;

impl LibraryManager {
    /// Coven enforces administrator authority and publishes the new name.
    pub async fn rename_library(
        &self,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), LibraryError> {
        self.database.rename_store(name.as_str()).await?;
        Ok(())
    }

    pub async fn library_name(&self) -> Result<String, LibraryError> {
        Ok(self.database.store_name().await?)
    }

    pub fn subscribe_library_name(&self) -> coven::LiveQuery<String> {
        self.database.subscribe_store_name()
    }

    /// The caller stops its playback and import services before removal.
    /// Coven leaves the store first; a failure returns without deleting it.
    pub async fn remove_library(&self, unsent: coven::IfUnsent) -> Result<(), LibraryError> {
        let id = self.database.store_id();
        self.database.delete_store(unsent).await?;
        self.tasks.close().await;
        crate::library::local_lifecycle::remove_preferences(&self.app_dir, id).await
    }

    pub async fn forget_encryption_key(&self) -> Result<(), LibraryError> {
        self.database.forget_store_keys().await?;
        Ok(())
    }

    pub fn store_key_state(&self) -> Result<coven::StoreKeyState, LibraryError> {
        Ok(self.database.store_key_state()?)
    }
}
