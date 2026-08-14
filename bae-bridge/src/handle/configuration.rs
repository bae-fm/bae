use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig::from_core(&self.services.get_config())
    }

    pub fn set_pause_between_sides(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_pause_between_sides(enabled)
            .map_err(BridgeError::config)
    }

    pub fn set_max_concurrent_uploads(&self, n: u32) -> Result<(), BridgeError> {
        self.services
            .set_max_concurrent_uploads(n)
            .map_err(BridgeError::config)
    }

    pub fn set_max_concurrent_downloads(&self, n: u32) -> Result<(), BridgeError> {
        self.services
            .set_max_concurrent_downloads(n)
            .map_err(BridgeError::config)
    }

    pub fn set_show_remaining_time(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_show_remaining_time(enabled)
            .map_err(BridgeError::config)
    }

    pub fn set_library_full_width(&self, enabled: bool) -> Result<(), BridgeError> {
        self.services
            .set_library_full_width(enabled)
            .map_err(BridgeError::config)
    }

    pub fn set_save_presets(
        &self,
        presets: Vec<crate::types::BridgeSavePreset>,
    ) -> Result<(), BridgeError> {
        self.services
            .set_save_presets(
                presets
                    .into_iter()
                    .map(crate::types::BridgeSavePreset::into_core)
                    .collect(),
            )
            .map_err(BridgeError::config)
    }

    pub fn set_default_track_save_preset(&self, preset_id: String) -> Result<(), BridgeError> {
        self.services
            .set_default_track_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    pub fn set_default_release_save_preset(&self, preset_id: String) -> Result<(), BridgeError> {
        self.services
            .set_default_release_save_preset(preset_id)
            .map_err(BridgeError::config)
    }

    pub fn has_encryption_key(&self) -> bool {
        self.services.has_encryption()
    }

    pub fn rename_library(&self, library_id: String, name: String) -> Result<(), BridgeError> {
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|error| BridgeError::config(error.to_string()))?;
        self.services.rename_library(&library_id, &name)?;
        Ok(())
    }

    pub fn lock_active_library(&self) -> Result<(), BridgeError> {
        self.services.forget_encryption_key()?;
        Ok(())
    }

    pub fn get_discogs_token(&self) -> Result<Option<String>, BridgeError> {
        Ok(self.services.get_discogs_token()?)
    }
}
