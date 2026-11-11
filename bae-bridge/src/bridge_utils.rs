#[cfg(desktop)]
use crate::types::BridgeOutputKind;
use crate::types::{
    BridgeCastTranscodeFormat, BridgeConfig, BridgeDiscogsTokenStatus, BridgeImportDestination,
    BridgeLookupCatalog, BridgeLookupCatalogSetting, BridgeMcpConfig, BridgeNamePatternError,
    BridgeSaveBitDepth, BridgeSaveCodec, BridgeSaveExtras, BridgeSaveFilenamePiece,
    BridgeSaveFilenameValue, BridgeSaveFolderPiece, BridgeSaveFolderValue, BridgeSavePreset,
    BridgeSidePauseCountdown, BridgeSourceAvailability, BridgeSubsonicConfig,
    BridgeSubsonicNetwork,
};

mirror_enum! {
    BridgeImportDestination = bae_preferences::ImportDestination,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { ThisDevice, Cloud },
}

mirror_enum! {
    BridgeSourceAvailability = bae_preferences::SourceAvailability,
    from_core: pub(crate) fn,
    variants: { On, Off, NotConfigured },
}

impl BridgeLookupCatalogSetting {
    pub(crate) fn from_core(entry: bae_preferences::LookupCatalogSetting) -> Self {
        let bae_preferences::LookupCatalogSetting { availability, can_change } = entry;
        let bae_preferences::CatalogAvailability { catalog, state } = availability;
        Self {
            catalog: BridgeLookupCatalog::from_core(catalog),
            availability: BridgeSourceAvailability::from_core(state),
            can_change,
        }
    }
}

mirror_enum! {
    BridgeSaveBitDepth = bae_preferences::SaveBitDepth,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { SameAsSource, Bits16 },
}

impl BridgeSaveCodec {
    pub(crate) fn from_core(codec: &bae_preferences::SaveCodec) -> Self {
        match codec {
            bae_preferences::SaveCodec::Flac { bit_depth } => {
                BridgeSaveCodec::Flac { bit_depth: BridgeSaveBitDepth::from_core(*bit_depth) }
            }
            bae_preferences::SaveCodec::Mp3 { bitrate_kbps } => {
                BridgeSaveCodec::Mp3 { bitrate_kbps: *bitrate_kbps }
            }
            bae_preferences::SaveCodec::Aac { bitrate_kbps } => {
                BridgeSaveCodec::Aac { bitrate_kbps: *bitrate_kbps }
            }
            bae_preferences::SaveCodec::OpusOgg { bitrate_kbps } => {
                BridgeSaveCodec::OpusOgg { bitrate_kbps: *bitrate_kbps }
            }
            bae_preferences::SaveCodec::Wav { bit_depth } => {
                BridgeSaveCodec::Wav { bit_depth: BridgeSaveBitDepth::from_core(*bit_depth) }
            }
            bae_preferences::SaveCodec::Aiff { bit_depth } => {
                BridgeSaveCodec::Aiff { bit_depth: BridgeSaveBitDepth::from_core(*bit_depth) }
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_preferences::SaveCodec {
        match self {
            BridgeSaveCodec::Flac { bit_depth } => {
                bae_preferences::SaveCodec::Flac { bit_depth: bit_depth.into_core() }
            }
            BridgeSaveCodec::Mp3 { bitrate_kbps } => {
                bae_preferences::SaveCodec::Mp3 { bitrate_kbps }
            }
            BridgeSaveCodec::Aac { bitrate_kbps } => {
                bae_preferences::SaveCodec::Aac { bitrate_kbps }
            }
            BridgeSaveCodec::OpusOgg { bitrate_kbps } => {
                bae_preferences::SaveCodec::OpusOgg { bitrate_kbps }
            }
            BridgeSaveCodec::Wav { bit_depth } => {
                bae_preferences::SaveCodec::Wav { bit_depth: bit_depth.into_core() }
            }
            BridgeSaveCodec::Aiff { bit_depth } => {
                bae_preferences::SaveCodec::Aiff { bit_depth: bit_depth.into_core() }
            }
        }
    }
}

impl BridgeSaveExtras {
    pub(crate) fn from_core(extras: &bae_preferences::SaveExtras) -> Self {
        let bae_preferences::SaveExtras { embed_cover, cover_file, loudness_tags } = extras;
        BridgeSaveExtras {
            embed_cover: *embed_cover,
            cover_file: *cover_file,
            loudness_tags: *loudness_tags,
        }
    }
}

mirror_enum! {
    BridgeSaveFolderValue = bae_preferences::SaveFolderValue,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Artist, Album, AlbumYear, ReleaseYear, Label, CatalogNumber },
}

mirror_enum! {
    BridgeSaveFilenameValue = bae_preferences::SaveFilenameValue,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Title, Artist, Album, AlbumYear, ReleaseYear, TrackNumber, DiscNumber, TrackTotal },
}

impl BridgeSaveFilenamePiece {
    pub(crate) fn from_core(
        piece: &bae_preferences::NamePiece<bae_preferences::SaveFilenameValue>,
    ) -> Self {
        match piece {
            bae_preferences::NamePiece::Value(value) => {
                Self::Value { value: BridgeSaveFilenameValue::from_core(*value) }
            }
            bae_preferences::NamePiece::Text(text) => Self::Text { text: text.clone() },
        }
    }

