use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig::from_core(&self.services.get_config())
    }

    pub fn rename_library(&self, library_id: String, name: String) -> Result<(), BridgeError> {
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|error| BridgeError::config(error.to_string()))?;
        self.services.rename_library(&library_id, &name)?;
        Ok(())
    }
}

forward! { sync this => {
    fn set_pause_between_sides(enabled: bool) -> Result<(), BridgeError> {
        Ok(this.services.set_pause_between_sides(enabled)?)
    }

    fn set_max_concurrent_uploads(n: u32) -> Result<(), BridgeError> {
        Ok(this.services.set_max_concurrent_uploads(n)?)
    }

    fn set_max_concurrent_downloads(n: u32) -> Result<(), BridgeError> {
        Ok(this.services.set_max_concurrent_downloads(n)?)
    }

    fn set_identify_automatically(enabled: bool) -> Result<(), BridgeError> {
        Ok(this.services.set_identify_automatically(enabled)?)
    }

    /// Ask, or stop asking, one metadata source. Refused when it would leave
    /// nothing to ask — the error carries the sentence to show.
    fn set_metadata_source_enabled(
        source: crate::types::BridgeMetadataSource,
        enabled: bool,
    ) -> Result<(), BridgeError> {
        Ok(this
            .services
            .set_metadata_source_enabled(source.into_core(), enabled)?)
    }

    fn set_prefill_with_tags(enabled: bool) -> Result<(), BridgeError> {
        Ok(this.services.set_prefill_with_tags(enabled)?)
    }

    fn set_show_remaining_time(enabled: bool) -> Result<(), BridgeError> {
        Ok(this.services.set_show_remaining_time(enabled)?)
    }

    fn set_library_full_width(enabled: bool) -> Result<(), BridgeError> {
        Ok(this.services.set_library_full_width(enabled)?)
    }

    fn set_save_presets(
        presets: Vec<crate::types::BridgeSavePreset>,
    ) -> Result<(), BridgeError> {
        Ok(this.services.set_save_presets(
            presets
                .into_iter()
                .map(crate::types::BridgeSavePreset::into_core)
                .collect(),
        )?)
    }

    fn set_default_track_save_preset(preset_id: String) -> Result<(), BridgeError> {
        Ok(this.services.set_default_track_save_preset(preset_id)?)
    }

    fn set_default_release_save_preset(preset_id: String) -> Result<(), BridgeError> {
        Ok(this.services.set_default_release_save_preset(preset_id)?)
    }

    fn cloud_home_key_state() -> Result<BridgeCloudHomeKeyState, BridgeError> {
        Ok(this.services.cloud_home_key_state()?.into())
    }
} }

forward! { async this => {
    fn lock_active_library() -> () {
        this.services.forget_encryption_key().await?;
        Ok(())
    }

    fn unlock_cloud_home(serialized_master_key: String) -> () {
        this.services
            .unlock_cloud_home(&serialized_master_key)
            .await?;
        Ok(())
    }
} }
