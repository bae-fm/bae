use crate::types::{
    BridgeConfig, BridgeDiscogsTokenStatus, BridgeExportBitDepth, BridgeExportLocation,
    BridgeExportMetadata, BridgeExportPregapPlacement, BridgeExportPreset, BridgeExportPresetCodec,
    BridgeExportSelection, BridgeMcpConfig, BridgeSyncConfig, BridgeSyncProvider,
};

/// Convert a core `ExportLocation` to the bridge enum. A fixed folder's path
/// crosses as a string.
pub(crate) fn bridge_export_location(
    location: &bae_core::config::ExportLocation,
) -> BridgeExportLocation {
    match location {
        bae_core::config::ExportLocation::AskEachTime => BridgeExportLocation::AskEachTime,
        bae_core::config::ExportLocation::Fixed(dir) => BridgeExportLocation::Fixed {
            dir: dir.to_string_lossy().to_string(),
        },
    }
}

/// Convert the bridge enum back to a core `ExportLocation` for persisting.
pub(crate) fn core_export_location(
    location: BridgeExportLocation,
) -> bae_core::config::ExportLocation {
    match location {
        BridgeExportLocation::AskEachTime => bae_core::config::ExportLocation::AskEachTime,
        BridgeExportLocation::Fixed { dir } => {
            bae_core::config::ExportLocation::Fixed(std::path::PathBuf::from(dir))
        }
    }
}

/// Convert a core `ExportMetadata` to the bridge record for the UI.
pub(crate) fn bridge_export_metadata(
    metadata: &bae_core::config::ExportMetadata,
) -> BridgeExportMetadata {
    BridgeExportMetadata {
        title: metadata.title,
        artist: metadata.artist,
        album: metadata.album,
        year: metadata.year,
        track_number: metadata.track_number,
        disc_number: metadata.disc_number,
        cover_art: metadata.cover_art,
    }
}

/// Convert the bridge record back to a core `ExportMetadata` for persisting.
pub(crate) fn core_export_metadata(
    metadata: BridgeExportMetadata,
) -> bae_core::config::ExportMetadata {
    bae_core::config::ExportMetadata {
        title: metadata.title,
        artist: metadata.artist,
        album: metadata.album,
        year: metadata.year,
        track_number: metadata.track_number,
        disc_number: metadata.disc_number,
        cover_art: metadata.cover_art,
    }
}

pub(crate) fn bridge_export_bit_depth(
    bit_depth: bae_core::config::ExportBitDepth,
) -> BridgeExportBitDepth {
    match bit_depth {
        bae_core::config::ExportBitDepth::Source => BridgeExportBitDepth::Source,
        bae_core::config::ExportBitDepth::Bits16 => BridgeExportBitDepth::Bits16,
        bae_core::config::ExportBitDepth::Bits24 => BridgeExportBitDepth::Bits24,
        bae_core::config::ExportBitDepth::Bits32 => BridgeExportBitDepth::Bits32,
    }
}

pub(crate) fn core_export_bit_depth(
    bit_depth: BridgeExportBitDepth,
) -> bae_core::config::ExportBitDepth {
    match bit_depth {
        BridgeExportBitDepth::Source => bae_core::config::ExportBitDepth::Source,
        BridgeExportBitDepth::Bits16 => bae_core::config::ExportBitDepth::Bits16,
        BridgeExportBitDepth::Bits24 => bae_core::config::ExportBitDepth::Bits24,
        BridgeExportBitDepth::Bits32 => bae_core::config::ExportBitDepth::Bits32,
    }
}

