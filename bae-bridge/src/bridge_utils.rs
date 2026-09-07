#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::types::BridgeOutputKind;
use crate::types::{
    BridgeConfig, BridgeDefaultImportMetadataSource, BridgeDiscogsTokenStatus, BridgeMcpConfig,
    BridgeSaveBitDepth, BridgeSaveCodec, BridgeSaveFilenameToken, BridgeSavePregapPlacement,
    BridgeSavePreset, BridgeSubsonicConfig, BridgeSyncConfig, BridgeSyncProvider,
};

mirror_enum! {
    BridgeDefaultImportMetadataSource = bae_core::config::DefaultImportMetadataSource,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { FindOnline, FileTags, None },
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
    /// `sync` is `Some` whenever a provider is set in YAML — it does not mean the
    /// sync loop is running. That is runtime state, and lives in the sync-status
    /// snapshot, not on `BridgeConfig`.
    ///
    /// bae's own settings are exhaustively destructured so a new one fails the
    /// build here. The coven `inner` sub-config `Config` embeds is an external
    /// crate's type — exempt from the destructure — so its fields (`store_id`,
    /// `cloud_home`, …) stay dotted reads through `inner`.
    pub(crate) fn from_core(config: &bae_core::config::Config) -> Self {
        let discogs_status = config.discogs_token_status();
        let cloud_account_display = config.cloud_account_display();
        let bae_core::config::Config { inner, prefs, .. } = config;
        let bae_core::config::Preferences {
            // Read via the derived `discogs_token_status()` above.
            discogs: _,
            // Playback loudness policy; not surfaced on the config screen.
            replay_gain_mode: _,
            save_presets,
            default_track_save_preset,
            default_release_save_preset,
            pause_between_sides,
            max_concurrent_uploads,
            max_concurrent_downloads,
            identify_automatically,
            default_import_metadata_source,
            show_remaining_time,
            library_full_width,
            // Import-time decode verification; not surfaced on the config screen.
            verify_decode_on_import: _,
            cast_enabled,
            mcp,
            subsonic,
        } = prefs;

        let bae_core::config::McpConfig { enabled, port } = mcp;
        let bae_core::config::SubsonicConfig {
            enabled: subsonic_enabled,
            port: subsonic_port,
            username: subsonic_username,
            bind_address: subsonic_bind_address,
        } = subsonic;

        BridgeConfig {
            library_id: inner.store_id.clone(),
            library_name: inner.store_name.clone(),
            library_path: config.library_path().to_string_lossy().to_string(),
            pause_between_sides: *pause_between_sides,
            max_concurrent_uploads: max_concurrent_uploads.get(),
            max_concurrent_downloads: max_concurrent_downloads.get(),
            identify_automatically: *identify_automatically,
            default_import_metadata_source: BridgeDefaultImportMetadataSource::from_core(
                *default_import_metadata_source,
            ),
            show_remaining_time: *show_remaining_time,
            library_full_width: *library_full_width,
            save_presets: save_presets
                .iter()
                .map(BridgeSavePreset::from_core)
                .collect(),
            default_track_save_preset: default_track_save_preset.clone(),
            default_release_save_preset: default_release_save_preset.clone(),
            cast_enabled: *cast_enabled,
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
            sync: inner
                .cloud_home
                .provider
                .as_ref()
                .map(|provider| BridgeSyncConfig {
                    provider: BridgeSyncProvider::from_core(provider, &inner.cloud_home),
                    cloud_account_display,
                }),
        }
    }
}

impl BridgeSyncProvider {
    /// Carries only the fields the given provider uses. `CloudHomeConfig` is
    /// coven's (external crate) type, so its fields stay dotted reads.
    fn from_core(
        provider: &bae_core::config::CloudProvider,
        cloud_home: &bae_core::config::CloudHomeConfig,
    ) -> Self {
        use bae_core::config::CloudProvider;
        match provider {
            CloudProvider::S3 => BridgeSyncProvider::S3 {
                bucket: cloud_home.s3_bucket.clone(),
                region: cloud_home.s3_region.clone(),
                endpoint: cloud_home.s3_endpoint.clone(),
            },
            CloudProvider::GoogleDrive => BridgeSyncProvider::GoogleDrive,
            CloudProvider::Dropbox => BridgeSyncProvider::Dropbox,
            CloudProvider::OneDrive => BridgeSyncProvider::OneDrive,
            CloudProvider::CloudKit => BridgeSyncProvider::CloudKit,
        }
    }
}
