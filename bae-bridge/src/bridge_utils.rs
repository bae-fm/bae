use crate::types::{
    BridgeConfig, BridgeDiscogsTokenStatus, BridgeExportBitDepth, BridgeExportFilenameToken,
    BridgeExportPregapPlacement, BridgeExportPreset, BridgeExportPresetCodec, BridgeMcpConfig,
    BridgeOutputKind, BridgeSyncConfig, BridgeSyncProvider,
};

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

impl BridgeExportFilenameToken {
    pub(crate) fn from_core(token: bae_core::config::ExportFilenameToken) -> Self {
        match token {
            bae_core::config::ExportFilenameToken::Title => BridgeExportFilenameToken::Title,
            bae_core::config::ExportFilenameToken::Artist => BridgeExportFilenameToken::Artist,
            bae_core::config::ExportFilenameToken::Album => BridgeExportFilenameToken::Album,
            bae_core::config::ExportFilenameToken::Year => BridgeExportFilenameToken::Year,
            bae_core::config::ExportFilenameToken::TrackNumber => {
                BridgeExportFilenameToken::TrackNumber
            }
            bae_core::config::ExportFilenameToken::DiscNumber => {
                BridgeExportFilenameToken::DiscNumber
            }
            bae_core::config::ExportFilenameToken::TrackTotal => {
                BridgeExportFilenameToken::TrackTotal
            }
        }
    }

    pub(crate) fn into_core(self) -> bae_core::config::ExportFilenameToken {
        match self {
            BridgeExportFilenameToken::Title => bae_core::config::ExportFilenameToken::Title,
            BridgeExportFilenameToken::Artist => bae_core::config::ExportFilenameToken::Artist,
            BridgeExportFilenameToken::Album => bae_core::config::ExportFilenameToken::Album,
            BridgeExportFilenameToken::Year => bae_core::config::ExportFilenameToken::Year,
            BridgeExportFilenameToken::TrackNumber => {
                bae_core::config::ExportFilenameToken::TrackNumber
            }
            BridgeExportFilenameToken::DiscNumber => {
                bae_core::config::ExportFilenameToken::DiscNumber
            }
            BridgeExportFilenameToken::TrackTotal => {
                bae_core::config::ExportFilenameToken::TrackTotal
            }
        }
    }
}

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

impl BridgeExportPreset {
    pub(crate) fn from_core(preset: &bae_core::config::ExportPreset) -> Self {
        let bae_core::config::ExportPreset {
            id,
            name,
            codec,
            filename_tokens,
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
            filename_tokens: filename_tokens
                .iter()
                .copied()
                .map(BridgeExportFilenameToken::from_core)
                .collect(),
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
            filename_tokens,
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
            filename_tokens: filename_tokens
                .into_iter()
                .map(BridgeExportFilenameToken::into_core)
                .collect(),
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
            export_filename_tokens,
            export_presets,
            default_track_save_preset,
            default_release_save_preset,
            pause_between_sides,
            max_concurrent_uploads,
            max_concurrent_downloads,
            show_remaining_time,
            library_full_width,
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
            max_concurrent_uploads: max_concurrent_uploads.get(),
            max_concurrent_downloads: max_concurrent_downloads.get(),
            show_remaining_time: *show_remaining_time,
            library_full_width: *library_full_width,
            export_filename_tokens: export_filename_tokens
                .iter()
                .copied()
                .map(BridgeExportFilenameToken::from_core)
                .collect(),
            export_presets: export_presets
                .iter()
                .map(BridgeExportPreset::from_core)
                .collect(),
            default_track_save_preset: default_track_save_preset.clone(),
            default_release_save_preset: default_release_save_preset.clone(),
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
