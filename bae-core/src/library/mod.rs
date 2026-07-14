pub mod app_services;
pub mod download_snapshot;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod export;
pub mod export_snapshot;
pub mod manager;
pub mod outbox_snapshot;
pub mod release_queue;
pub mod search;
pub(crate) mod sync_controller;
pub mod sync_events;
pub mod upload_sessions;
pub mod upload_throughput;
pub use app_services::*;
pub use download_snapshot::{
    DownloadOp, DownloadProgress, DownloadSnapshot, DownloadState, DownloadTransferProgress,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use export::ExportService;
pub use export_snapshot::{ExportOp, ExportProgress, ExportSnapshot, ExportState};
pub use manager::*;
pub use outbox_snapshot::{
    DeleteOp, OutboxSnapshot, UploadActivity, UploadFileOp, UploadProgress, UploadReleaseGroup,
    UploadState,
};
pub use release_queue::ReleaseQueue;
pub use search::{LibrarySearchQuery, SEARCH_RESULT_LIMIT};
pub use upload_sessions::UploadSessions;
pub use upload_throughput::UploadThroughput;

use crate::config::{Config, ConfigError};
use crate::keys::StoreKeys;
use coven::StoreDir;
use coven::{EncryptionError, MasterKeyring};
use coven::{IdentityCustody, KeyCustody, UserKeypair};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

pub type DownloadQueue = ReleaseQueue<(), DownloadTransferProgress>;
pub type ExportQueue = ReleaseQueue<export_snapshot::ExportRequest, u8>;

#[derive(Debug, thiserror::Error)]
pub enum RestoreFromCodeError {
    #[error("restore cancelled")]
    Cancelled,
    #[error("{0}")]
    Restore(String),
}

#[derive(Debug, thiserror::Error)]
enum LibraryCodeOperationError {
    #[error("operation cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

impl From<LibraryCodeOperationError> for RestoreFromCodeError {
    fn from(error: LibraryCodeOperationError) -> Self {
        match error {
            LibraryCodeOperationError::Cancelled => RestoreFromCodeError::Cancelled,
            LibraryCodeOperationError::Failed(error) => RestoreFromCodeError::Restore(error),
        }
    }
}

/// Create a library under a generated name and make it active.
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

    // coven's StoreDir::create returns coven's Config; bae's Config adds its own
    // fields (Discogs), so build and persist the bae one here.
    let library_dir = StoreDir::new(crate::config::registered_library_path(
        &bae_dir,
        &library_id,
    ));
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

/// bae's on-disk layout for coven stores: libraries live under
/// `<bae_dir>/libraries/<id>/store.db`, the same place `create_library` and
/// discovery use. coven's default is `stores/<id>`, so join/restore are told
/// bae's `libraries/` name here rather than landing in a directory bae never
/// scans.
fn library_layout(bae_dir: impl Into<std::path::PathBuf>) -> coven::StoreLayout {
    coven::StoreLayout::new(bae_dir).stores_dirname("libraries")
}

/// Bridge bae's `CancellationToken` onto the `watch::Receiver<bool>` that coven's
/// cancellable operations (join/restore, make-Local) poll at phase boundaries. The
/// channel is seeded with the token's current state, so a token cancelled before
/// the bridge task runs is seen immediately. Abort the returned handle once the
/// operation finishes, so the bridge task doesn't linger.
pub(crate) fn cancel_token_to_watch(
    handle: &tokio::runtime::Handle,
    token: CancellationToken,
) -> (watch::Receiver<bool>, tokio::task::JoinHandle<()>) {
    let (tx, rx) = watch::channel(token.is_cancelled());
    let join = handle.spawn(async move {
        token.cancelled().await;
        if let Err(error) = tx.send(true) {
            tracing::debug!(?error, "cancellation watch receiver already dropped");
        }
    });
    (rx, join)
}

/// `None` yields a receiver whose sender is dropped, so it reads `false` forever
/// (never cancels) and spawns no bridge task. coven checks the receiver at phase
/// boundaries and, on cancel, removes the partial store directory it created — the
/// same cleanup a failure gets — so bae neither races the operation nor clears up
/// after it.
fn cancel_receiver(
    cancel: Option<CancellationToken>,
) -> (watch::Receiver<bool>, Option<tokio::task::JoinHandle<()>>) {
    match cancel {
        Some(token) => {
            let (rx, join) = cancel_token_to_watch(&tokio::runtime::Handle::current(), token);
            (rx, Some(join))
        }
        None => (watch::channel(false).1, None),
    }
}

/// Finish a code-driven join/restore: stop the cancel bridge, then map coven's
/// outcome. `BootstrapError::Cancelled` (coven cancelled cooperatively at a phase
/// boundary and already removed its partial store dir) becomes our `Cancelled`;
/// any other error is `Failed`; success persists bae's wrapped `Config`.
fn finish_code_operation(
    result: Result<coven::Config, coven::BootstrapError>,
    bridge: Option<tokio::task::JoinHandle<()>>,
) -> Result<Config, LibraryCodeOperationError> {
    if let Some(handle) = bridge {
        handle.abort();
    }
    match result {
        Ok(coven_config) => {
            save_coven_library(coven_config).map_err(LibraryCodeOperationError::Failed)
        }
        Err(coven::BootstrapError::Cancelled) => Err(LibraryCodeOperationError::Cancelled),
        Err(e) => Err(LibraryCodeOperationError::Failed(e.to_string())),
    }
}

/// Restore a library from cloud storage into bae's app dir. Wraps coven's
/// `restore_from_cloud`, supplying bae's layout, clock, id source, and blob plan.
pub async fn restore_from_cloud(
    library_id: &str,
    encryption_key_hex: &str,
    library_name: &str,
    source: crate::sync::RestoreSource,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    let app_dir = crate::config::bae_dir().map_err(|e| e.to_string())?;
    let layout = library_layout(app_dir);
    let store_dir = layout.store_dir(library_id);

    // The restoring device signs its control objects during restore with THIS
    // store's signing identity (per (store, device), not device-global). Reuse
    // it if already established for this library id (a retried restore after an
    // earlier attempt failed past coven's own already-exists guard would find
    // one; a fresh library id never does) — else mint one now, before the
    // network call, so a bootstrap failure's own cleanup (which owns this
    // `identity_custody`) forgets it too, leaving nothing for bae to roll back
    // itself. (The restore-code path instead imports the code's own key, since
    // it recovers a device that already held one; this is the
    // caller-supplies-the-encryption-key path, which has no such key to
    // recover.)
    let identity_custody = IdentityCustody::Keyring.resolve(library_id, &store_dir);
    let keypair = match identity_custody.unlock().map_err(|e| e.to_string())? {
        Some(existing) => existing,
        None => {
            let fresh = UserKeypair::generate();
            identity_custody
                .persist(&fresh)
                .map_err(|e| e.to_string())?;
            fresh
        }
    };
    let key_custody = KeyCustody::Keyring.resolve(library_id, &store_dir);

    // This restore-with-a-key path is for an opaque home: the caller supplies the
    // library key, so coven rebuilds the encrypted, obfuscated home from its
    // presence (`Some`). A browsable home has no key and restores through the
    // restore-code path instead, where the absent `ek` selects it. This path is
    // not cancellable, so it passes a never-firing cancel receiver.
    let (cancel, _bridge) = cancel_receiver(None);
    let coven_config = crate::sync::restore_from_cloud(
        library_id,
        Some(encryption_key_hex),
        library_name,
        &crate::sync::synced_tables(),
        &crate::migrations::all(),
        key_custody,
        identity_custody,
        source,
        // No restore code here (the caller supplies the encryption key
        // directly), so there is no membership floor minted at code-generation
        // time to seed from — this recovery path accepts whatever membership
        // head the storage serves, the same trust-on-first-use restore already
        // applies when adopting the store owner from the chain founder.
        &[],
        &keypair,
        &layout,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
        &cancel,
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
    restore_from_code_inner(code, oauth_tokens, cloudkit_ops, None, on_status)
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
    restore_from_code_inner(code, oauth_tokens, cloudkit_ops, Some(cancel), on_status)
        .await
        .map_err(RestoreFromCodeError::from)
}

#[derive(Debug, thiserror::Error)]
pub enum JoinFromCodeError {
    #[error("join cancelled")]
    Cancelled,
    #[error("{0}")]
    Join(String),
}

impl From<LibraryCodeOperationError> for JoinFromCodeError {
    fn from(error: LibraryCodeOperationError) -> Self {
        match error {
            LibraryCodeOperationError::Cancelled => JoinFromCodeError::Cancelled,
            LibraryCodeOperationError::Failed(error) => JoinFromCodeError::Join(error),
        }
    }
}

/// Join a shared library from an invite code. Wraps coven's
/// `join_from_invite_code`, supplying bae's app dir, clock, id source, and blob
/// plan. For an OAuth provider the caller fetches `oauth_tokens` first (the
/// joining device authorizes its own cloud account), exactly as restore does.
///
/// `join_request_code` is the code this device's own
/// [`crate::sync::membership::generate_join_request`] produced when it asked to
/// join — coven minted a pending identity for it at that point, and a
/// completed join promotes that same identity into this store's own identity
/// custody, so the caller must keep and pass back the exact code it generated.
pub async fn join_from_code(
    invite_code: &str,
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, String> {
    join_from_code_inner(
        invite_code,
        join_request_code,
        oauth_tokens,
        cloudkit_ops,
        None,
        on_status,
    )
    .await
    .map_err(|e| e.to_string())
}

pub async fn join_from_code_cancellable(
    invite_code: &str,
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    join_from_code_inner(
        invite_code,
        join_request_code,
        oauth_tokens,
        cloudkit_ops,
        Some(cancel),
        on_status,
    )
    .await
    .map_err(JoinFromCodeError::from)
}

async fn restore_from_code_inner(
    code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, LibraryCodeOperationError> {
    let app_dir =
        crate::config::bae_dir().map_err(|e| LibraryCodeOperationError::Failed(e.to_string()))?;
    let (rx, bridge) = cancel_receiver(cancel);
    // The default custody for both the master key and this device's identity —
    // the OS keyring, mirroring what `Coven::builder` itself defaults to for a
    // library opened the ordinary way (bae never overrides either).
    let result = crate::sync::restore_from_code(
        code,
        &crate::sync::synced_tables(),
        &crate::migrations::all(),
        coven::KeyCustody::Keyring,
        coven::IdentityCustody::Keyring,
        oauth_tokens,
        cloudkit_ops,
        &library_layout(app_dir),
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
        &rx,
    )
    .await;
    finish_code_operation(result, bridge)
}

async fn join_from_code_inner(
    invite_code: &str,
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, LibraryCodeOperationError> {
    let app_dir =
        crate::config::bae_dir().map_err(|e| LibraryCodeOperationError::Failed(e.to_string()))?;
    let (rx, bridge) = cancel_receiver(cancel);
    // Same default custody choice as `restore_from_code_inner` above.
    let result = crate::sync::join_from_invite_code(
        invite_code,
        join_request_code,
        &library_layout(app_dir),
        &crate::sync::synced_tables(),
        &crate::migrations::all(),
        coven::KeyCustody::Keyring,
        coven::IdentityCustody::Keyring,
        oauth_tokens,
        cloudkit_ops,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
        &rx,
    )
    .await;
    finish_code_operation(result, bridge)
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
}

/// Unlock a library by validating the encryption key against the stored
/// fingerprint, then saving it to the keyring.
pub fn unlock_library(library_id: &str, key_hex: &str) -> Result<(), UnlockError> {
    let bae_dir = crate::config::bae_dir().map_err(|e| UnlockError::Config(e.to_string()))?;
    unlock_library_in_dir(&bae_dir, library_id, key_hex, &coven::UuidProvider)
}

fn unlock_library_in_dir(
    bae_dir: &Path,
    library_id: &str,
    key_hex: &str,
    ids: &dyn coven::IdProvider,
) -> Result<(), UnlockError> {
    // `MasterKeyring::from_serialized` performs the hex+length validation and
    // returns `EncryptionError::KeyManagement` with the specific cause, so a
    // malformed key surfaces as `UnlockError::Encryption` rather than
    // collapsing into "no fingerprint computed". `MasterKeyring` (not
    // `EncryptionService`, which is test-only reachable from production code)
    // is coven's production-facing master-key value type — the same one
    // `CovenHandle::import_master_key` computes its own fingerprint from.
    let fingerprint = MasterKeyring::from_serialized(key_hex)?.fingerprint();

    let config =
        Config::load_registered_library_from_bae_dir(bae_dir, library_id, ids).map_err(|e| {
            if matches!(e, ConfigError::Io(ref io) if io.kind() == std::io::ErrorKind::NotFound) {
                UnlockError::NotFound(library_id.to_string())
            } else {
                UnlockError::Config(e.to_string())
            }
        })?;

    if let Some(ref stored_fp) = config.encryption_key_fingerprint {
        if *stored_fp != fingerprint {
            return Err(UnlockError::Validation(
                "Encryption key fingerprint mismatch".to_string(),
            ));
        }
    }

    let key_service = StoreKeys::new(library_id.to_string());
    key_service.set_encryption_key(key_hex)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn unlock_library_uses_requested_library_not_active_pointer() {
        crate::config::install_test_keyring();
        let tmp = TempDir::new().unwrap();
        let library_id = "library-to-unlock";
        let key_hex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let bae_dir = tmp.path().join(".bae");
        let library_path = crate::config::registered_library_path(&bae_dir, library_id);
        let mut config = Config::with_defaults(
            library_id.to_string(),
            "device-id".to_string(),
            StoreDir::new(library_path),
            "Library Name".to_string(),
        );
        config.encryption_key_fingerprint = Some(
            MasterKeyring::from_serialized(key_hex)
                .unwrap()
                .fingerprint(),
        );
        config.save_to_config_yaml().unwrap();
        std::fs::create_dir(bae_dir.join("active-library")).unwrap();

        unlock_library_in_dir(
            &bae_dir,
            library_id,
            key_hex,
            &coven::SequentialIdProvider::new("generated-device"),
        )
        .unwrap();

        let stored = StoreKeys::new(library_id.to_string())
            .get_encryption_key()
            .unwrap();
        assert_eq!(stored.as_deref(), Some(key_hex));
    }
}
