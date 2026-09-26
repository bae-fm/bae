//! Config access and Discogs token management for [`LibraryManager`].
//!
//! Reads and writes config fields only. Library-lifecycle operations that
//! mutate the on-disk library presence (rename, lock, forget) live in
//! `lifecycle.rs`.

use super::*;

/// A preference the UI writes and nothing else reacts to: the write *is* the
/// whole operation, and the config value stream re-renders whoever reads it.
/// Setters that also validate, or that hand the new value to something already
/// running, are written out below instead.
macro_rules! pref_setter {
    ($(#[$doc:meta])* $name:ident, $field:ident: $ty:ty) => {
        $(#[$doc])*
        pub async fn $name(&self, value: $ty) -> Result<(), crate::config::ConfigError> {
            self.config_handle
                .update_preferences(move |prefs| prefs.$field = value)
                .await
        }
    };
}

impl LibraryManager {
    pub fn get_config(&self) -> crate::config::Config {
        self.config_handle.config().clone()
    }

    /// Subscribe to the config-state stream; each change yields the whole latest
    /// `Config`.
    pub fn subscribe_config_changes(&self) -> tokio::sync::watch::Receiver<crate::config::Config> {
        self.config_handle.subscribe()
    }

    pref_setter!(set_pause_between_sides, pause_between_sides: bool);

    pref_setter!(
        /// Whether a side or disc pause ends on its own after a countdown, and
        /// how long. Read when playback pauses at a boundary, so a change applies
        /// from the next pause on.
        set_side_pause_countdown,
        side_pause_countdown: crate::config::SidePauseCountdown
    );

    pref_setter!(
        /// Whether the seek bar's leading label counts down the time remaining
        /// instead of showing the time elapsed.
        set_show_remaining_time,
        show_remaining_time: bool
    );

    pref_setter!(
        /// Whether the library page spans the window's full width instead of
        /// centering its content in a width-capped column.
        set_library_full_width,
        library_full_width: bool
    );

    /// Whether identification starts on its own. The identification queue
    /// follows the value: on, it admits what has no answer; off, it admits
    /// nothing new, and nothing it identifies is imported on its own.
    pub async fn set_identify_automatically(
        &self,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| prefs.identification.automatic = enabled)
            .await
    }

    /// Whether what an automatic run settles on as needing nothing is imported
    /// straight away. Turning it on imports nothing identified before: only a
    /// run that settles while it is on owes an import. Turning it off
    /// withdraws what was owed and not yet started; an import already running
    /// runs to its end.
    pub async fn set_import_when_identified(
        &self,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| prefs.identification.import_when_identified = enabled)
            .await
    }

    /// Take, or stop taking, one step of every identification run. A run
    /// reads its steps as it starts, so the change applies from the next run
    /// on and a run in flight finishes the way it began.
    pub async fn set_identification_step(
        &self,
        step: crate::config::IdentificationStep,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| prefs.identification.steps.set(step, enabled))
            .await
    }

    /// Whether an import goes to the cloud home, when the library has one —
    /// the choice an import pane last made, and what an automatic import
    /// goes by.
    pub async fn set_import_to_cloud(
        &self,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| prefs.import_storage.cloud = enabled)
            .await
    }

    /// Whether a release that goes to the cloud also stays downloaded here.
    pub async fn set_import_pinned(&self, enabled: bool) -> Result<(), crate::config::ConfigError> {
        self.config_handle
            .update_preferences(move |prefs| prefs.import_storage.pinned = enabled)
            .await
    }

    /// The steps an identification run takes, as a run starting now reads
    /// them.
    pub fn identification_steps(&self) -> crate::config::IdentificationSteps {
        self.config_handle.config().prefs.identification.steps
    }

    /// Which metadata sources this library asks, one entry per
    /// [`Catalog`](crate::import::Catalog). See
    /// [`Config::metadata_sources`](crate::config::Config::metadata_sources) —
    /// the answer is a fact about the stored config, read here off the current
    /// one.
    pub fn metadata_sources(&self) -> Vec<crate::import::CatalogAvailability> {
        self.config_handle.config().metadata_sources()
    }

    /// Ask, or stop asking, one metadata source. The switch behind every place
    /// the sources are asked together.
    ///
    /// Refused when switching `source` off would leave nothing to ask: a
    /// library that asks no source cannot look anything up, and the run that
    /// would report so has nothing to report about. The surface disables the
    /// last remaining source's switch, so this is the backstop for two writes
    /// racing, not the path a person takes.
    ///
    /// Writes the preference only. Re-laying live runs and searches over the
    /// new list is [`AppServices::set_metadata_source_enabled`](crate::library::AppServices::set_metadata_source_enabled)'s
    /// job — this layer owns config and knows nothing about what is running.
    pub async fn set_metadata_source_enabled(
        &self,
        source: crate::import::Catalog,
        enabled: bool,
    ) -> Result<(), crate::config::ConfigError> {
        if !enabled && crate::import::is_the_only_asked_source(&self.metadata_sources(), source) {
            return Err(crate::config::ConfigError::Config(format!(
                "{} is the only source left to search",
                source.display_name()
            )));
        }
        self.config_handle
            .update_preferences(move |prefs| prefs.identification.catalogs.set(source, enabled))
            .await
    }

    pref_setter!(
        /// Whether a candidate's draft is created from the folder's own metadata.
        set_prefill_with_file_metadata,
        prefill_with_file_metadata: bool
    );

    pref_setter!(
        /// Whether casting to a network receiver is available. Turning it off
        /// is what ends an active session: the desktop cast controller follows
        /// this field, stops browsing, and disconnects.
        set_cast_enabled,
        cast_enabled: bool
    );

    /// How many blob uploads coven's upload drain runs at once. Rejected outside
    /// 1..=[`MAX_CONCURRENT_TRANSFERS`](crate::config::MAX_CONCURRENT_TRANSFERS):
    /// zero would leave the drain admitting nothing. Durable in the config and
    /// applied to the open store at once: the next drain pass runs under it.
    pub async fn set_max_concurrent_uploads(
        &self,
        n: u32,
    ) -> Result<(), crate::config::ConfigError> {
        let n = crate::config::validate_concurrency(n)?;
        self.config_handle
            .update_preferences(move |prefs| prefs.max_concurrent_uploads = n)
            .await?;
        self.apply_transfer_limits();
        Ok(())
    }

    /// How many blob downloads a pin fetches at once. Same bounds and
    /// application as [`Self::set_max_concurrent_uploads`].
    pub async fn set_max_concurrent_downloads(
        &self,
        n: u32,
    ) -> Result<(), crate::config::ConfigError> {
        let n = crate::config::validate_concurrency(n)?;
        self.config_handle
            .update_preferences(move |prefs| prefs.max_concurrent_downloads = n)
            .await?;
        self.apply_transfer_limits();
        Ok(())
    }

    /// Hand the stored transfer limits to the open store. The builder reads
    /// them at open; this is what a change after open does.
    fn apply_transfer_limits(&self) {
        let config = self.config_handle.config();
        self.database.set_transfer_limits(coven::TransferLimits {
            uploads: crate::config::usize_bound(config.prefs.max_concurrent_uploads),
            downloads: crate::config::usize_bound(config.prefs.max_concurrent_downloads),
        });
    }

    /// The limits the open store runs under — what a change through the
    /// setters above has taken effect as.
    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.database.transfer_limits()
    }

    pub fn save_presets(&self) -> Vec<crate::config::SavePreset> {
        self.config_handle.config().prefs.save_presets.clone()
    }

    pub async fn set_save_presets(
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
                config.prefs.default_track_save_preset.clone(),
                config.prefs.default_release_save_preset.clone(),
            )
        };
        Self::validate_default_save_preset(&default_track, &presets, true)?;
        Self::validate_default_save_preset(&default_release, &presets, false)?;
        self.config_handle
            .update_preferences(move |prefs| prefs.save_presets = presets)
            .await
    }

    pub async fn set_default_track_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_default_save_preset(
            &preset_id,
            &self.config_handle.config().prefs.save_presets,
            true,
        )?;
        self.config_handle
            .update_preferences(move |prefs| prefs.default_track_save_preset = preset_id)
            .await
    }

    pub async fn set_default_release_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), crate::config::ConfigError> {
        Self::validate_default_save_preset(
            &preset_id,
            &self.config_handle.config().prefs.save_presets,
            false,
        )?;
        self.config_handle
            .update_preferences(move |prefs| prefs.default_release_save_preset = preset_id)
            .await
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
    pub async fn set_mcp_config(
        &self,
        config: crate::config::McpConfig,
    ) -> Result<(), crate::config::ConfigError> {
        config.validate()?;
        self.config_handle
            .update_preferences(move |prefs| prefs.mcp = config)
            .await
    }

    /// The MCP bearer token, read from the keychain on a blocking thread.
    pub async fn get_mcp_token(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::MCP_BEARER_TOKEN).await
    }

    /// The MCP bearer token, generated and stored the first time it is asked
    /// for.
    pub async fn ensure_mcp_token(&self) -> Result<String, LibraryError> {
        match self.get_mcp_token().await? {
            Some(token) => Ok(token),
            None => {
                let token = super::generate_mcp_token();
                self.set_mcp_token(token.clone()).await?;
                Ok(token)
            }
        }
    }

    pub async fn set_mcp_token(&self, token: String) -> Result<(), LibraryError> {
        self.set_host_secret(crate::keys::MCP_BEARER_TOKEN, token)
            .await
    }

    /// Set the Subsonic server config. Rejects `port == 0` (no real endpoint)
    /// and an enabled server with no username (it could authenticate no one)
    /// before persisting.
    pub async fn set_subsonic_config(
        &self,
        config: crate::config::SubsonicConfig,
    ) -> Result<(), crate::config::ConfigError> {
        config.validate()?;
        self.config_handle
            .update_preferences(move |prefs| prefs.subsonic = config)
            .await
    }

    pub async fn get_subsonic_password(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::SUBSONIC_PASSWORD).await
    }

    pub async fn set_subsonic_password(&self, password: String) -> Result<(), LibraryError> {
        self.set_host_secret(crate::keys::SUBSONIC_PASSWORD, password)
            .await
    }

    /// A host secret, read from the keychain on a blocking thread: a keychain
    /// call can wait on the system, and on a prompt.
    pub(super) async fn host_secret(
        &self,
        name: &'static str,
    ) -> Result<Option<String>, LibraryError> {
        let database = self.database.clone();
        match tokio::task::spawn_blocking(move || database.host_secret(name)).await {
            Ok(secret) => Ok(secret?),
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }

    /// Store a host secret in the keychain on a blocking thread.
    async fn set_host_secret(&self, name: &'static str, value: String) -> Result<(), LibraryError> {
        let database = self.database.clone();
        match tokio::task::spawn_blocking(move || database.set_host_secret(name, &value)).await {
            Ok(stored) => Ok(stored?),
            Err(error) => std::panic::resume_unwind(error.into_panic()),
        }
    }
}
