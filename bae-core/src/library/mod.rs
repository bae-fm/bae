pub mod app_services;
pub mod download_queue;
pub mod download_snapshot;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod export;
pub mod manager;
pub mod outbox_snapshot;
pub mod sync_events;
pub mod upload_throughput;
pub mod versioned_image_path;
pub use app_services::*;
pub use download_queue::DownloadQueue;
pub use download_snapshot::{DownloadOp, DownloadProgress, DownloadSnapshot, DownloadState};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use export::{ExportFormat, MP3_EXPORT_BITRATE};
pub use manager::*;
pub use outbox_snapshot::{DeleteOp, OutboxSnapshot, UploadOp, UploadProgress, UploadState};
pub use upload_throughput::UploadThroughput;

use crate::config::{Config, ConfigError, ConfigYaml};
use crate::encryption::{EncryptionError, EncryptionService};
use crate::keys::KeyService;
use crate::library_dir::LibraryDir;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

pub use tokio_util::sync::CancellationToken;

#[derive(Debug, thiserror::Error)]
pub enum RestoreFromCodeError {
    #[error("restore cancelled")]
    Cancelled,
    #[error("{0}")]
    Restore(String),
}

fn library_dir_path(app_dir: &Path, library_id: &str) -> PathBuf {
    app_dir.join("libraries").join(library_id)
}

fn restore_error(error: impl ToString) -> RestoreFromCodeError {
    RestoreFromCodeError::Restore(error.to_string())
}

/// Create a new library with an optional name, and set it as active.
///
/// Generates a UUID, optional random name, creates the directory, and
/// writes the active-library marker.
pub fn create_library_default(
    ids: &dyn crate::id_provider::IdProvider,
) -> Result<Config, ConfigError> {
    create_library(crate::library_name::generate_library_name(), ids)
}

pub fn create_library(
    name: String,
    ids: &dyn crate::id_provider::IdProvider,
) -> Result<Config, ConfigError> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Failed to get home directory",
        ))
    })?;
    let bae_dir = home_dir.join(".bae");
    let library_id = ids.new_id();

    // coven's LibraryDir::create returns coven's Config; bae's Config adds its own
    // fields (Discogs), so build and persist the bae one here.
    let library_dir = LibraryDir::new(library_dir_path(&bae_dir, &library_id));
    std::fs::create_dir_all(&*library_dir)?;
    let device_id = ids.new_id();
    let config = Config::with_defaults(library_id, device_id, library_dir, name);
    config.save_to_config_yaml()?;
    config.save_active_library()?;
    Ok(config)
}

/// The blob plan coven binds to a freshly created/restored library dir.
fn make_blob_plan(library_dir: &LibraryDir) -> Box<dyn coven::blob::BlobPlan> {
    Box::new(crate::sync::blob_plan::BaeBlobPlan::new(
        library_dir.clone(),
    ))
}

/// coven's restore returns the recovered Config; wrap it in bae's Config
/// (which adds Discogs fields) and persist it.
fn save_restored(coven_config: coven::config::Config) -> Result<Config, String> {
    let config = Config::from_coven(coven_config);
    config.save_to_config_yaml().map_err(|e| e.to_string())?;
    Ok(config)
}

/// Restore a library from cloud storage into bae's app dir. Wraps coven's
/// `restore_from_cloud`, supplying bae's app dir, clock, id source, and blob plan.
pub async fn restore_from_cloud(
    library_id: &str,
    encryption_key_hex: &str,
    library_name: &str,
    source: crate::sync::restore::RestoreSource,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    let app_dir = crate::config::bae_dir().map_err(|e| e.to_string())?;
    // bae homes are always encrypted (a key is always present) and always
    // obfuscate their blob paths, so the key is `Some` and `obfuscate_blob_paths`
    // is `true`.
    let coven_config = crate::sync::restore::restore_from_cloud(
        library_id,
        Some(encryption_key_hex),
        true,
        library_name,
        &crate::sync::synced_tables(),
        source,
        &app_dir,
        std::sync::Arc::new(crate::clock::SystemClock),
        std::sync::Arc::new(crate::id_provider::UuidProvider),
        make_blob_plan,
        on_status,
    )
    .await
    .map_err(|e| e.to_string())?;
    save_restored(coven_config)
}

