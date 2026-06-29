//! Config access and Discogs token management for [`LibraryManager`].

use super::*;

impl LibraryManager {
    /// Get a snapshot of the current config.
    pub fn get_config(&self) -> crate::config::Config {
        self.config_handle.config().clone()
    }

    /// Rename a library by id. If the id matches the active library, the
    /// rename goes through the reactive `ConfigState` so all current
    /// subscribers see it. Otherwise the library's `config.yaml` on disk
    /// is edited in place — the inactive library isn't loaded into
    /// memory.
    pub fn rename_library(&self, library_id: &str, name: &str) -> Result<(), String> {
        if name.trim().is_empty() {
            return Err("Library name cannot be empty".to_string());
        }
        if library_id == self.config_handle.config().library_id {
            return self
                .config_handle
                .rename_library(name)
                .map_err(|e| format!("{e}"));
        }
        let bae_dir = crate::config::bae_dir().map_err(|e| format!("{e}"))?;
        crate::config::rename_inactive_library(&bae_dir, library_id, name)
            .map_err(|e| format!("{e}"))
    }

    /// Forget the active library's encryption key. The running
    /// `sync_manager` still holds the key in memory so this session
    /// keeps working; the next launch lands on `UnlockView` because
    /// the keyring is empty. Used by the sidebar's "Lock Library" action.
    pub fn forget_encryption_key(&self) -> Result<(), String> {
        self.key_service
            .forget_encryption_key()
            .map_err(|e| format!("{e}"))
    }

    /// Subscribe to the config-state stream; each change yields the whole latest
    /// `Config`.
    pub fn subscribe_config_changes(&self) -> tokio::sync::watch::Receiver<crate::config::Config> {
        self.config_handle.subscribe()
    }

    /// Set whether playback pauses between vinyl/cassette sides.
    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.pause_between_sides = enabled)
    }

    /// Forget this (local) library on this device: delete its master encryption
    /// key, clear the active-library pointer, and remove its data directory. The
    /// owner's cloud copy (if any) is untouched — this only drops the device's
    /// local presence.
    ///
    /// The caller must drop this handle immediately afterward: the database
    /// lives in the directory being removed, so this must be the handle's last
    /// operation. The next launch re-discovers and opens another library (or
    /// onboards) since the active pointer is gone.
    pub fn forget_library(&self) -> Result<(), String> {
        if let Err(e) = self.key_service.delete_encryption_key() {
            warn!("Failed to delete encryption key while forgetting library: {e}");
        }

        let library_id = self.config_handle.config().library_id.clone();
        let bae_dir = crate::config::bae_dir().map_err(|e| e.to_string())?;

        let active_pointer = bae_dir.join("active-library");
        if active_pointer.exists() {
            if let Err(e) = std::fs::remove_file(&active_pointer) {
                warn!("Failed to clear active-library pointer: {e}");
            }
        }

        if let Some(dir) = crate::config::library_data_dir(&bae_dir, &library_id) {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                warn!("Failed to remove library data at {}: {e}", dir.display());
            }
        }

        Ok(())
    }

    // =========================================================================
    // Discogs token management
    // =========================================================================

    pub fn has_discogs_token(&self) -> bool {
        self.config_handle.has_discogs_key()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, String> {
        self.key_service
            .get_discogs_key()
            .map_err(|e| e.to_string())
    }

    pub fn save_discogs_key(&self, token: &str) -> Result<(), String> {
        self.key_service
            .set_discogs_key(token)
            .map_err(|e| e.to_string())
    }

    pub fn delete_discogs_key(&self) -> Result<(), String> {
        self.key_service
            .delete_discogs_key()
            .map_err(|e| e.to_string())
    }

    /// Record a stored key with its validation state — the single write for the
    /// save path. `Some(validation)` is both the stored-key hint and the state,
    /// so one `update` keeps them consistent and fires one watch-channel
    /// notification.
    pub fn set_discogs_key_stored(
        &self,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.discogs = Some(validation))
    }

    /// Clear the stored-key state — no key, so no validation.
    pub fn clear_discogs_key_stored(&self) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.discogs = None)
    }

    /// Update the stored key's validation state. No-op when no key is stored —
    /// validation describes a key that exists.
    pub fn set_discogs_validation(
        &self,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| {
            if c.discogs.is_some() {
                c.discogs = Some(validation);
            }
        })
    }

    pub fn discogs_validation(&self) -> Option<crate::config::DiscogsValidation> {
        self.config_handle.config().discogs
    }

    /// An observer that folds a Discogs call's outcome into the stored key's
    /// validation, so every call site updates the stored validation state
    /// without recording the outcome itself. A 401 marks it `Rejected`; a
    /// success while it was `Unvalidated` confirms it `Valid`; any other outcome
    /// (network, rate-limit, success while already settled, or no key stored)
    /// leaves it untouched. Injected into the client by [`Self::discogs_client`].
    pub(crate) fn discogs_validation_observer(
        &self,
    ) -> crate::discogs::client::DiscogsValidationObserver {
        use crate::config::DiscogsValidation;
        use crate::discogs::client::DiscogsKeySignal;
        let config_handle = self.config_handle.clone();
        std::sync::Arc::new(move |signal| {
            let Some(current) = config_handle.config().discogs else {
                // The key was removed while a Discogs call was in flight: the
                // client outlives the config entry it was built from. Nothing to
                // fold the outcome into.
                tracing::debug!("discogs validation signal ignored: no key stored");
                return;
            };
            let next = match signal {
                DiscogsKeySignal::Rejected => DiscogsValidation::Rejected,
                DiscogsKeySignal::Accepted if current == DiscogsValidation::Unvalidated => {
                    DiscogsValidation::Valid
                }
                _ => return,
            };
            if current == next {
                return;
            }
            if let Err(e) = config_handle.update(|c| c.discogs = Some(next)) {
                tracing::warn!("failed to persist discogs validation {next:?}: {e}");
            }
        })
    }

    /// A client for the stored key, unless that key is `Rejected`. A `Valid` or
    /// `Unvalidated` key is served (the latter used optimistically); a
    /// `Rejected` key is withheld so search call sites skip Discogs entirely.
    /// The client reports each call's outcome back into the validation state.
    pub fn discogs_client(&self) -> Result<Option<crate::discogs::DiscogsClient>, String> {
        if self.discogs_validation() == Some(crate::config::DiscogsValidation::Rejected) {
            return Ok(None);
        }
        let observer = self.discogs_validation_observer();
        Ok(self
            .key_service
            .get_discogs_key()
            .map_err(|e| e.to_string())?
            .map(|key| crate::discogs::DiscogsClient::with_observer(key, observer)))
    }
}
