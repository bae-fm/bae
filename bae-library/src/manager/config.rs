//! This device's preferences for the library, read and written through its
//! `preferences` row, for [`LibraryManager`].
//!
//! Reads and writes preferences only. Library-lifecycle operations that
//! mutate the on-disk library presence (rename, lock, forget) live in
//! `lifecycle.rs`.

use super::*;

/// A preference the UI writes and nothing else reacts to: the write *is* the
/// whole operation, and the row's live queries re-render whoever reads it.
/// Setters that also validate, or that hand the new value to something already
/// running, are written out below instead.
macro_rules! pref_setter {
    ($(#[$doc:meta])* $name:ident, $field:ident: $ty:ty) => {
        $(#[$doc])*
        pub async fn $name(&self, value: $ty) -> Result<(), bae_preferences::PreferencesError> {
            self
                .update_preferences(move |prefs| {
                    prefs.$field = value;
                    Ok(())
                })
                .await
        }
    };
}

impl LibraryManager {
    /// Edit the preferences in one write of their row. `edit` sees what is
    /// stored and may refuse the change, which then writes nothing.
    pub(super) async fn update_preferences(
        &self,
        edit: impl FnOnce(
                &mut bae_preferences::Preferences,
            ) -> Result<(), bae_preferences::PreferencesError>
            + Send
            + 'static,
    ) -> Result<(), bae_preferences::PreferencesError> {
        bae_preferences::edit_preferences(&self.database, edit).await
    }

    /// This device's preferences for the library, as stored now.
    pub async fn preferences(&self) -> Result<bae_preferences::Preferences, bae_db::DbError> {
        bae_preferences::preferences(&self.database).await
    }

    /// The library this is, by its store id.
    pub fn store_id(&self) -> coven::StoreId {
        self.database.store_id()
    }

    /// bae's own directory for this library, apart from coven's store: the
    /// remote image cache and the like.
    pub fn library_path(&self) -> PathBuf {
        self.app_dir.registered_library(&self.database.store_id().to_string())
    }

    /// The library's preferences as the settings screens read them, with the
    /// library and its directory: the first value at once, then one after
    /// each write that changes them, until the surface cancels or the library
    /// closes.
    pub fn subscribe_config(&self) -> bae_db::live_query::LiveValues<bae_preferences::Config> {
        let store_id = self.database.store_id();
        let library_path = self.library_path();
        self.values_until_closed(bae_preferences::subscribe_preferences(&self.database).process(
            move |prefs| {
                Ok(bae_preferences::Config { store_id, library_path: library_path.clone(), prefs })
            },
        ))
    }

    /// The part of the preferences `part` takes, followed: the first value at
    /// once, then one after each write that changes that part, until the
    /// library closes.
    pub fn follow_preferences<T: Clone + PartialEq + Send + 'static>(
        &self,
        part: impl Fn(bae_preferences::Preferences) -> T + Send + Sync + 'static,
    ) -> bae_db::live_query::LiveValues<T> {
        self.values_until_closed(
            bae_preferences::subscribe_preferences(&self.database)
                .process(move |prefs| Ok(part(prefs))),
        )
    }

    pref_setter!(set_pause_between_sides, pause_between_sides: bool);

    pref_setter!(
        /// Whether a side or disc pause ends on its own after a countdown, and
        /// how long. Read when playback pauses at a boundary, so a change applies
        /// from the next pause on.
        set_side_pause_countdown,
        side_pause_countdown: bae_preferences::SidePauseCountdown
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

    /// Whether releases found from now on are identified on its own. It
    /// affects no candidate already found or queued.
    pub async fn set_identify_automatically(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.identification.automatic = enabled;
            Ok(())
        })
        .await
    }

    /// Whether what an identification run settles on as needing nothing is
    /// imported straight away, whether the run started on its own or a person
    /// asked for it. Read as each run settles: turning it on imports nothing
    /// identified before, and turning it off leaves an import already started
    /// to run to its end.
    pub async fn set_import_when_identified(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.identification.import_when_identified = enabled;
            Ok(())
        })
        .await
    }

    /// Whether imports go to the cloud home, when the library has one.
    pub async fn set_import_to_cloud(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.import_storage.cloud = enabled;
            Ok(())
        })
        .await
    }

    /// Whether a release that goes to the cloud also stays downloaded here.
    pub async fn set_import_pinned(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.import_storage.pinned = enabled;
            Ok(())
        })
        .await
    }

    /// Which metadata sources this library asks, one entry per
    /// [`Catalog`](bae_catalogs::catalog::Catalog). See
    /// [`Preferences::metadata_sources`](bae_preferences::Preferences::metadata_sources)
    /// — the answer is a fact about the stored preferences, read here off the
    /// current ones.
    pub async fn metadata_sources(
        &self,
    ) -> Result<Vec<bae_preferences::CatalogAvailability>, bae_db::DbError> {
        Ok(self.preferences().await?.metadata_sources())
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
    /// new list is the app's `AppServices::set_metadata_source_enabled`'s
    /// job — this layer owns config and knows nothing about what is running.
    pub async fn set_metadata_source_enabled(
        &self,
        source: bae_catalogs::catalog::LookupCatalog,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            if !enabled
                && bae_preferences::is_the_only_asked_source(&prefs.metadata_sources(), source)
            {
                return Err(bae_preferences::PreferencesError::Invalid(format!(
                    "{} is the only source left to search",
                    source.display_name()
                )));
            }
            prefs.identification.catalogs.set(source, enabled);
            Ok(())
        })
        .await
    }

    pref_setter!(
        /// Whether casting to a network receiver is available. Turning it off
        /// is what ends an active session: the desktop cast controller follows
        /// this field, stops browsing, and disconnects.
        set_cast_enabled,
        cast_enabled: bool
    );

    pref_setter!(
        /// What a track a Cast or UPnP device can't be served as stored is
        /// converted to. Read as each track is loaded onto the device, so a
        /// change applies from the next track.
        set_cast_transcode_format,
        cast_transcode_format: bae_preferences::CastTranscodeFormat
    );

    /// How many file uploads coven's upload drain runs at once. Rejected outside
    /// 1..=[`MAX_CONCURRENT_TRANSFERS`](bae_preferences::MAX_CONCURRENT_TRANSFERS):
    /// zero would leave the drain admitting nothing. Durable in the config and
    /// applied to the open store at once: the next drain pass runs under it.
    pub async fn set_max_concurrent_uploads(
        &self,
        n: u32,
    ) -> Result<(), bae_preferences::PreferencesError> {
        let n = bae_preferences::validate_concurrency(n)?;
        self.update_preferences(move |prefs| {
            prefs.max_concurrent_uploads = n;
            Ok(())
        })
        .await?;
        self.apply_transfer_limits().await
    }

    /// How many file downloads a pin fetches at once. Same bounds and
    /// application as [`Self::set_max_concurrent_uploads`].
    pub async fn set_max_concurrent_downloads(
        &self,
        n: u32,
    ) -> Result<(), bae_preferences::PreferencesError> {
        let n = bae_preferences::validate_concurrency(n)?;
        self.update_preferences(move |prefs| {
            prefs.max_concurrent_downloads = n;
            Ok(())
        })
        .await?;
        self.apply_transfer_limits().await
    }

    /// Hand the stored transfer limits to the open store, as opening it
    /// does; this is what a change after open does.
    async fn apply_transfer_limits(&self) -> Result<(), bae_preferences::PreferencesError> {
        self.database.set_transfer_limits(self.preferences().await?.transfer_limits());
        Ok(())
    }

    /// The limits the open store runs under — what a change through the
    /// setters above has taken effect as.
    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.database.transfer_limits()
    }

    pub async fn save_presets(&self) -> Result<Vec<bae_preferences::SavePreset>, bae_db::DbError> {
        Ok(self.preferences().await?.save_presets)
    }

    /// The save preset `preset_id` names, where it is offered for saves of
    /// `level`.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) async fn save_preset_for(
        &self,
        preset_id: &str,
        level: bae_preferences::SaveLevel,
    ) -> Result<bae_preferences::SavePreset, LibraryError> {
        self.save_presets()
            .await?
            .into_iter()
            .find(|preset| preset.id == preset_id && preset.applies_to(level))
            .ok_or_else(|| {
                LibraryError::Save(format!(
                    "save preset {preset_id} is not available for {level} save"
                ))
            })
    }

    pub async fn set_save_presets(
        &self,
        presets: Vec<bae_preferences::SavePreset>,
    ) -> Result<(), bae_preferences::PreferencesError> {
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            preset.validate()?;
            if !ids.insert(preset.id.clone()) {
                return Err(bae_preferences::PreferencesError::Invalid(format!(
                    "duplicate export preset id {}",
                    preset.id
                )));
            }
        }
        // Both save defaults are required, valid preset ids: re-validate the
        // stored ones against the new list so deleting a default's preset (or
        // the last preset applicable to a level) is refused rather than
        // leaving a dangling default.
        self.update_preferences(move |prefs| {
            Self::validate_default_save_preset(
                &prefs.default_track_save_preset,
                &presets,
                bae_preferences::SaveLevel::Track,
            )?;
            Self::validate_default_save_preset(
                &prefs.default_release_save_preset,
                &presets,
                bae_preferences::SaveLevel::Release,
            )?;
            prefs.save_presets = presets;
            Ok(())
        })
        .await
    }

    pub async fn set_default_track_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            Self::validate_default_save_preset(
                &preset_id,
                &prefs.save_presets,
                bae_preferences::SaveLevel::Track,
            )?;
            prefs.default_track_save_preset = preset_id;
            Ok(())
        })
        .await
    }

    pub async fn set_default_release_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            Self::validate_default_save_preset(
                &preset_id,
                &prefs.save_presets,
                bae_preferences::SaveLevel::Release,
            )?;
            prefs.default_release_save_preset = preset_id;
            Ok(())
        })
        .await
    }

    /// A save default must name a preset that exists and applies to its level
    /// (track or release). Rejects an unknown id or one whose preset doesn't
    /// cover the level, so a stored default is never dangling or wrong-level.
    fn validate_default_save_preset(
        preset_id: &str,
        presets: &[bae_preferences::SavePreset],
        level: bae_preferences::SaveLevel,
    ) -> Result<(), bae_preferences::PreferencesError> {
        let Some(preset) = presets.iter().find(|preset| preset.id == *preset_id) else {
            return Err(bae_preferences::PreferencesError::Invalid(format!(
                "unknown export preset {preset_id}"
            )));
        };
        if preset.applies_to(level) {
            Ok(())
        } else {
            Err(bae_preferences::PreferencesError::Invalid(format!(
                "export preset {preset_id} does not apply to {level} saves"
            )))
        }
    }

    /// Set the local MCP server config. Port 0 means "ask the OS for any port",
    /// which would make the configured endpoint false, so reject it before
    /// persisting.
    pub async fn set_mcp_config(
        &self,
        config: bae_preferences::McpConfig,
    ) -> Result<(), bae_preferences::PreferencesError> {
        config.validate()?;
        self.update_preferences(move |prefs| {
            prefs.mcp = config;
            Ok(())
        })
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
        self.set_host_secret(crate::keys::MCP_BEARER_TOKEN, token).await
    }

    /// Set the Subsonic server config. Rejects `port == 0` (no real endpoint)
    /// and an enabled server with no username (it could authenticate no one)
    /// before persisting.
    pub async fn set_subsonic_config(
        &self,
        config: bae_preferences::SubsonicConfig,
    ) -> Result<(), bae_preferences::PreferencesError> {
        config.validate()?;
        self.update_preferences(move |prefs| {
            prefs.subsonic = config;
            Ok(())
        })
        .await
    }

    pub async fn get_subsonic_password(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::SUBSONIC_PASSWORD).await
    }

    pub async fn set_subsonic_password(&self, password: String) -> Result<(), LibraryError> {
        self.set_host_secret(crate::keys::SUBSONIC_PASSWORD, password).await
    }

    /// A host secret, read from the keychain on a blocking thread: a keychain
    /// call can wait on the system, and on a prompt.
    pub(super) async fn host_secret(
        &self,
        name: &'static str,
    ) -> Result<Option<String>, LibraryError> {
        let database = self.database.clone();
        let secret = blocking("read a host secret", move || database.host_secret(name))
            .await
            .map_err(|failed| self.bug(failed))?;
        Ok(secret?)
    }

    /// Store a host secret in the keychain on a blocking thread.
    async fn set_host_secret(&self, name: &'static str, value: String) -> Result<(), LibraryError> {
        let database = self.database.clone();
        let stored =
            blocking("store a host secret", move || database.set_host_secret(name, &value))
                .await
                .map_err(|failed| self.bug(failed))?;
        Ok(stored?)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