    pub(crate) fn into_core(
        self,
    ) -> bae_preferences::NamePiece<bae_preferences::SaveFilenameValue> {
        match self {
            Self::Value { value } => bae_preferences::NamePiece::Value(value.into_core()),
            Self::Text { text } => bae_preferences::NamePiece::Text(text),
        }
    }
}

impl BridgeSaveFolderPiece {
    pub(crate) fn from_core(
        piece: &bae_preferences::NamePiece<bae_preferences::SaveFolderValue>,
    ) -> Self {
        match piece {
            bae_preferences::NamePiece::Value(value) => {
                Self::Value { value: BridgeSaveFolderValue::from_core(*value) }
            }
            bae_preferences::NamePiece::Text(text) => Self::Text { text: text.clone() },
        }
    }

    pub(crate) fn into_core(self) -> bae_preferences::NamePiece<bae_preferences::SaveFolderValue> {
        match self {
            Self::Value { value } => bae_preferences::NamePiece::Value(value.into_core()),
            Self::Text { text } => bae_preferences::NamePiece::Text(text),
        }
    }
}

impl BridgeNamePatternError {
    /// Core's answer on `pattern`: why it can't make a name, or `None`.
    pub(crate) fn of<Value>(pattern: &bae_preferences::NamePattern<Value>) -> Option<Self> {
        match pattern.validate() {
            Ok(()) => None,
            Err(bae_preferences::NamePatternError::Empty) => Some(Self::Empty),
            Err(bae_preferences::NamePatternError::StartsWithDot) => Some(Self::StartsWithDot),
            Err(bae_preferences::NamePatternError::ForbiddenCharacter(character)) => {
                Some(Self::ForbiddenCharacter { character: character.to_string() })
            }
        }
    }
}

// The output queue and its bridge functions are desktop-only (handle.rs gates
// them off ios/android), so this conversion's one consumer is too.
#[cfg(desktop)]
impl BridgeOutputKind {
    /// Display-only: a save carries its preset's resolved name, not an id — the
    /// queue row never dereferences a preset. No `into_core`; enqueue takes a
    /// preset id directly.
    pub(crate) fn from_core(kind: &bae_transfers::OutputKind) -> Self {
        match kind {
            bae_transfers::OutputKind::Export => BridgeOutputKind::Export,
            bae_transfers::OutputKind::Save { preset, .. } => {
                BridgeOutputKind::Save { preset_name: preset.name.clone() }
            }
        }
    }
}

impl BridgeSavePreset {
    pub(crate) fn from_core(preset: &bae_preferences::SavePreset) -> Self {
        let bae_preferences::SavePreset { id, name, codec } = preset;
        BridgeSavePreset {
            id: id.clone(),
            name: name.clone(),
            codec: BridgeSaveCodec::from_core(codec),
        }
    }

