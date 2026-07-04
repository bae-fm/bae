pub mod app_services;
pub mod download_snapshot;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod export;
pub mod export_snapshot;
pub mod manager;
pub mod outbox_snapshot;
pub mod release_queue;
pub(crate) mod sync_controller;
pub mod sync_events;
pub mod upload_throughput;
pub use app_services::*;
pub use download_snapshot::{DownloadOp, DownloadProgress, DownloadSnapshot, DownloadState};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use export::ExportFormat;
pub use export_snapshot::{ExportOp, ExportProgress, ExportSnapshot, ExportState};
pub use manager::*;
pub use outbox_snapshot::{
    DeleteOp, OutboxSnapshot, UploadActivity, UploadProgress, UploadReleaseGroup, UploadState,
};
pub use release_queue::ReleaseQueue;
pub use upload_throughput::UploadThroughput;

use crate::config::{Config, ConfigError};
use crate::keys::KeyService;
use coven::LibraryDir;
use coven::{EncryptionError, EncryptionService};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

pub use tokio_util::sync::CancellationToken;

pub type DownloadQueue = ReleaseQueue<(), ()>;
pub type ExportQueue = ReleaseQueue<PathBuf, u8>;

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

struct CodeOperationCancel {
    token: CancellationToken,
    library_dir: PathBuf,
    library_dir_existed: bool,
}

/// Create a new library with an optional name, and set it as active.
///
/// Generates a UUID, optional random name, creates the directory, and
/// writes the active-library marker.
pub fn create_library_default(ids: &dyn coven::IdProvider) -> Result<Config, ConfigError> {
    create_library(crate::library_name::generate_library_name(), ids)
}

pub fn create_library(name: String, ids: &dyn coven::IdProvider) -> Result<Config, ConfigError> {
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

/// coven's restore/join returns the recovered Config; wrap it in bae's Config
/// (which adds Discogs fields) and persist it.
fn save_coven_library(coven_config: coven::Config) -> Result<Config, String> {
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
    source: crate::sync::RestoreSource,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    let app_dir = crate::config::bae_dir().map_err(|e| e.to_string())?;
    // The restoring device signs its control objects during restore with its own
    // identity. Get-or-create the device keypair under the library being restored,
    // mirroring the keyring identity coven imports on the restore-code path.
    let keypair = KeyService::new(library_id.to_string())
        .get_or_create_user_keypair()
        .map_err(|e| e.to_string())?;
    // This restore-with-a-key path is for an opaque home: the caller supplies the
    // library key, so coven rebuilds the encrypted, obfuscated home from its
    // presence (`Some`). A browsable home has no key and restores through the
    // restore-code path instead, where the absent `ek` selects it.
    let coven_config = crate::sync::restore_from_cloud(
        library_id,
        Some(encryption_key_hex),
        library_name,
        &crate::sync::synced_tables(),
        &crate::migrations::all(),
        source,
        &keypair,
        &app_dir,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
    )
    .await
    .map_err(|e| e.to_string())?;
    save_coven_library(coven_config)
}

/// Restore a library from a restore code. Wraps coven's `restore_from_code`.
pub async fn restore_from_code(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    restore_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, None, on_status)
        .await
        .map_err(|e| e.to_string())
}

pub async fn restore_from_code_cancellable(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    restore_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, Some(cancel), on_status).await
}

async fn restore_from_code_with_cancel(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    let app_dir = crate::config::bae_dir().map_err(restore_error)?;
    let cancel = prepare_code_operation_cancel(
        code,
        &app_dir,
        cancel,
        |code| {
            crate::sync::decode_restore_code_info(code)
                .map(|info| info.library_id)
                .map_err(|e| e.to_string())
        },
        restore_error,
    )?;
    let synced_tables = crate::sync::synced_tables();
    let migrations = crate::migrations::all();

    let restore = crate::sync::restore_from_code(
        code,
        &synced_tables,
        &migrations,
        oauth_tokens,
        cloudkit_ops,
        &app_dir,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
    );
    run_code_operation(
        restore,
        cancel,
        restore_error,
        RestoreFromCodeError::Restore,
        || RestoreFromCodeError::Cancelled,
    )
    .await
}

