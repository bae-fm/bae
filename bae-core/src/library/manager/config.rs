//! Config access and Discogs token management for [`LibraryManager`].
//!
//! Reads and writes config fields only. Library-lifecycle operations that
//! mutate the on-disk library presence (rename, lock, forget) live in
//! `lifecycle.rs`.

use super::*;

impl LibraryManager {
    pub fn get_config(&self) -> crate::config::Config {
        self.config_handle.config().clone()
    }

    /// Subscribe to the config-state stream; each change yields the whole latest
    /// `Config`.
    pub fn subscribe_config_changes(&self) -> tokio::sync::watch::Receiver<crate::config::Config> {
        self.config_handle.subscribe()
    }

    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.pause_between_sides = enabled)
    }

    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. No playback side effect — unlike
    /// `pause_between_sides`, nothing is staged on it — so the write is the whole
    /// operation; the resulting config invalidation re-renders the bar.
    pub fn set_show_remaining_time(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.show_remaining_time = enabled)
    }

    /// Where release exports write: prompt each time, or a fixed default folder.
    pub fn export_location(&self) -> crate::config::ExportLocation {
        self.config_handle.config().export_location.clone()
    }

    pub fn set_export_location(
        &self,
        location: crate::config::ExportLocation,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.export_location = location)
    }

    /// The template rendering a single-track export's suggested filename.
    pub fn export_filename_template(&self) -> String {
        self.config_handle.config().export_filename_template.clone()
    }

    pub fn set_export_filename_template(
        &self,
        template: String,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.export_filename_template = template)
    }

    pub fn export_presets(&self) -> Vec<crate::config::ExportPreset> {
        self.config_handle.config().export_presets.clone()
    }

    pub fn set_export_presets(
        &self,
        presets: Vec<crate::config::ExportPreset>,
    ) -> Result<(), crate::config::ConfigError> {
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            preset.validate()?;
            if !ids.insert(preset.id.clone()) {
                return Err(crate::config::ConfigError::Config(format!(
                    "duplicate export preset id {}",
                    preset.id
                )));
            }
        }
        let (default_track_export_selection, default_release_export_selection) = {
            let config = self.config_handle.config();
            (
                config.default_track_export_selection.clone(),
                config.default_release_export_selection.clone(),
            )
        };
        Self::validate_export_selection_against_presets(
            &default_track_export_selection,
            &presets,
            true,
        )?;
        Self::validate_export_selection_against_presets(
            &default_release_export_selection,
            &presets,
            false,
        )?;
        self.config_handle.update(|c| c.export_presets = presets)
    }

    pub fn set_default_track_export_selection(
        &self,
        selection: crate::config::ExportSelection,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_export_selection_against_presets(
            &selection,
            &self.config_handle.config().export_presets,
            true,
        )?;
        self.config_handle
            .update(|c| c.default_track_export_selection = selection)
    }

    pub fn set_default_release_export_selection(
        &self,
        selection: crate::config::ExportSelection,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_export_selection_against_presets(
            &selection,
            &self.config_handle.config().export_presets,
            false,
        )?;
        self.config_handle
            .update(|c| c.default_release_export_selection = selection)
    }

    fn validate_export_selection_against_presets(
        selection: &crate::config::ExportSelection,
        presets: &[crate::config::ExportPreset],
        track_level: bool,
    ) -> Result<(), crate::config::ConfigError> {
        let crate::config::ExportSelection::Preset { preset_id } = selection else {
            return Ok(());
        };
        let Some(preset) = presets.iter().find(|preset| preset.id == *preset_id) else {
            return Err(crate::config::ConfigError::Config(format!(
                "unknown export preset {}",
                preset_id
            )));
        };
        let allowed = if track_level {
            preset.applies_to_track
        } else {
            preset.applies_to_release
        };
        if allowed {
            Ok(())
        } else {
            Err(crate::config::ConfigError::Config(format!(
                "export preset {} does not apply to this export level",
                preset_id
            )))
        }
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

    pub fn has_discogs_token(&self) -> bool {
        self.config_handle.has_discogs_key()
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.key_service.get_discogs_key()?)
    }

    /// Store the Discogs key across both durable stores — keyring bytes, then
    /// config hint — as one operation. Keyring first is deliberate: a failure or
    /// crash after the keyring write but before the config write leaves the bytes
    /// present with no config hint, which [`Self::discogs_client`] reads as "not
    /// configured", so Discogs stays fully off until a retry completes both. The
    /// reverse order could leave config claiming a usable key the keyring lacks —
    /// the one torn state that offers Discogs in the UI while search drops it.
    ///
    /// On config-write failure the keyring bytes are left in place (not rolled
    /// back): both stores then show a key exists, the client re-validates it on
    /// use, and the returned error tells the caller to retry. Deleting the bytes
    /// instead would manufacture the dangerous config-present/keyring-empty state.
    pub fn set_discogs_key(
        &self,
        token: &str,
        validation: crate::config::DiscogsValidation,
    ) -> Result<(), LibraryError> {
        self.key_service.set_discogs_key(token)?;
        self.config_handle
            .update(|c| c.discogs = Some(validation))?;
        Ok(())
    }

    /// Remove the Discogs key from both durable stores — config hint, then
    /// keyring bytes — as one operation. Config first is deliberate: a crash
    /// between leaves no config hint with orphaned keyring bytes, read as "not
    /// configured" and cleaned up by an idempotent retry. Deleting the keyring
    /// first would leave config claiming a key the keyring no longer has — the
    /// dangerous torn-remove state.
    pub fn clear_discogs_key(&self) -> Result<(), LibraryError> {
        self.config_handle.update(|c| c.discogs = None)?;
        self.key_service.delete_discogs_key()?;
        Ok(())
    }

    pub fn get_mcp_token(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.key_service.get_mcp_token()?)
    }

    pub fn ensure_mcp_token(&self) -> Result<String, LibraryError> {
        match self.get_mcp_token()? {
            Some(token) => Ok(token),
            None => {
                let token = super::generate_mcp_token();
                self.set_mcp_token(token.clone())?;
                Ok(token)
            }
        }
    }

    pub fn set_mcp_token(&self, token: String) -> Result<(), LibraryError> {
        Ok(self.key_service.set_mcp_token(&token)?)
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

    /// Folds a Discogs call's outcome into the stored key's validation, so no call
    /// site has to record it. A 401 marks the key `Rejected`; a success while it was
    /// `Unvalidated` confirms it `Valid`; anything else leaves it alone.
    pub(crate) fn discogs_validation_observer(
        &self,
    ) -> crate::discogs::client::DiscogsValidationObserver {
        use crate::config::DiscogsValidation;
        use crate::discogs::client::DiscogsKeySignal;
        let config_handle = self.config_handle.clone();
        std::sync::Arc::new(move |signal| {
            let Some(current) = config_handle.config().discogs else {
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

    /// A client for the stored key, or `None` when Discogs can't be used — which
    /// withholds it from every search call site. `None` when no key is configured,
    /// or the key is `Rejected`; an `Unvalidated` key is served optimistically. The
    /// client reports each call's outcome back into the validation state.
    ///
    /// A usable key requires *both* stores to agree it exists: the config hint here
    /// and the keyring bytes below. A torn write (or external keyring tampering)
    /// that leaves one without the other reads as "not configured" rather than
    /// serving — or claiming to serve — a half-present key.
    pub fn discogs_client(&self) -> Result<Option<crate::discogs::DiscogsClient>, LibraryError> {
        use crate::config::DiscogsValidation;
        match self.discogs_validation() {
            None | Some(DiscogsValidation::Rejected) => return Ok(None),
            Some(DiscogsValidation::Valid | DiscogsValidation::Unvalidated) => {}
        }
        let observer = self.discogs_validation_observer();
        Ok(self
            .key_service
            .get_discogs_key()?
            .map(|key| crate::discogs::DiscogsClient::with_observer(key, observer)))
    }
}
