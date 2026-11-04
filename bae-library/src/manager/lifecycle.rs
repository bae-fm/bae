//! Names, removal, keys and memory follow coven's shared store lifecycle.

use super::*;

impl LibraryManager {
    /// Coven checks administrator authority and records the rename on this
    /// device at once, offline too; sync publishes the new name.
    pub async fn rename_library(&self, name: &bae_db::EnteredName) -> Result<(), LibraryError> {
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
    /// removal on this manager, or reopen it to resume normal use. `steps`
    /// hears each step as it starts.
    pub async fn remove_library(
        &self,
        steps: &(dyn Fn(crate::ClosingStep) + Send + Sync),
    ) -> Result<(), crate::LibraryRemovalError> {
        let id = self.database.store_id();
        steps(crate::ClosingStep::FinishingWork);
        self.tasks.close().await;
        crate::local_lifecycle::remove_recorded(
            &self.app_dir,
            &self.diagnostics,
            id,
            self.database.delete_store(),
            steps,
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

    /// Give back the memory the store's SQLite connections keep cached, for
    /// when the OS warns of low memory or the app leaves the screen. Reads
    /// after it load what they need again.
    pub async fn release_memory(&self) -> Result<(), LibraryError> {
        Ok(self.database.release_memory().await?)
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