/// Remove the library directory a cancelled restore/join left partially built.
/// Only called when the directory did not exist before the operation, so this
/// never deletes a pre-existing library.
fn remove_cancelled_library_dir(library_dir: &std::path::Path) {
    match std::fs::remove_dir_all(library_dir) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => debug!(
            path = %library_dir.display(),
            "cancelled library directory already absent"
        ),
        Err(e) => warn!(
            path = %library_dir.display(),
            "failed to remove cancelled library directory: {e}"
        ),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum JoinFromCodeError {
    #[error("join cancelled")]
    Cancelled,
    #[error("{0}")]
    Join(String),
}

fn join_error(error: impl ToString) -> JoinFromCodeError {
    JoinFromCodeError::Join(error.to_string())
}

/// Join a shared library from an invite code. Wraps coven's
/// `join_from_invite_code`, supplying bae's app dir, clock, id source, and blob
/// plan. For an OAuth provider the caller fetches `oauth_tokens` first (the
/// joining device authorizes its own cloud account), exactly as restore does.
pub async fn join_from_code(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    join_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, None, on_status)
        .await
        .map_err(|e| e.to_string())
}

pub async fn join_from_code_cancellable(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    join_from_code_with_cancel(code, oauth_tokens, cloudkit_ops, Some(cancel), on_status).await
}

async fn join_from_code_with_cancel(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    let app_dir = crate::config::bae_dir().map_err(join_error)?;
    let cancel = prepare_code_operation_cancel(
        code,
        &app_dir,
        cancel,
        |code| {
            crate::sync::decode_invite_code_info(code)
                .map(|info| info.library_id)
                .map_err(|e| e.to_string())
        },
        join_error,
    )?;
    let synced_tables = crate::sync::synced_tables();
    let migrations = crate::migrations::all();

    let join = crate::sync::join_from_invite_code(
        code,
        &app_dir,
        &synced_tables,
        &migrations,
        oauth_tokens,
        cloudkit_ops,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
    );
    run_code_operation(join, cancel, join_error, JoinFromCodeError::Join, || {
        JoinFromCodeError::Cancelled
    })
    .await
}

fn prepare_code_operation_cancel<E>(
    code: &str,
    app_dir: &Path,
    cancel: Option<CancellationToken>,
    decode_library_id: impl FnOnce(&str) -> Result<String, String>,
    error: impl Fn(String) -> E,
) -> Result<Option<CodeOperationCancel>, E> {
    let Some(token) = cancel else {
        return Ok(None);
    };

    let library_id = decode_library_id(code).map_err(&error)?;
    let library_dir = library_dir_path(app_dir, &library_id);
    let library_dir_existed = library_dir.try_exists().map_err(|e| error(e.to_string()))?;
    Ok(Some(CodeOperationCancel {
        token,
        library_dir,
        library_dir_existed,
    }))
}

async fn run_code_operation<E, OpError, Operation>(
    operation: Operation,
    cancel: Option<CodeOperationCancel>,
    operation_error: impl Fn(OpError) -> E,
    save_error: impl Fn(String) -> E,
    cancelled: impl Fn() -> E,
) -> Result<Config, E>
where
    Operation: Future<Output = Result<coven::Config, OpError>>,
{
    let coven_config = if let Some(cancel) = cancel {
        if cancel.token.is_cancelled() {
            return Err(cancel_code_operation(cancel, cancelled));
        }
        tokio::pin!(operation);
        tokio::select! {
            result = &mut operation => result.map_err(operation_error)?,
            _ = cancel.token.cancelled() => return Err(cancel_code_operation(cancel, cancelled)),
        }
    } else {
        operation.await.map_err(operation_error)?
    };

    save_coven_library(coven_config).map_err(save_error)
}

fn cancel_code_operation<E>(cancel: CodeOperationCancel, error: impl FnOnce() -> E) -> E {
    if !cancel.library_dir_existed {
        remove_cancelled_library_dir(&cancel.library_dir);
    }
    error()
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

    let libraries = Config::discover_libraries();
    let lib_info = libraries
        .into_iter()
        .find(|lib| lib.id == library_id)
        .ok_or_else(|| UnlockError::NotFound(library_id.to_string()))?;

    if let Some(ref stored_fp) = lib_info.encryption_key_fingerprint {
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
