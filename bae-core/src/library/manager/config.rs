//! Config access and Discogs token management for [`LibraryManager`].
//!
//! Reads and writes config fields only. Library-lifecycle operations that
//! mutate the on-disk library presence (rename, lock, forget) live in
//! `lifecycle.rs`.

use super::*;

impl LibraryManager {
    /// Get a snapshot of the current config.
    pub fn get_config(&self) -> crate::config::Config {
        self.config_handle.config().clone()
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

    /// Where release exports write: prompt each time, or a fixed default folder.
    pub fn export_location(&self) -> crate::config::ExportLocation {
        self.config_handle.config().export_location.clone()
    }

    /// Set where release exports write. Persisted in the config file.
    pub fn set_export_location(
        &self,
        location: crate::config::ExportLocation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.export_location = location)
    }

    /// Set the local MCP server config. Port 0 means "ask the OS for any port",
    /// which would make the configured endpoint false, so reject it before
    /// persisting.
    pub fn set_mcp_config(
        &self,
        config: crate::config::McpConfig,
    ) -> Result<(), crate::config::ConfigError> {
        config.validate()?;
        self.config_handle.update(|c| c.mcp = config)
    }

    // =========================================================================
    // Discogs token management
    // =========================================================================

    pub fn has_discogs_token(&self) -> bool {
        self.discogs.has_discogs_token()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, String> {
        self.discogs.get_discogs_token()
    }

    pub fn save_discogs_key(&self, token: &str) -> Result<(), String> {
        self.discogs.save_discogs_key(token)
    }

    pub fn delete_discogs_key(&self) -> Result<(), String> {
        self.discogs.delete_discogs_key()
    }

    // =========================================================================
    // MCP token management
    // =========================================================================

    pub fn get_mcp_token(&self) -> Result<Option<String>, String> {
        self.key_service.get_mcp_token().map_err(|e| e.to_string())
    }

    pub fn ensure_mcp_token(&self) -> Result<String, String> {
        match self.get_mcp_token()? {
            Some(token) => Ok(token),
            None => {
                let token = super::generate_mcp_token();
                self.set_mcp_token(token.clone())?;
                Ok(token)
            }
        }
    }

    pub fn set_mcp_token(&self, token: String) -> Result<(), String> {
        self.key_service
            .set_mcp_token(&token)
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
        self.discogs.set_discogs_key_stored(validation)
    }

    /// Clear the stored-key state — no key, so no validation.
    pub fn clear_discogs_key_stored(&self) -> Result<(), crate::config::ConfigError> {
        self.discogs.clear_discogs_key_stored()
    }

    /// Update the stored key's validation state. No-op when no key is stored —
    /// validation describes a key that exists.
    pub fn set_discogs_validation(
        &self,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), crate::config::ConfigError> {
        self.discogs.set_discogs_validation(validation)
    }

    pub fn discogs_validation(&self) -> Option<crate::config::DiscogsValidation> {
        self.discogs.discogs_validation()
    }

    /// A client for the stored key, unless that key is `Rejected`. A `Valid` or
    /// `Unvalidated` key is served (the latter used optimistically); a
    /// `Rejected` key is withheld so search call sites skip Discogs entirely.
    /// The client reports each call's outcome back into the validation state.
    pub fn discogs_client(&self) -> Result<Option<crate::discogs::DiscogsClient>, String> {
        self.discogs.discogs_client()
    }
}
