#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::types::BridgeOutputKind;
use crate::types::{
    BridgeCastTranscodeFormat, BridgeCatalog, BridgeConfig, BridgeDiscogsTokenStatus,
    BridgeImportStorage, BridgeLookupCatalogSetting, BridgeMcpConfig, BridgeSaveBitDepth,
    BridgeSaveCodec, BridgeSaveFilenameToken, BridgeSavePregapPlacement, BridgeSavePreset,
    BridgeSidePauseCountdown, BridgeSourceAvailability, BridgeSubsonicConfig,
};

mirror_enum! {
    BridgeSourceAvailability = bae_core::import::SourceAvailability,
    from_core: pub(crate) fn,
    variants: { On, Off, NotConfigured },
}

impl BridgeLookupCatalogSetting {
    pub(crate) fn from_core(entry: bae_core::config::LookupCatalogSetting) -> Self {
        let bae_core::config::LookupCatalogSetting {
            availability,
            can_change,
        } = entry;
        let bae_core::import::CatalogAvailability { catalog, state } = availability;
        Self {
            catalog: BridgeCatalog::from_core(catalog),
            availability: BridgeSourceAvailability::from_core(state),
            can_change,
        }
    }
}

mirror_enum! {
    BridgeSaveBitDepth = bae_core::config::SaveBitDepth,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Source, Bits16, Bits24, Bits32 },
}

impl BridgeSaveCodec {
    pub(crate) fn from_core(codec: &bae_core::config::SaveCodec) -> Self {
        match codec {
            bae_core::config::SaveCodec::Flac { bit_depth } => BridgeSaveCodec::Flac {
                bit_depth: BridgeSaveBitDepth::from_core(*bit_depth),
            },
            bae_core::config::SaveCodec::Mp3 { bitrate_kbps } => BridgeSaveCodec::Mp3 {
                bitrate_kbps: *bitrate_kbps,
            },
            bae_core::config::SaveCodec::Aac { bitrate_kbps } => BridgeSaveCodec::Aac {
                bitrate_kbps: *bitrate_kbps,
            },
            bae_core::config::SaveCodec::OpusOgg { bitrate_kbps } => BridgeSaveCodec::OpusOgg {
                bitrate_kbps: *bitrate_kbps,
            },
            bae_core::config::SaveCodec::Wav { bit_depth } => BridgeSaveCodec::Wav {
                bit_depth: BridgeSaveBitDepth::from_core(*bit_depth),
            },
            bae_core::config::SaveCodec::Aiff { bit_depth } => BridgeSaveCodec::Aiff {
                bit_depth: BridgeSaveBitDepth::from_core(*bit_depth),
            },
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::SaveCodec {
        match self {
            BridgeSaveCodec::Flac { bit_depth } => bae_core::config::SaveCodec::Flac {
                bit_depth: bit_depth.into_core(),
            },
            BridgeSaveCodec::Mp3 { bitrate_kbps } => {
                bae_core::config::SaveCodec::Mp3 { bitrate_kbps }
            }
            BridgeSaveCodec::Aac { bitrate_kbps } => {
                bae_core::config::SaveCodec::Aac { bitrate_kbps }
            }
            BridgeSaveCodec::OpusOgg { bitrate_kbps } => {
                bae_core::config::SaveCodec::OpusOgg { bitrate_kbps }
            }
            BridgeSaveCodec::Wav { bit_depth } => bae_core::config::SaveCodec::Wav {
                bit_depth: bit_depth.into_core(),
            },
            BridgeSaveCodec::Aiff { bit_depth } => bae_core::config::SaveCodec::Aiff {
                bit_depth: bit_depth.into_core(),
            },
        }
    }
}

mirror_enum! {
    BridgeSavePregapPlacement = bae_core::config::SavePregapPlacement,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        AppendToPreviousExceptHtoa,
        AppendToPreviousIncludingHtoa,
        Exclude,
        SingleFileWithCue,
    },
}

mirror_enum! {
    BridgeSaveFilenameToken = bae_core::config::SaveFilenameToken,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Title, Artist, Album, Year, TrackNumber, DiscNumber, TrackTotal },
}

// The output queue and its bridge functions are desktop-only (handle.rs gates
// them off ios/android), so this conversion's one consumer is too.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl BridgeOutputKind {
    /// Display-only: a save carries its preset's resolved name, not an id — the
    /// queue row never dereferences a preset. No `into_core`; enqueue takes a
    /// preset id directly.
    pub(crate) fn from_core(kind: &bae_core::library::OutputKind) -> Self {
        match kind {
            bae_core::library::OutputKind::Export => BridgeOutputKind::Export,
            bae_core::library::OutputKind::Save { preset } => BridgeOutputKind::Save {
                preset_name: preset.name.clone(),
            },
        }
    }
}