    pub(crate) fn into_core(self) -> bae_preferences::SavePreset {
        let BridgeSavePreset { id, name, codec } = self;
        bae_preferences::SavePreset { id, name, codec: codec.into_core() }
    }
}

impl BridgeConfig {
    pub(crate) fn from_core(config: &bae_preferences::Config) -> Self {
        let discogs_status = config.prefs.discogs_token_status();
        let catalogs = config.prefs.lookup_catalog_settings();
        let lookup_catalogs =
            catalogs.into_iter().map(BridgeLookupCatalogSetting::from_core).collect();
        let prefs = &config.prefs;
        let bae_preferences::Preferences {
            // Read via the derived `discogs_token_status()` above.
            discogs: _,
            // Playback loudness policy; not surfaced on the config screen.
            replay_gain_mode: _,
            save_presets,
            save_filename_pattern,
            save_folder_pattern,
            save_extras,
            last_save_preset,
            pause_between_sides,
            side_pause_countdown,
            max_concurrent_uploads,
            max_concurrent_downloads,
            import_destination,
            identification,
            show_remaining_time,
            library_full_width,
            ask_before_unpinning,
            cast_enabled,
            cast_transcode_format,
            mcp,
            subsonic,
        } = prefs;

        let bae_preferences::IdentificationPreferences {
            automatic,
            import_when_identified,
            // Read through `Preferences::metadata_sources()` above, which folds this
            // raw preference together with each catalog's credentials into the
            // one answer a surface renders.
            catalogs: _,
        } = identification;
        let bae_preferences::McpConfig { enabled, port } = mcp;
        let bae_preferences::SubsonicConfig {
            enabled: subsonic_enabled,
            port: subsonic_port,
            username: subsonic_username,
            network: subsonic_network,
        } = subsonic;

        BridgeConfig {
            library_id: config.store_id.to_string(),
            library_path: config.library_path.to_string_lossy().to_string(),
            pause_between_sides: *pause_between_sides,
            side_pause_countdown: BridgeSidePauseCountdown::from_core(*side_pause_countdown),
            max_concurrent_uploads: max_concurrent_uploads.get(),
            max_concurrent_downloads: max_concurrent_downloads.get(),
            max_concurrent_transfers: bae_preferences::MAX_CONCURRENT_TRANSFERS,
            identify_automatically: *automatic,
            import_when_identified: *import_when_identified,
            import_destination: BridgeImportDestination::from_core(*import_destination),
            lookup_catalogs,
            has_source_to_search: bae_preferences::asks_a_source(&config.prefs.metadata_sources()),
            show_remaining_time: *show_remaining_time,
            library_full_width: *library_full_width,
            ask_before_unpinning: *ask_before_unpinning,
            save_presets: save_presets.iter().map(BridgeSavePreset::from_core).collect(),
            filename_pieces: save_filename_pattern
                .pieces()
                .iter()
                .map(BridgeSaveFilenamePiece::from_core)
                .collect(),
            filename_pattern_error: BridgeNamePatternError::of(save_filename_pattern),
            folder_pieces: save_folder_pattern
                .pieces()
                .iter()
                .map(BridgeSaveFolderPiece::from_core)
                .collect(),
            folder_pattern_error: BridgeNamePatternError::of(save_folder_pattern),
            save_extras: BridgeSaveExtras::from_core(save_extras),
            last_save_preset: last_save_preset.clone(),
            cast_enabled: *cast_enabled,
            cast_transcode_format: BridgeCastTranscodeFormat::from_core(*cast_transcode_format),
            mcp: BridgeMcpConfig { enabled: *enabled, port: *port },
            subsonic: BridgeSubsonicConfig {
                enabled: *subsonic_enabled,
                port: *subsonic_port,
                username: subsonic_username.clone(),
                network: BridgeSubsonicNetwork::from_core(subsonic_network.clone()),
            },
            discogs_usable: discogs_status.is_usable(),
            discogs_token_status: match discogs_status {
                bae_preferences::DiscogsTokenStatus::NotConfigured => {
                    BridgeDiscogsTokenStatus::NotConfigured
                }
                bae_preferences::DiscogsTokenStatus::Valid => BridgeDiscogsTokenStatus::Valid,
                bae_preferences::DiscogsTokenStatus::Unvalidated => {
                    BridgeDiscogsTokenStatus::Unvalidated
                }
                bae_preferences::DiscogsTokenStatus::Rejected => BridgeDiscogsTokenStatus::Rejected,
            },
        }
    }
}
