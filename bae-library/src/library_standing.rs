//! Where a library stands with its storage, for the welcome's Details: read
//! from the open library's own sync status, or from coven without opening a
//! store that isn't open.

use crate::LibraryError;
use bae_diagnostics::Diagnostics;
use std::time::SystemTime;

/// A library's standing with its storage.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryStanding {
    pub state: StandingState,
    /// When sync last finished, if it ever did.
    pub last_synced: Option<SystemTime>,
}

/// Where a library stands with its storage.
#[derive(Debug, Clone, PartialEq)]
pub enum StandingState {
    /// This device committed writes or files it hasn't uploaded, whatever
    /// state sync was in.
    UnsentChanges,
    /// Nothing waits to upload: sync's state, worded as the Sync page words
    /// it.
    Sync(bae_sync_status::SyncShown),
}

impl LibraryStanding {
    /// The standing of a library from its sync status and what this device
    /// hasn't uploaded.
    pub(crate) fn of(
        sync: &bae_sync_status::SyncStatusSnapshot,
        unsent: coven::UnsentWork,
    ) -> Self {
        let state = if unsent.is_empty() {
            StandingState::Sync(sync.shown())
        } else {
            StandingState::UnsentChanges
        };
        Self { state, last_synced: sync.coven.last_synced }
    }
}

/// Read how the library `store`, which isn't open, last stood. Fails with
/// coven's `AlreadyOpen` while a handle runs the store, here or in another
/// process, and for a store coven refuses to open. A bug coven's reads carry
/// back is reported to `diagnostics`.
pub async fn closed_library_standing(
    coven: &coven::Coven,
    store: coven::StoreId,
    diagnostics: &Diagnostics,
) -> Result<LibraryStanding, LibraryError> {
    let directory = coven.store_dir(&store);
    let status = coven
        .closed_sync_status(&directory)
        .await
        .map_err(|error| coven::task::reported(diagnostics, error))?;
    let provider = coven::Coven::storage_config(&directory)
        .await
        .map_err(|error| coven::task::reported(diagnostics, error))?
        .map(|config| config.provider());
    Ok(LibraryStanding::of(
        &bae_sync_status::SyncStatusSnapshot::closed(status.sync, provider),
        status.unsent,
    ))
}

impl crate::LibraryManager {
    /// How this open library stands with its storage, from its live sync
    /// status.
    pub async fn standing(&self) -> Result<LibraryStanding, LibraryError> {
        let sync = self.sync_status().await?;
        let unsent = self.unsent_before_removal().await?;
        Ok(LibraryStanding::of(&sync, unsent))
    }
}

#[cfg(test)]
#[path = "library_standing_tests.rs"]
mod tests;
