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

    /// What removing this library from this device would lose, for the
    /// person to confirm before [`Self::remove_library`].
    pub async fn unsent_before_removal(&self) -> Result<coven::UnsentWork, LibraryError> {
        Ok(self.database.unsent_before_removal().await?)
    }

    /// The caller stops its playback and import services before removal.
    /// This ends the manager's workers before leaving, and discards what this
    /// device hasn't uploaded. An offline refusal preserves the store; retry
    /// removal on this manager, or reopen it to resume normal use.
    pub async fn remove_library(&self) -> Result<(), crate::library::LibraryRemovalError> {
        let id = self.database.store_id();
        self.tasks.close().await;
        crate::library::local_lifecycle::remove_recorded(
            &self.app_dir,
            id,
            self.database.delete_store(),
        )
        .await
    }

    pub async fn forget_encryption_key(&self) -> Result<(), LibraryError> {
        self.database.forget_store_keys().await?;
        Ok(())
    }

    pub fn store_key_state(&self) -> Result<coven::StoreKeyState, LibraryError> {
        Ok(self.database.store_key_state()?)
    }
}
