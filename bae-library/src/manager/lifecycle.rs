//! Names, removal, keys and memory follow coven's shared store lifecycle.

use super::*;

impl LibraryManager {
    /// Coven checks administrator authority and records the rename on this
    /// device at once, offline too; sync publishes the new name.
    pub async fn rename_library(&self, name: &bae_store::EnteredName) -> Result<(), LibraryError> {
        self.membership.rename_store(name.as_str()).await?;
        Ok(())
    }

    pub async fn library_name(&self) -> Result<String, LibraryError> {
        Ok(self.store.store_name().await?)
    }

    pub fn subscribe_library_name(&self) -> coven::LiveQuery<String, coven::CovenError> {
        self.store.subscribe_store_name()
    }

    /// What removing this library from this device would lose, for the
    /// person to confirm before [`Self::remove_library`].
    pub async fn unsent_before_removal(&self) -> Result<coven::UnsentWork, LibraryError> {
        Ok(self.store.unsent_before_removal().await?)
    }

    /// The caller stops its playback and import services before removal.
    /// This ends the manager's workers before leaving, and discards what this
    /// device hasn't uploaded. An offline refusal preserves the store; retry
    /// removal on this manager, or reopen it to resume normal use. `coven`,
    /// the installation the library was opened from, finishes a deletion
    /// whose close failed after leaving. `steps` hears each step as it
    /// starts.
    pub async fn remove_library(
        &self,
        coven: &coven::Coven,
        steps: &(dyn Fn(crate::ClosingStep) + Send + Sync),
    ) -> Result<(), crate::LibraryRemovalError> {
        let id = self.store.store_id();
        steps(crate::ClosingStep::FinishingWork);
        self.tasks.close().await;
        crate::local_lifecycle::remove_recorded(
            &self.app_dir,
            coven,
            &self.diagnostics,
            id,
            self.store.delete_store(),
            steps,
        )
        .await
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