impl BridgeSavePreset {
    pub(crate) fn from_core(preset: &bae_core::config::SavePreset) -> Self {
        let bae_core::config::SavePreset {
            id,
            name,
            codec,
            filename_tokens,
            pregap_placement,
            applies_to_track,
            applies_to_release,
            embed_cover,
        } = preset;
        BridgeSavePreset {
            id: id.clone(),
            name: name.clone(),
            codec: BridgeSaveCodec::from_core(codec),
            // Derived from the codec; the core preset re-derives it on `into_core`.
            extension: codec.extension().to_string(),
            filename_tokens: filename_tokens
                .iter()
                .copied()
                .map(BridgeSaveFilenameToken::from_core)
                .collect(),
            pregap_placement: BridgeSavePregapPlacement::from_core(*pregap_placement),
            applies_to_track: *applies_to_track,
            applies_to_release: *applies_to_release,
            embed_cover: *embed_cover,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::SavePreset {
        let BridgeSavePreset {
            id,
            name,
            codec,
            filename_tokens,
            pregap_placement,
            applies_to_track,
            applies_to_release,
            embed_cover,
            // The core preset re-derives this from `codec`; the carried value is
            // dropped.
            extension: _,
        } = self;
        bae_core::config::SavePreset {
            id,
            name,
            codec: codec.into_core(),
            filename_tokens: filename_tokens
                .into_iter()
                .map(BridgeSaveFilenameToken::into_core)
                .collect(),
            pregap_placement: pregap_placement.into_core(),
            applies_to_track,
            applies_to_release,
            embed_cover,
        }
    }
}

impl BridgeConfig {
    pub(crate) fn from_core(config: &bae_core::config::Config) -> Self {
        let discogs_status = config.discogs_token_status();
        let catalogs = config.lookup_catalog_settings();
        let lookup_catalogs = catalogs
            .into_iter()
            .map(BridgeLookupCatalogSetting::from_core)
            .collect();
        let prefs = &config.prefs;
        let bae_core::config::Preferences {
            // Read via the derived `discogs_token_status()` above.
            discogs: _,
            // Playback loudness policy; not surfaced on the config screen.
            replay_gain_mode: _,
            save_presets,
            default_track_save_preset,
            default_release_save_preset,
            pause_between_sides,
            side_pause_countdown,
            max_concurrent_uploads,
            max_concurrent_downloads,
            import_storage,
            identification,
            show_remaining_time,
            library_full_width,
            // Import-time decode verification; not surfaced on the config screen.
            verify_decode_on_import: _,
            cast_enabled,
            cast_transcode_format,
            mcp,
            subsonic,
        } = prefs;

        let bae_core::config::IdentificationPreferences {
            automatic,
            import_when_identified,
            // Read through `Config::metadata_sources()` above, which folds this
            // raw preference together with each catalog's credentials into the
            // one answer a surface renders.
            catalogs: _,
        } = identification;
        let bae_core::config::ImportStoragePreferences { cloud, pinned } = import_storage;
        let bae_core::config::McpConfig { enabled, port } = mcp;
        let bae_core::config::SubsonicConfig {
            enabled: subsonic_enabled,
            port: subsonic_port,
            username: subsonic_username,
            bind_address: subsonic_bind_address,
        } = subsonic;

        BridgeConfig {
            library_id: config.store_id.to_string(),
            library_path: config.library_path().to_string_lossy().to_string(),
            pause_between_sides: *pause_between_sides,
            side_pause_countdown: BridgeSidePauseCountdown::from_core(*side_pause_countdown),
            max_concurrent_uploads: max_concurrent_uploads.get(),
            max_concurrent_downloads: max_concurrent_downloads.get(),
            identify_automatically: *automatic,
            import_when_identified: *import_when_identified,
            import_storage: BridgeImportStorage {
                cloud: *cloud,
                pinned: *pinned,
            },
            lookup_catalogs,
            show_remaining_time: *show_remaining_time,
            library_full_width: *library_full_width,
            save_presets: save_presets
                .iter()
                .map(BridgeSavePreset::from_core)
                .collect(),
            default_track_save_preset: default_track_save_preset.clone(),
            default_release_save_preset: default_release_save_preset.clone(),
            cast_enabled: *cast_enabled,
            cast_transcode_format: BridgeCastTranscodeFormat::from_core(*cast_transcode_format),
            mcp: BridgeMcpConfig {
                enabled: *enabled,
                port: *port,
            },
            subsonic: BridgeSubsonicConfig {
                enabled: *subsonic_enabled,
                port: *subsonic_port,
                username: subsonic_username.clone(),
                bind_address: subsonic_bind_address.clone(),
            },
            discogs_usable: discogs_status.is_usable(),
            discogs_token_status: match discogs_status {
                bae_core::config::DiscogsTokenStatus::NotConfigured => {
                    BridgeDiscogsTokenStatus::NotConfigured
                }
                bae_core::config::DiscogsTokenStatus::Valid => BridgeDiscogsTokenStatus::Valid,
                bae_core::config::DiscogsTokenStatus::Unvalidated => {
                    BridgeDiscogsTokenStatus::Unvalidated
                }
                bae_core::config::DiscogsTokenStatus::Rejected => {
                    BridgeDiscogsTokenStatus::Rejected
                }
            },
        }
    }
}