pub(crate) fn bridge_export_preset_codec(
    codec: &bae_core::config::ExportPresetCodec,
) -> BridgeExportPresetCodec {
    match codec {
        bae_core::config::ExportPresetCodec::Flac { bit_depth } => BridgeExportPresetCodec::Flac {
            bit_depth: bridge_export_bit_depth(*bit_depth),
        },
        bae_core::config::ExportPresetCodec::Mp3 { bitrate_kbps } => BridgeExportPresetCodec::Mp3 {
            bitrate_kbps: *bitrate_kbps,
        },
        bae_core::config::ExportPresetCodec::OpusOgg { bitrate_kbps } => {
            BridgeExportPresetCodec::OpusOgg {
                bitrate_kbps: *bitrate_kbps,
            }
        }
        bae_core::config::ExportPresetCodec::Wav { bit_depth } => BridgeExportPresetCodec::Wav {
            bit_depth: bridge_export_bit_depth(*bit_depth),
        },
        bae_core::config::ExportPresetCodec::Aiff { bit_depth } => BridgeExportPresetCodec::Aiff {
            bit_depth: bridge_export_bit_depth(*bit_depth),
        },
    }
}

pub(crate) fn core_export_preset_codec(
    codec: BridgeExportPresetCodec,
) -> bae_core::config::ExportPresetCodec {
    match codec {
        BridgeExportPresetCodec::Flac { bit_depth } => bae_core::config::ExportPresetCodec::Flac {
            bit_depth: core_export_bit_depth(bit_depth),
        },
        BridgeExportPresetCodec::Mp3 { bitrate_kbps } => {
            bae_core::config::ExportPresetCodec::Mp3 { bitrate_kbps }
        }
        BridgeExportPresetCodec::OpusOgg { bitrate_kbps } => {
            bae_core::config::ExportPresetCodec::OpusOgg { bitrate_kbps }
        }
        BridgeExportPresetCodec::Wav { bit_depth } => bae_core::config::ExportPresetCodec::Wav {
            bit_depth: core_export_bit_depth(bit_depth),
        },
        BridgeExportPresetCodec::Aiff { bit_depth } => bae_core::config::ExportPresetCodec::Aiff {
            bit_depth: core_export_bit_depth(bit_depth),
        },
    }
}

pub(crate) fn bridge_export_pregap_placement(
    placement: bae_core::config::ExportPregapPlacement,
) -> BridgeExportPregapPlacement {
    match placement {
        bae_core::config::ExportPregapPlacement::AppendToPreviousExceptHtoa => {
            BridgeExportPregapPlacement::AppendToPreviousExceptHtoa
        }
        bae_core::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa => {
            BridgeExportPregapPlacement::AppendToPreviousIncludingHtoa
        }
        bae_core::config::ExportPregapPlacement::Exclude => BridgeExportPregapPlacement::Exclude,
        bae_core::config::ExportPregapPlacement::SingleFileWithCue => {
            BridgeExportPregapPlacement::SingleFileWithCue
        }
    }
}

pub(crate) fn core_export_pregap_placement(
    placement: BridgeExportPregapPlacement,
) -> bae_core::config::ExportPregapPlacement {
    match placement {
        BridgeExportPregapPlacement::AppendToPreviousExceptHtoa => {
            bae_core::config::ExportPregapPlacement::AppendToPreviousExceptHtoa
        }
        BridgeExportPregapPlacement::AppendToPreviousIncludingHtoa => {
            bae_core::config::ExportPregapPlacement::AppendToPreviousIncludingHtoa
        }
        BridgeExportPregapPlacement::Exclude => bae_core::config::ExportPregapPlacement::Exclude,
        BridgeExportPregapPlacement::SingleFileWithCue => {
            bae_core::config::ExportPregapPlacement::SingleFileWithCue
        }
    }
}

pub(crate) fn bridge_export_selection(
    selection: &bae_core::config::ExportSelection,
) -> BridgeExportSelection {
    match selection {
        bae_core::config::ExportSelection::Original => BridgeExportSelection::Original,
        bae_core::config::ExportSelection::Preset { preset_id } => BridgeExportSelection::Preset {
            preset_id: preset_id.clone(),
        },
    }
}

pub(crate) fn core_export_selection(
    selection: BridgeExportSelection,
) -> bae_core::config::ExportSelection {
    match selection {
        BridgeExportSelection::Original => bae_core::config::ExportSelection::Original,
        BridgeExportSelection::Preset { preset_id } => {
            bae_core::config::ExportSelection::Preset { preset_id }
        }
    }
}

