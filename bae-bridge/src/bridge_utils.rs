use crate::types::{
    BridgeConfig, BridgeDiscogsTokenStatus, BridgeExportBitDepth, BridgeExportLocation,
    BridgeExportPregapPlacement, BridgeExportPreset, BridgeExportPresetCodec,
    BridgeExportSelection, BridgeMcpConfig, BridgeSyncConfig, BridgeSyncProvider,
};

impl BridgeExportLocation {
    /// A fixed folder's path crosses as a string.
    pub(crate) fn from_core(location: &bae_core::config::ExportLocation) -> Self {
        match location {
            bae_core::config::ExportLocation::AskEachTime => BridgeExportLocation::AskEachTime,
            bae_core::config::ExportLocation::Fixed(dir) => BridgeExportLocation::Fixed {
                dir: dir.to_string_lossy().to_string(),
            },
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportLocation {
        match self {
            BridgeExportLocation::AskEachTime => bae_core::config::ExportLocation::AskEachTime,
            BridgeExportLocation::Fixed { dir } => {
                bae_core::config::ExportLocation::Fixed(std::path::PathBuf::from(dir))
            }
        }
    }
}

impl BridgeExportBitDepth {
    pub(crate) fn from_core(bit_depth: bae_core::config::ExportBitDepth) -> Self {
        match bit_depth {
            bae_core::config::ExportBitDepth::Source => BridgeExportBitDepth::Source,
            bae_core::config::ExportBitDepth::Bits16 => BridgeExportBitDepth::Bits16,
            bae_core::config::ExportBitDepth::Bits24 => BridgeExportBitDepth::Bits24,
            bae_core::config::ExportBitDepth::Bits32 => BridgeExportBitDepth::Bits32,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportBitDepth {
        match self {
            BridgeExportBitDepth::Source => bae_core::config::ExportBitDepth::Source,
            BridgeExportBitDepth::Bits16 => bae_core::config::ExportBitDepth::Bits16,
            BridgeExportBitDepth::Bits24 => bae_core::config::ExportBitDepth::Bits24,
            BridgeExportBitDepth::Bits32 => bae_core::config::ExportBitDepth::Bits32,
        }
    }
}

impl BridgeExportPresetCodec {
    pub(crate) fn from_core(codec: &bae_core::config::ExportPresetCodec) -> Self {
        match codec {
            bae_core::config::ExportPresetCodec::Flac { bit_depth } => {
                BridgeExportPresetCodec::Flac {
                    bit_depth: BridgeExportBitDepth::from_core(*bit_depth),
                }
            }
            bae_core::config::ExportPresetCodec::Mp3 { bitrate_kbps } => {
                BridgeExportPresetCodec::Mp3 {
                    bitrate_kbps: *bitrate_kbps,
                }
            }
            bae_core::config::ExportPresetCodec::OpusOgg { bitrate_kbps } => {
                BridgeExportPresetCodec::OpusOgg {
                    bitrate_kbps: *bitrate_kbps,
                }
            }
            bae_core::config::ExportPresetCodec::Wav { bit_depth } => {
                BridgeExportPresetCodec::Wav {
                    bit_depth: BridgeExportBitDepth::from_core(*bit_depth),
                }
            }
            bae_core::config::ExportPresetCodec::Aiff { bit_depth } => {
                BridgeExportPresetCodec::Aiff {
                    bit_depth: BridgeExportBitDepth::from_core(*bit_depth),
                }
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportPresetCodec {
        match self {
            BridgeExportPresetCodec::Flac { bit_depth } => {
                bae_core::config::ExportPresetCodec::Flac {
                    bit_depth: bit_depth.into_core(),
                }
            }
            BridgeExportPresetCodec::Mp3 { bitrate_kbps } => {
                bae_core::config::ExportPresetCodec::Mp3 { bitrate_kbps }
            }
            BridgeExportPresetCodec::OpusOgg { bitrate_kbps } => {
                bae_core::config::ExportPresetCodec::OpusOgg { bitrate_kbps }
            }
            BridgeExportPresetCodec::Wav { bit_depth } => {
                bae_core::config::ExportPresetCodec::Wav {
                    bit_depth: bit_depth.into_core(),
                }
            }
            BridgeExportPresetCodec::Aiff { bit_depth } => {
                bae_core::config::ExportPresetCodec::Aiff {
                    bit_depth: bit_depth.into_core(),
                }
            }
        }
    }
}

impl BridgeExportPregapPlacement {
    pub(crate) fn from_core(placement: bae_core::config::ExportPregapPlacement) -> Self {
        match placement {
            bae_core::config::ExportPregapPlacement::AppendToPreviousExceptHtoa => {
                BridgeExportPregapPlacement::AppendToPreviousExceptHtoa
            }
            bae_core::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa => {
                BridgeExportPregapPlacement::AppendToPreviousIncludingHtoa
            }
            bae_core::config::ExportPregapPlacement::Exclude => {
                BridgeExportPregapPlacement::Exclude
            }
            bae_core::config::ExportPregapPlacement::SingleFileWithCue => {
                BridgeExportPregapPlacement::SingleFileWithCue
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportPregapPlacement {
        match self {
            BridgeExportPregapPlacement::AppendToPreviousExceptHtoa => {
                bae_core::config::ExportPregapPlacement::AppendToPreviousExceptHtoa
            }
            BridgeExportPregapPlacement::AppendToPreviousIncludingHtoa => {
                bae_core::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa
            }
            BridgeExportPregapPlacement::Exclude => {
                bae_core::config::ExportPregapPlacement::Exclude
            }
            BridgeExportPregapPlacement::SingleFileWithCue => {
                bae_core::config::ExportPregapPlacement::SingleFileWithCue
            }
        }
    }
}

impl BridgeExportSelection {
    pub(crate) fn from_core(selection: &bae_core::config::ExportSelection) -> Self {
        match selection {
            bae_core::config::ExportSelection::Original => BridgeExportSelection::Original,
            bae_core::config::ExportSelection::Preset { preset_id } => {
                BridgeExportSelection::Preset {
                    preset_id: preset_id.clone(),
                }
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportSelection {
        match self {
            BridgeExportSelection::Original => bae_core::config::ExportSelection::Original,
            BridgeExportSelection::Preset { preset_id } => {
                bae_core::config::ExportSelection::Preset { preset_id }
            }
        }
    }
}

impl BridgeExportPreset {
    pub(crate) fn from_core(preset: &bae_core::config::ExportPreset) -> Self {
        let bae_core::config::ExportPreset {
            id,
            name,
            codec,
            filename_template,
            pregap_placement,
            applies_to_track,
            applies_to_release,
        } = preset;
        BridgeExportPreset {
            id: id.clone(),
            name: name.clone(),
            codec: BridgeExportPresetCodec::from_core(codec),
            // Derived from the codec; the core preset re-derives it on `into_core`.
            extension: codec.extension().to_string(),
            filename_template: filename_template.clone(),
            pregap_placement: BridgeExportPregapPlacement::from_core(*pregap_placement),
            applies_to_track: *applies_to_track,
            applies_to_release: *applies_to_release,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportPreset {
        let BridgeExportPreset {
            id,
            name,
            codec,
            filename_template,
            pregap_placement,
            applies_to_track,
            applies_to_release,
            // The core preset re-derives this from `codec`; the carried value is
            // dropped.
            extension: _,
        } = self;
        bae_core::config::ExportPreset {
            id,
            name,
            codec: codec.into_core(),
            filename_template,
            pregap_placement: pregap_placement.into_core(),
            applies_to_track,
            applies_to_release,
        }
    }
}

impl BridgeConfig {
    /// `sync` is `Some` whenever a provider is set in YAML — it does not mean the
    /// sync loop is running. That is runtime state, and lives in the sync-status
    /// snapshot, not on `BridgeConfig`.
    ///
    /// bae-core's own `Config` fields are exhaustively destructured so a new one
    /// fails the build here. The coven `inner` sub-config it embeds is an external
    /// crate's type — exempt from the destructure — so its fields (`store_id`,
    /// `cloud_home`, …) stay dotted reads through `inner`.
    pub(crate) fn from_core(config: &bae_core::config::Config) -> Self {
        let discogs_status = config.discogs_token_status();
        let cloud_account_display = config.cloud_account_display();
        let bae_core::config::Config {
            inner,
            // Read via the derived `discogs_token_status()` above.
            discogs: _,
            // Playback loudness policy; not surfaced on the config screen.
            replay_gain_mode: _,
            export_location,
            export_filename_template,
            export_presets,
            default_track_export_selection,
            default_release_export_selection,
            pause_between_sides,
            show_remaining_time,
            // Import-time decode verification; not surfaced on the config screen.
            verify_decode_on_import: _,
            mcp,
        } = config;

        let bae_core::config::McpConfig { enabled, port } = mcp;

        BridgeConfig {
            library_id: inner.store_id.clone(),
            library_name: inner.store_name.clone(),
            library_path: inner.store_dir.to_string_lossy().to_string(),
            encryption_key_stored: inner.encryption_key_stored,
            encryption_key_fingerprint: inner.encryption_key_fingerprint.clone(),
            pause_between_sides: *pause_between_sides,
            show_remaining_time: *show_remaining_time,
            export_location: BridgeExportLocation::from_core(export_location),
            export_filename_template: export_filename_template.clone(),
            export_presets: export_presets
                .iter()
                .map(BridgeExportPreset::from_core)
                .collect(),
            default_track_export_selection: BridgeExportSelection::from_core(
                default_track_export_selection,
            ),
            default_release_export_selection: BridgeExportSelection::from_core(
                default_release_export_selection,
            ),
            mcp: BridgeMcpConfig {
                enabled: *enabled,
                port: *port,
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
