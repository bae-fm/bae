//! Device-local playback-state persistence for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Write the device-local playback-state row. Propagates the DB error so the
    /// caller can distinguish a write failure from a stored absence; the resume
    /// row is a device-local cache, so the call site logs and continues rather
    /// than treating a failed write as fatal to playback.
    pub async fn save_playback_state(
        &self,
        state: &crate::db::DbPlaybackState,
    ) -> Result<(), LibraryError> {
        Ok(self.database.save_playback_state(state).await?)
    }

    /// Read the device-local playback-state row (kept, not deleted — it's
    /// overwritten on the next playback change and cleared on stop). `Ok(None)`
    /// means no row is stored; an `Err` is a read failure, kept distinct from
    /// absence so the caller doesn't silently start fresh on a DB error.
    pub async fn load_playback_state(
        &self,
    ) -> Result<Option<crate::db::DbPlaybackState>, LibraryError> {
        Ok(self.database.load_playback_state().await?)
    }

    /// Delete the device-local playback-state row (playback stopped).
    pub async fn clear_playback_state(&self) -> Result<(), LibraryError> {
        Ok(self.database.clear_playback_state().await?)
    }
}