pub(crate) fn bridge_export_preset(preset: &bae_core::config::ExportPreset) -> BridgeExportPreset {
    BridgeExportPreset {
        id: preset.id.clone(),
        name: preset.name.clone(),
        codec: bridge_export_preset_codec(&preset.codec),
        extension: preset.codec.extension().to_string(),
        filename_template: preset.filename_template.clone(),
        metadata: bridge_export_metadata(&preset.metadata),
        pregap_placement: bridge_export_pregap_placement(preset.pregap_placement),
        applies_to_track: preset.applies_to_track,
        applies_to_release: preset.applies_to_release,
    }
}

pub(crate) fn core_export_preset(preset: BridgeExportPreset) -> bae_core::config::ExportPreset {
    bae_core::config::ExportPreset {
        id: preset.id,
        name: preset.name,
        codec: core_export_preset_codec(preset.codec),
        filename_template: preset.filename_template,
        metadata: core_export_metadata(preset.metadata),
        pregap_placement: core_export_pregap_placement(preset.pregap_placement),
        applies_to_track: preset.applies_to_track,
        applies_to_release: preset.applies_to_release,
    }
}

/// Convert a core `Config` to `BridgeConfig` for the UI. Pure translation —
/// `cloud_account_display` is a core method; this just reads it. `sync` is
/// `Some` whenever a provider is set in YAML. Sync-loop running status is
/// runtime state, not config — it rides `BridgeUiEvent::ConfigChanged`
/// separately, not on `BridgeConfig`.
pub(crate) fn build_bridge_config(config: &bae_core::config::Config) -> BridgeConfig {
    let discogs_status = config.discogs_token_status();
    BridgeConfig {
        library_id: config.library_id.clone(),
        library_name: config.library_name.clone(),
        library_path: config.library_dir.to_string_lossy().to_string(),
        encryption_key_stored: config.encryption_key_stored,
        encryption_key_fingerprint: config.encryption_key_fingerprint.clone(),
        pause_between_sides: config.pause_between_sides,
        export_location: bridge_export_location(&config.export_location),
        export_filename_template: config.export_filename_template.clone(),
        export_metadata: bridge_export_metadata(&config.export_metadata),
        export_presets: config
            .export_presets
            .iter()
            .map(bridge_export_preset)
            .collect(),
        default_track_export_selection: bridge_export_selection(
            &config.default_track_export_selection,
        ),
        default_release_export_selection: bridge_export_selection(
            &config.default_release_export_selection,
        ),
        mcp: BridgeMcpConfig {
            enabled: config.mcp.enabled,
            port: config.mcp.port,
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
            bae_core::config::DiscogsTokenStatus::Rejected => BridgeDiscogsTokenStatus::Rejected,
        },
        sync: config
            .cloud_home
            .provider
            .as_ref()
            .map(|provider| BridgeSyncConfig {
                provider: bridge_sync_provider(provider, &config.cloud_home),
                cloud_account_display: config.cloud_account_display(),
            }),
    }
}

/// Map a connected provider + its cloud-home settings to the bridge enum,
/// carrying only the fields that provider uses.
fn bridge_sync_provider(
    provider: &bae_core::config::CloudProvider,
    cloud_home: &bae_core::config::CloudHomeConfig,
) -> BridgeSyncProvider {
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

/// Track count derived from categorized files.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn extract_track_count(
    files: &bae_core::import::folder_scanner::CategorizedFiles,
) -> u32 {
    files.audio.track_count()
}

#[cfg(test)]
mod tests {
    use super::{bridge_export_metadata, core_export_metadata};
    use bae_core::config::ExportMetadata;

    /// Round-tripping the metadata selection through the bridge record and back
    /// preserves every field, so a toggle set in the UI reaches core unchanged.
    #[test]
    fn export_metadata_roundtrips_field_for_field() {
        let original = ExportMetadata {
            title: true,
            artist: false,
            album: true,
            year: false,
            track_number: true,
            disc_number: false,
            cover_art: true,
        };
        assert_eq!(
            core_export_metadata(bridge_export_metadata(&original)),
            original
        );
    }
}
