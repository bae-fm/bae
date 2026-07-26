pub mod app_services;
pub mod download_snapshot;
pub mod manager;
pub mod outbox_snapshot;
pub mod output_snapshot;
pub mod release_queue;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod save;
pub mod search;
pub(crate) mod sync_controller;
pub mod sync_events;
pub mod upload_sessions;
pub mod upload_throughput;
pub use app_services::*;
pub use download_snapshot::{
    DownloadOp, DownloadProgress, DownloadSnapshot, DownloadState, DownloadTransferProgress,
};
pub use manager::*;
pub use outbox_snapshot::{
    DeleteOp, OutboxSnapshot, UploadActivity, UploadFileOp, UploadProgress, UploadReleaseGroup,
    UploadState,
};
pub use output_snapshot::{OutputKind, OutputOp, OutputProgress, OutputSnapshot, OutputState};
pub use release_queue::{CountLabel, ReleaseQueue};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use save::SaveService;
pub use search::{LibrarySearchQuery, SEARCH_RESULT_LIMIT};
/// How a device join this library invited ended. The controller itself stays
/// crate-private; this outcome is part of the public sharing surface.
pub use sync_controller::DeviceJoinOutcome;
pub use upload_sessions::UploadSessions;
pub use upload_throughput::UploadThroughput;

use crate::config::{Config, ConfigError};
use crate::keys::StoreKeys;
use coven::StoreDir;
use coven::{EncryptionError, MasterKeyring};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

pub type DownloadQueue = ReleaseQueue<(), DownloadTransferProgress>;
pub type OutputQueue = ReleaseQueue<output_snapshot::OutputRequest, u8>;

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

pub fn create_library(
    name: crate::library_name::LibraryName,
    ids: &dyn coven::IdProvider,
) -> Result<Config, ConfigError> {
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
    let config = Config::with_defaults(library_id, device_id, library_dir, name.into_string());
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
    /// The owner's device never took its next step in the handshake before the
    /// transport's deadline. Both devices have to be running the join at the same
    /// time, so this is the "open bae on the other device and try again" state
    /// rather than a fault in the scanned code.
    #[error("the inviting device is not running the join")]
    OwnerOffline,
    /// The owner withdrew the attempt before it completed.
    #[error("the inviting device ended the join")]
    Abandoned,
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

/// Join a shared library from an invite the owner's device displayed as a QR
/// code. Wraps coven's `join_with_scanned_invite`, supplying bae's app dir,
/// clock, id source, and blob plan. For an OAuth provider the caller fetches
/// `oauth_tokens` first (the joining device authorizes its own cloud account),
/// exactly as restore does.
///
/// `scanned` is the payload bytes read off the QR: it carries both the invite
/// code — which is where this device's provider credentials come from — and the
/// owner's signed join offer with the storage slots the two devices exchange the
/// rest of the handshake through.
///
/// `join_request_code` is the code this device's own
/// [`crate::sync::membership::generate_join_request`] produced when it asked to
/// join, and which the owner scanned/received to mint the invite: coven minted a
/// pending identity for it at that point, and a completed join promotes that same
/// identity into this store's own identity custody, so the caller must keep and
/// pass back the exact code it generated.
///
/// The owner's device must be running its side of the handshake while this runs.
/// If it never appears, this fails with [`JoinFromCodeError::OwnerOffline`].
pub async fn join_from_scanned_bundle(
    scanned: &[u8],
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    join_from_scanned_bundle_inner(
        scanned,
        join_request_code,
        oauth_tokens,
        cloudkit_ops,
        None,
        on_status,
    )
    .await
}

pub async fn join_from_scanned_bundle_cancellable(
    scanned: &[u8],
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    join_from_scanned_bundle_inner(
        scanned,
        join_request_code,
        oauth_tokens,
        cloudkit_ops,
        Some(cancel),
        on_status,
    )
    .await
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
        // No custom-S3 exact-slot assertion: the provider comes from the restore
        // code, and bae exposes no operator toggle asserting a non-AWS endpoint
        // supports conditional requests.
        None,
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

async fn join_from_scanned_bundle_inner(
    scanned: &[u8],
    join_request_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, JoinFromCodeError> {
    let app_dir = crate::config::bae_dir().map_err(|e| JoinFromCodeError::Join(e.to_string()))?;
    let (rx, bridge) = cancel_receiver(cancel);
    // Same default custody choice as `restore_from_code_inner` above.
    let result = crate::sync::join_with_scanned_invite(
        scanned,
        join_request_code,
        library_layout(app_dir),
        crate::sync::synced_tables(),
        crate::migrations::all(),
        // No custom-S3 exact-slot assertion, as on the restore path: the provider
        // comes from the scanned invite and bae exposes no operator toggle.
        None,
        coven::KeyCustody::Keyring,
        coven::IdentityCustody::Keyring,
        oauth_tokens,
        cloudkit_ops,
        std::sync::Arc::new(coven::SystemClock),
        crate::sync::device_join_timing(),
        on_status,
        &rx,
    )
    .await;
    if let Some(handle) = bridge {
        handle.abort();
    }
    match result {
        Ok(coven::DeviceJoinTransportOutcome::Joined(coven_config)) => {
            save_coven_library(coven_config).map_err(JoinFromCodeError::Join)
        }
        // The owner gave up on this attempt before it completed. Not a failure of
        // this device — a distinct end the UI reports as such.
        Ok(coven::DeviceJoinTransportOutcome::Abandoned(_)) => Err(JoinFromCodeError::Abandoned),
        Err(coven::BootstrapError::Cancelled) => Err(JoinFromCodeError::Cancelled),
        Err(error) => Err(classify_join_error(error)),
    }
}

/// Map coven's bootstrap failure onto bae's join outcome. A transport wait that
/// ran out its deadline means the owner's device never took its next step, which
/// the UI reports as "the other device has to be open", not as an internal error.
fn classify_join_error(error: coven::BootstrapError) -> JoinFromCodeError {
    if let coven::BootstrapError::DeviceJoinTransport(coven::DeviceJoinTransportError::Timeout {
        ..
    }) = &error
    {
        return JoinFromCodeError::OwnerOffline;
    }
    JoinFromCodeError::Join(error.to_string())
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
    // `MasterKeyring::from_serialized` parses coven's stored keyring format and
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
        // The stored master key is coven's keyring format, not a bare hex key —
        // a keyring carries every generation, so it is what both the unlock's
        // validation and the keyring entry round-trip through.
        let stored_key = MasterKeyring::generate().to_serialized();
        let bae_dir = tmp.path().join(".bae");
        let library_path = crate::config::registered_library_path(&bae_dir, library_id);
        let mut config = Config::with_defaults(
            library_id.to_string(),
            "device-id".to_string(),
            StoreDir::new(library_path),
            "Library Name".to_string(),
        );
        config.encryption_key_fingerprint = Some(
            MasterKeyring::from_serialized(&stored_key)
                .unwrap()
                .fingerprint(),
        );
        config.save_to_config_yaml().unwrap();
        std::fs::create_dir(bae_dir.join("active-library")).unwrap();

        unlock_library_in_dir(
            &bae_dir,
            library_id,
            &stored_key,
            &coven::SequentialIdProvider::new("generated-device"),
        )
        .unwrap();

        let stored = StoreKeys::new(library_id.to_string())
            .get_encryption_key()
            .unwrap();
        assert_eq!(stored.as_deref(), Some(stored_key.as_str()));
    }
}
