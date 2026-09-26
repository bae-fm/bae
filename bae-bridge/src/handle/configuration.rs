use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn get_config(&self) -> BridgeConfig {
        BridgeConfig::from_core(&self.services.get_config())
    }
}

// Every setter persists the config with a durable file replace, so each is an
// async call the host awaits off its own thread; the write itself runs on a
// blocking thread in core.
forward! { async this => {
    fn rename_library(library_id: String, name: String) -> () {
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|error| BridgeError::config(error.to_string()))?;
        this.services.rename_library(&library_id, &name).await?;
        Ok(())
    }

    fn set_pause_between_sides(enabled: bool) -> () {
        Ok(this.services.set_pause_between_sides(enabled).await?)
    }

    fn set_side_pause_countdown(countdown: crate::types::BridgeSidePauseCountdown) -> () {
        Ok(this
            .services
            .set_side_pause_countdown(countdown.into_core())
            .await?)
    }

    fn set_max_concurrent_uploads(n: u32) -> () {
        Ok(this.services.set_max_concurrent_uploads(n).await?)
    }

    fn set_max_concurrent_downloads(n: u32) -> () {
        Ok(this.services.set_max_concurrent_downloads(n).await?)
    }

    fn set_identify_automatically(enabled: bool) -> () {
        Ok(this.services.set_identify_automatically(enabled).await?)
    }

    /// Import what an automatic run settles on as needing nothing, or stop.
    /// On, only runs that settle from now on import; off, nothing owed and not
    /// yet started is imported.
    fn set_import_when_identified(enabled: bool) -> () {
        Ok(this.services.set_import_when_identified(enabled).await?)
    }

    /// Take, or stop taking, one step of every identification run. Runs
    /// already going finish the way they started.
    fn set_identification_step(step: crate::types::BridgeIdentificationStep, enabled: bool) -> () {
        Ok(this
            .services
            .set_identification_step(step.into_core(), enabled)
            .await?)
    }

    /// Whether an import goes to the cloud home, when the library has one.
    fn set_import_to_cloud(enabled: bool) -> () {
        Ok(this.services.set_import_to_cloud(enabled).await?)
    }

    /// Whether a release that goes to the cloud also stays downloaded here.
    fn set_import_pinned(enabled: bool) -> () {
        Ok(this.services.set_import_pinned(enabled).await?)
    }

    /// Ask, or stop asking, one metadata source. Refused when it would leave
    /// nothing to ask — the error carries the sentence to show.
    fn set_metadata_source_enabled(source: crate::types::BridgeCatalog, enabled: bool) -> () {
        Ok(this
            .services
            .set_metadata_source_enabled(source.into_core(), enabled)
            .await?)
    }

    fn set_prefill_with_file_metadata(enabled: bool) -> () {
        Ok(this.services.set_prefill_with_file_metadata(enabled).await?)
    }

    fn set_show_remaining_time(enabled: bool) -> () {
        Ok(this.services.set_show_remaining_time(enabled).await?)
    }

    fn set_library_full_width(enabled: bool) -> () {
        Ok(this.services.set_library_full_width(enabled).await?)
    }

    fn set_save_presets(presets: Vec<crate::types::BridgeSavePreset>) -> () {
        Ok(this
            .services
            .set_save_presets(
                presets
                    .into_iter()
                    .map(crate::types::BridgeSavePreset::into_core)
                    .collect(),
            )
            .await?)
    }

    fn set_default_track_save_preset(preset_id: String) -> () {
        Ok(this.services.set_default_track_save_preset(preset_id).await?)
    }

    fn set_default_release_save_preset(preset_id: String) -> () {
        Ok(this.services.set_default_release_save_preset(preset_id).await?)
    }
} }

forward! { sync this => {
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
