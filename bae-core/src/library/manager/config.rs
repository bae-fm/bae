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

    pub fn save_discogs_key(&self, token: &str) -> Result<(), LibraryError> {
        Ok(self.key_service.set_discogs_key(token)?)
    }

    pub fn delete_discogs_key(&self) -> Result<(), LibraryError> {
        Ok(self.key_service.delete_discogs_key()?)
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

    /// Record a stored key with its validation state. `Some(validation)` is both
    /// the "a key exists" hint and the state, so one `update` keeps the two
    /// consistent and fires a single watch-channel notification.
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

    /// A client for the stored key, unless that key is `Rejected` — withholding it
    /// is what makes search call sites skip Discogs entirely. An `Unvalidated` key is
    /// served optimistically. The client reports each call's outcome back into the
    /// validation state.
    pub fn discogs_client(&self) -> Result<Option<crate::discogs::DiscogsClient>, LibraryError> {
        if self.discogs_validation() == Some(crate::config::DiscogsValidation::Rejected) {
            return Ok(None);
        }
        let observer = self.discogs_validation_observer();
        Ok(self
            .key_service
            .get_discogs_key()?
            .map(|key| crate::discogs::DiscogsClient::with_observer(key, observer)))
    }
}
