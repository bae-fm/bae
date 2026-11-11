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
        self.preferences.edit(edit).await
    }

    /// This device's preferences for the library, as stored now.
    pub async fn preferences(&self) -> Result<bae_preferences::Preferences, bae_store::DbError> {
        bae_preferences::preferences(&self.store).await
    }

    /// bae's own directory for this library, apart from coven's store: the
    /// remote image cache and the like.
    pub fn library_path(&self) -> PathBuf {
        self.app_dir.registered_library(&self.store.store_id().to_string())
    }

    /// The library's preferences as the settings screens read them, with the
    /// library and its directory: the first value at once, then one after
    /// each write that changes them, until the surface cancels or the library
    /// closes.
    pub fn subscribe_config(&self) -> bae_store::live_query::LiveValues<bae_preferences::Config> {
        let store_id = self.store.store_id();
        let library_path = self.library_path();
        self.values_until_closed(bae_preferences::subscribe_preferences(&self.store).process(
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
    ) -> bae_store::live_query::LiveValues<T> {
        self.values_until_closed(
            bae_preferences::subscribe_preferences(&self.store)
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

    pref_setter!(
        /// Whether Unpin asks before removing a release's files from this
        /// device.
        set_ask_before_unpinning,
        ask_before_unpinning: bool
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

    /// Where an import goes: this device or the cloud.
    pub async fn set_import_destination(
        &self,
        destination: bae_preferences::ImportDestination,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.import_destination = destination;
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
    ) -> Result<Vec<bae_preferences::CatalogAvailability>, bae_store::DbError> {
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
        self.store.set_transfer_limits(self.preferences().await?.transfer_limits());
        Ok(())
    }

    /// The limits the open store runs under — what a change through the
    /// setters above has taken effect as.
    pub fn transfer_limits(&self) -> coven::TransferLimits {
        self.store.transfer_limits()
    }

    pub async fn save_presets(
        &self,
    ) -> Result<Vec<bae_preferences::SavePreset>, bae_store::DbError> {
        Ok(self.preferences().await?.save_presets)
    }

    /// The save preset `preset_id` names.
    #[cfg(desktop)]
    pub(crate) async fn save_preset_for(
        &self,
        preset_id: &str,
    ) -> Result<bae_preferences::SavePreset, LibraryError> {
        self.save_presets()
            .await?
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .ok_or_else(|| LibraryError::Save(format!("save preset {preset_id} does not exist")))
    }

    pub async fn set_save_presets(
        &self,
        presets: Vec<bae_preferences::SavePreset>,
    ) -> Result<(), bae_preferences::PreferencesError> {
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for preset in &presets {
            preset.validate()?;
            if !names.insert(preset.name.clone()) {
                return Err(bae_preferences::PreferencesError::Invalid(format!(
                    "another export preset is named {}",
                    preset.name
                )));
            }
            if !ids.insert(preset.id.clone()) {
                return Err(bae_preferences::PreferencesError::Invalid(format!(
                    "duplicate export preset id {}",
                    preset.id
                )));
            }
        }
        let Some(first) = presets.first().map(|preset| preset.id.clone()) else {
            return Err(bae_preferences::PreferencesError::Invalid(
                "at least one export preset must remain".to_string(),
            ));
        };
        // The last-used format is a required, valid preset id: when the new
        // list no longer holds it, the first remaining format becomes it in
        // the same write.
        self.update_preferences(move |prefs| {
            if !presets.iter().any(|preset| preset.id == prefs.last_save_preset) {
                prefs.last_save_preset = first;
            }
            prefs.save_presets = presets;
            Ok(())
        })
        .await
    }

    /// Set the one file name pattern every save names its files by, whatever
    /// the format. A pattern that can't make a name is stored as typed (a save
    /// names by the default pattern until it is fixed); a piece of typed text
    /// with nothing in it is refused.
    pub async fn set_save_filename_pattern(
        &self,
        pattern: bae_preferences::SaveFilenamePattern,
    ) -> Result<(), bae_preferences::PreferencesError> {
        Self::refuse_empty_text(&pattern)?;
        self.update_preferences(move |prefs| {
            prefs.save_filename_pattern = pattern;
            Ok(())
        })
        .await
    }

    /// Set the folder name pattern a release save names its folder by, stored
    /// as typed like the file name pattern.
    pub async fn set_save_folder_pattern(
        &self,
        pattern: bae_preferences::SaveFolderPattern,
    ) -> Result<(), bae_preferences::PreferencesError> {
        Self::refuse_empty_text(&pattern)?;
        self.update_preferences(move |prefs| {
            prefs.save_folder_pattern = pattern;
            Ok(())
        })
        .await
    }

    fn refuse_empty_text<Value>(
        pattern: &bae_preferences::NamePattern<Value>,
    ) -> Result<(), bae_preferences::PreferencesError> {
        if pattern.has_empty_text() {
            return Err(bae_preferences::PreferencesError::Invalid(
                "a name pattern holds typed text of nothing".to_string(),
            ));
        }
        Ok(())
    }

    /// Set whether each saved file embeds the release's cover.
    pub async fn set_save_embed_cover(
        &self,
        on: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.save_extras.embed_cover = on;
            Ok(())
        })
        .await
    }

    /// Set whether a release save writes the cover as `cover.jpg` in its folder.
    pub async fn set_save_cover_file(
        &self,
        on: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.save_extras.cover_file = on;
            Ok(())
        })
        .await
    }

    /// Set whether each saved file carries ReplayGain tags.
    pub async fn set_save_loudness_tags(
        &self,
        on: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.save_extras.loudness_tags = on;
            Ok(())
        })
        .await
    }

    /// Record `preset_id` as the format the last save used, which the next
    /// save starts with.
    #[cfg(desktop)]
    pub(crate) async fn set_last_save_preset(
        &self,
        preset_id: String,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            Self::validate_last_save_preset(&preset_id, &prefs.save_presets)?;
            prefs.last_save_preset = preset_id;
            Ok(())
        })
        .await
    }

    /// The last-used format must name a preset that exists, so it is never
    /// dangling.
    #[cfg(desktop)]
    fn validate_last_save_preset(
        preset_id: &str,
        presets: &[bae_preferences::SavePreset],
    ) -> Result<(), bae_preferences::PreferencesError> {
        if presets.iter().any(|preset| preset.id == *preset_id) {
            Ok(())
        } else {
            Err(bae_preferences::PreferencesError::Invalid(format!(
                "unknown export preset {preset_id}"
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

    /// Switch the Subsonic server on or off: what the person wants. It stays
    /// as set whether or not the server can run, which the server reports.
    pub async fn set_subsonic_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.subsonic.enabled = enabled;
            Ok(())
        })
        .await
    }

    /// Set the port the Subsonic server listens on, as typed.
    pub async fn set_subsonic_port(
        &self,
        port: u32,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.subsonic.port = port;
            Ok(())
        })
        .await
    }

    /// Set the username Subsonic apps sign in with.
    pub async fn set_subsonic_username(
        &self,
        username: String,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.subsonic.username = username;
            Ok(())
        })
        .await
    }

    /// Set where the Subsonic server listens.
    pub async fn set_subsonic_network(
        &self,
        network: bae_preferences::SubsonicNetwork,
    ) -> Result<(), bae_preferences::PreferencesError> {
        self.update_preferences(move |prefs| {
            prefs.subsonic.network = network;
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
        let store = self.store.clone();
        let secret = self
            .diagnostics
            .blocking("read a host secret", move || store.host_secret(name))
            .await?;
        Ok(secret?)
    }

    /// Store a host secret in the keychain on a blocking thread.
    async fn set_host_secret(&self, name: &'static str, value: String) -> Result<(), LibraryError> {
        let store = self.store.clone();
        let stored = self
            .diagnostics
            .blocking("store a host secret", move || store.set_host_secret(name, &value))
            .await?;
        Ok(stored?)
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