/// Restore a library from a restore code. Wraps coven's `restore_from_code`.
pub async fn restore_from_code(
    code: &str,
    oauth_tokens: Option<crate::oauth::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn crate::storage::cloud::cloudkit::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    restore_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, None, on_status)
        .await
        .map_err(|e| e.to_string())
}

pub async fn restore_from_code_cancellable(
    code: &str,
    oauth_tokens: Option<crate::oauth::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn crate::storage::cloud::cloudkit::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    restore_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, Some(cancel), on_status).await
}

async fn restore_from_code_with_cancel(
    code: &str,
    oauth_tokens: Option<crate::oauth::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn crate::storage::cloud::cloudkit::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    let app_dir = crate::config::bae_dir().map_err(restore_error)?;
    let cancel = if let Some(cancel) = cancel {
        let info =
            crate::sync::restore_code::decode_restore_code_info(code).map_err(restore_error)?;
        let library_dir = library_dir_path(&app_dir, &info.library_id);
        let library_dir_existed = library_dir.try_exists().map_err(restore_error)?;
        Some((cancel, library_dir, library_dir_existed))
    } else {
        None
    };
    let synced_tables = crate::sync::synced_tables();

    let restore = crate::sync::restore::restore_from_code(
        code,
        &synced_tables,
        oauth_tokens,
        cloudkit_ops,
        &app_dir,
        std::sync::Arc::new(crate::clock::SystemClock),
        std::sync::Arc::new(crate::id_provider::UuidProvider),
        make_blob_plan,
        on_status,
    );
    let coven_config = if let Some((cancel, library_dir, library_dir_existed)) = cancel {
        if cancel.is_cancelled() {
            return Err(cancelled_restore(&library_dir, library_dir_existed));
        }
        tokio::pin!(restore);
        tokio::select! {
            result = &mut restore => result.map_err(restore_error)?,
            _ = cancel.cancelled() => return Err(cancelled_restore(&library_dir, library_dir_existed)),
        }
    } else {
        restore.await.map_err(restore_error)?
    };

    save_restored(coven_config).map_err(RestoreFromCodeError::Restore)
}

fn cancelled_restore(
    library_dir: &std::path::Path,
    library_dir_existed: bool,
) -> RestoreFromCodeError {
    if !library_dir_existed {
        remove_cancelled_restore_dir(library_dir);
    }
    RestoreFromCodeError::Cancelled
}

fn remove_cancelled_restore_dir(library_dir: &std::path::Path) {
    match std::fs::remove_dir_all(library_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => debug!(
            path = %library_dir.display(),
            "cancelled restore directory already absent"
        ),
        Err(e) => warn!(
            path = %library_dir.display(),
            "failed to remove cancelled restore directory: {e}"
        ),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum UnlockError {
    #[error("{0}")]
    Validation(String),
    #[error("library not found: {0}")]
    NotFound(String),
    #[error("config: {0}")]
    Config(String),
    #[error("encryption: {0}")]
    Encryption(#[from] EncryptionError),
    #[error("keyring: {0}")]
    Key(#[from] crate::keys::KeyError),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

/// Unlock a library by validating the encryption key against the stored
/// fingerprint, then saving it to the keyring.
pub fn unlock_library(library_id: &str, key_hex: &str) -> Result<(), UnlockError> {
    // `EncryptionService::new` performs the hex+length validation and returns
    // `EncryptionError::KeyManagement` with the specific cause, so a malformed
    // key surfaces as `UnlockError::Encryption` rather than collapsing into
    // "no fingerprint computed".
    let fingerprint = EncryptionService::new(key_hex)?.fingerprint();

    // Load config for this library to get the stored fingerprint
    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| UnlockError::NotFound(library_id.to_string()))?;

    // Read and parse config.yaml to get the stored fingerprint
    let config_path = lib_info.path.join("config.yaml");
    let config_str = std::fs::read_to_string(&config_path)?;
    let yaml_config: ConfigYaml =
        serde_yaml::from_str(&config_str).map_err(|e| UnlockError::Config(e.to_string()))?;

    if let Some(ref stored_fp) = yaml_config.encryption_key_fingerprint {
        if *stored_fp != fingerprint {
            return Err(UnlockError::Validation(
                "Encryption key fingerprint mismatch".to_string(),
            ));
        }
    }

    // Save key to keyring
    let key_service = KeyService::new(library_id.to_string());
    key_service.set_encryption_key(key_hex)?;

    Ok(())
}
