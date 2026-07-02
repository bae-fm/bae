use crate::types::{
    BridgeConfig, BridgeDiscogsTokenStatus, BridgeExportLocation, BridgeMcpConfig,
    BridgeSyncConfig, BridgeSyncProvider,
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
) -> Option<u32> {
    files.audio.track_count()
}
