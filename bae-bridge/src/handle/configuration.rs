use super::*;

#[uniffi::export]
impl AppHandle {
    /// This device's preferences for the library, as the settings screens
    /// show them: the first value at once, then one after each write that
    /// changes them.
    pub fn subscribe_config(&self) -> std::sync::Arc<crate::ConfigSubscription> {
        crate::screens::config(self.services.subscribe_config(), self.runtime.clone())
    }
}

// Every setter writes the preferences row, so each is an async call the host
// awaits off its own thread.
forward! { async this => {
    fn rename_library(name: String) -> () {
        let name = crate::from_app::name(&name)?;
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

    /// Where an import goes: this device or the cloud. One choice, shared by
    /// every place that offers it.
    fn set_import_destination(destination: crate::types::BridgeImportDestination) -> () {
        Ok(this.services.set_import_destination(destination.into_core()).await?)
    }

    /// Ask, or stop asking, one metadata source. Refused when it would leave
    /// nothing to ask — the error carries the sentence to show.
    fn set_metadata_source_enabled(source: crate::types::BridgeLookupCatalog, enabled: bool) -> () {
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

    fn set_save_embed_cover(on: bool) -> () {
        Ok(this.services.set_save_embed_cover(on).await?)
    }

    fn set_save_cover_file(on: bool) -> () {
        Ok(this.services.set_save_cover_file(on).await?)
    }

    fn set_save_loudness_tags(on: bool) -> () {
        Ok(this.services.set_save_loudness_tags(on).await?)
    }

    fn set_folder_pieces(pieces: Vec<crate::types::BridgeSaveFolderPiece>) -> () {
        Ok(this
            .services
            .set_save_folder_pattern(bae_preferences::NamePattern::new(
                pieces.into_iter().map(crate::types::BridgeSaveFolderPiece::into_core).collect(),
            ))
            .await?)
    }

    fn set_filename_pieces(pieces: Vec<crate::types::BridgeSaveFilenamePiece>) -> () {
        Ok(this
            .services
            .set_save_filename_pattern(bae_preferences::NamePattern::new(
                pieces
                    .into_iter()
                    .map(crate::types::BridgeSaveFilenamePiece::into_core)
                    .collect(),
            ))
            .await?)
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

#[cfg(test)]
#[path = "config_subscription_tests.rs"]
mod config_subscription_tests;
