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

    /// How many blob uploads coven's upload drain runs at once. Rejected outside
    /// 1..=[`MAX_CONCURRENT_TRANSFERS`](crate::config::MAX_CONCURRENT_TRANSFERS):
    /// zero would leave the drain admitting nothing. Durable in the config and
    /// applied to the open store at once: the next drain pass runs under it.
    pub fn set_max_concurrent_uploads(&self, n: u32) -> Result<(), crate::config::ConfigError> {
        let n = crate::config::validate_concurrency(n)?;
        self.config_handle
            .update(|c| c.max_concurrent_uploads = n)?;
        self.apply_transfer_limits();
        Ok(())
    }

    /// How many blob downloads a pin fetches at once. Same bounds and
    /// application as [`Self::set_max_concurrent_uploads`].
    pub fn set_max_concurrent_downloads(&self, n: u32) -> Result<(), crate::config::ConfigError> {
        let n = crate::config::validate_concurrency(n)?;
        self.config_handle
            .update(|c| c.max_concurrent_downloads = n)?;
        self.apply_transfer_limits();
        Ok(())
    }

    /// Hand the stored transfer limits to the open store. The builder reads
    /// them at open; this is what a change after open does.
    fn apply_transfer_limits(&self) {
        let config = self.config_handle.config();
        self.database.set_transfer_limits(coven::TransferLimits {
            uploads: crate::config::usize_bound(config.max_concurrent_uploads),
            downloads: crate::config::usize_bound(config.max_concurrent_downloads),
        });
    }

    /// The limits the open store runs under — what a change through the
    /// setters above has taken effect as.
    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.database.transfer_limits()
    }

    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. No playback side effect — unlike
    /// `pause_between_sides`, nothing is staged on it — so the write is the whole
    /// operation; the config value stream re-renders the bar.
    pub fn set_show_remaining_time(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.show_remaining_time = enabled)
    }

    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. Pure display
    /// preference: the config value stream re-renders the page after the write.
    pub fn set_library_full_width(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|c| c.library_full_width = enabled)
    }

    pub fn set_identify_automatically(
        &self,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|config| config.identify_automatically = enabled)
    }

    pub fn set_default_import_metadata_source(
        &self,
        source: crate::config::DefaultImportMetadataSource,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update(|config| config.default_import_metadata_source = source)
    }

    /// Whether casting to a network receiver is available. Turning it off is
    /// what ends an active session: the desktop cast controller follows this
    /// field, stops browsing, and disconnects.
    pub fn set_cast_enabled(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle.update(|c| c.cast_enabled = enabled)
    }

    pub fn save_presets(&self) -> Vec<crate::config::SavePreset> {
        self.config_handle.config().save_presets.clone()
    }

    pub fn set_save_presets(
        &self,
        presets: Vec<crate::config::SavePreset>,
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
        // Both save defaults are required, valid preset ids: re-validate them
        // against the new list so deleting a default's preset (or the last
        // preset applicable to a level) is rejected rather than leaving a
        // dangling default.
        let (default_track, default_release) = {
            let config = self.config_handle.config();
            (
                config.default_track_save_preset.clone(),
                config.default_release_save_preset.clone(),
            )
        };
        Self::validate_default_save_preset(&default_track, &presets, true)?;
        Self::validate_default_save_preset(&default_release, &presets, false)?;
        self.config_handle.update(|c| c.save_presets = presets)
    }

    pub fn set_default_track_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_default_save_preset(
            &preset_id,
            &self.config_handle.config().save_presets,
            true,
        )?;
        self.config_handle
            .update(|c| c.default_track_save_preset = preset_id)
    }

    pub fn set_default_release_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_default_save_preset(
            &preset_id,
            &self.config_handle.config().save_presets,
            false,
        )?;
        self.config_handle
            .update(|c| c.default_release_save_preset = preset_id)
    }

    /// A save default must name a preset that exists and applies to its level
    /// (track or release). Rejects an unknown id or one whose preset doesn't
    /// cover the level, so a stored default is never dangling or wrong-level.
    fn validate_default_save_preset(
        preset_id: &str,
        presets: &[crate::config::SavePreset],
        track_level: bool,
    ) -> Result<(), crate::config::ConfigError> {
        let Some(preset) = presets.iter().find(|preset| preset.id == *preset_id) else {
            return Err(crate::config::ConfigError::Config(format!(
                "unknown export preset {preset_id}"
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
                "export preset {preset_id} does not apply to this export level"
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

    pub fn get_mcp_token(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.database.host_secret(crate::keys::MCP_BEARER_TOKEN)?)
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
        Ok(self
            .database
            .set_host_secret(crate::keys::MCP_BEARER_TOKEN, &token)?)
    }

    /// Set the Subsonic server config. Rejects `port == 0` (no real endpoint)
    /// and an enabled server with no username (it could authenticate no one)
    /// before persisting.
    pub fn set_subsonic_config(
        &self,
        config: crate::config::SubsonicConfig,
    ) -> Result<(), crate::config::ConfigError> {
        config.validate()?;
        self.config_handle.update(|c| c.subsonic = config)
    }

    pub fn get_subsonic_password(&self) -> Result<Option<String>, LibraryError> {
        Ok(self.database.host_secret(crate::keys::SUBSONIC_PASSWORD)?)
    }

    pub fn set_subsonic_password(&self, password: String) -> Result<(), LibraryError> {
        Ok(self
            .database
            .set_host_secret(crate::keys::SUBSONIC_PASSWORD, &password)?)
    }
}
