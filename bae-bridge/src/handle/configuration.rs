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
    fn rename_library(name: String) -> () {
        let name = bae_core::library_name::LibraryName::parse(&name)
            .map_err(|error| BridgeError::config(error.to_string()))?;
        this.services.rename_library(&name).await?;
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

    /// Import what an identification run settles on as needing nothing, or
    /// stop — any run, started on its own or by a person. On, only runs that
    /// settle from now on import; off, an import already started runs to its
    /// end.
    fn set_import_when_identified(enabled: bool) -> () {
        Ok(this.services.set_import_when_identified(enabled).await?)
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
    fn store_key_state() -> Result<BridgeStoreKeyState, BridgeError> {
        Ok(this.services.store_key_state()?.into())
    }
} }

forward! { async this => {
    fn lock_active_library() -> () {
        this.services.forget_encryption_key().await?;
        Ok(())
    }

    fn unlock_store_key() -> () {
        this.services
            .unlock_store_key()
            .await?;
        Ok(())
    }
} }
