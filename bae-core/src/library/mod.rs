pub mod app_services;
mod browse;
mod device_pairing;
pub mod download_snapshot;
mod local_lifecycle;
pub mod manager;
pub mod outbox_snapshot;
pub mod output_snapshot;
pub mod release_queue;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod save;
pub mod search;
pub(crate) mod sync_controller;
pub mod upload_throughput;
pub use app_services::*;
pub use browse::*;
pub use coven::{EagerCacheFillProgress, EagerCacheFillStatus};
pub use device_pairing::{
    inspect_device_pairing_offer, DevicePairingOfferInfo, DevicePairingSession, PairingDevice,
    PendingDevicePairingJoinInfo,
};
pub use download_snapshot::{
    DownloadOp, DownloadProgress, DownloadSnapshot, DownloadState, DownloadTransferProgress,
};
pub use local_lifecycle::remove_local_library;
pub use manager::*;
pub use outbox_snapshot::{
    DeleteOp, OutboxPauseState, OutboxSnapshot, UploadActivity, UploadBar, UploadFileLabel,
    UploadFileOp, UploadIssue, UploadPhase, UploadProgress, UploadReleaseGroup, UploadState,
};
pub use output_snapshot::{OutputKind, OutputOp, OutputProgress, OutputSnapshot, OutputState};
pub use release_queue::{CountLabel, ReleaseQueue};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use save::SaveService;
pub use search::{LibrarySearchQuery, SEARCH_RESULT_LIMIT};
/// How a device join this library invited ended. The controller itself stays
/// crate-private; this outcome is part of the public sharing surface.
pub use upload_throughput::UploadThroughput;

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod device_pairing_tests;
#[cfg(test)]
mod local_lifecycle_tests;

use crate::config::{Config, ConfigError};
use coven::StoreDir;
use std::sync::Arc;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

pub type DownloadQueue = ReleaseQueue<(), DownloadTransferProgress>;
pub type OutputQueue = ReleaseQueue<output_snapshot::OutputRequest, u8>;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub struct SaveTrackPlan {
    audio_buffers: Vec<SaveAudioBuffer>,
    resolved: manager::ResolvedSaveTags,
    cover_image_bytes: Option<Vec<u8>>,
    decode: crate::playback::stream_pipeline::StreamDecodeParams,
    audio_meta: manager::TrackAudioMeta,
}

#[cfg(all(
    feature = "test-utils",
    not(any(target_os = "ios", target_os = "android"))
))]
impl SaveTrackPlan {
    pub fn has_cover_image_for_test(&self) -> bool {
        self.cover_image_bytes.is_some()
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct SaveAudioBuffer {
    file_id: String,
    buffer: crate::playback::SharedSparseBuffer,
}

#[derive(Debug, thiserror::Error)]
pub enum RestoreFromCodeError {
    #[error("restore cancelled")]
    Cancelled,
    #[error("{0}")]
    Restore(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CreateLibraryError {
    #[error("library configuration: {0}")]
    Config(#[from] ConfigError),
    #[error("open new library: {0}")]
    Open(#[source] Box<coven::CovenError>),
    #[error("establish new library identity: {0}")]
    Identity(#[source] Box<coven::IdentityError>),
    #[error("{failure}; removing the partial library also failed: {rollback}")]
    Rollback {
        failure: Box<CreateLibraryError>,
        #[source]
        rollback: std::io::Error,
    },
}

impl CreateLibraryError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory;
        match self {
            Self::Config(_) => UiErrorCategory::Config,
            Self::Open(_) => UiErrorCategory::Database,
            Self::Identity(_) => UiErrorCategory::Keyring,
            Self::Rollback { failure, .. } => failure.category(),
        }
    }

    fn with_rollback(self, rollback: Result<(), std::io::Error>) -> Self {
        match rollback {
            Ok(()) => self,
            Err(rollback) => Self::Rollback {
                failure: Box::new(self),
                rollback,
            },
        }
    }
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

/// Create a library under a generated name and establish its device identity.
pub fn create_library_default(ids: &dyn coven::IdProvider) -> Result<Config, CreateLibraryError> {
    create_library(crate::library_name::generate_library_name(), ids)
}

pub fn create_library(
    name: crate::library_name::LibraryName,
    ids: &dyn coven::IdProvider,
) -> Result<Config, CreateLibraryError> {
    let home_dir = dirs::home_dir().ok_or_else(|| {
        CreateLibraryError::Config(ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Failed to get home directory",
        )))
    })?;
    let bae_dir = home_dir.join(".bae");
    create_library_in_bae_dir(&bae_dir, name, ids)
}

fn create_library_in_bae_dir(
    bae_dir: &std::path::Path,
    name: crate::library_name::LibraryName,
    ids: &dyn coven::IdProvider,
) -> Result<Config, CreateLibraryError> {
    let library_id = ids.new_id();

    let library_dir = StoreDir::new(crate::config::registered_library_path(bae_dir, &library_id));
    let device_id = ids.new_id();
    let config = Config::with_defaults(library_id, device_id, &library_dir, name.into_string());
    let creation: Result<Config, CreateLibraryError> = (|| {
        config.save_to_config_yaml()?;
        let config_handle = Arc::new(crate::config::ConfigHandle::new(config.clone()));
        let handle = config_handle
            .coven_builder()
            .synced_tables(crate::sync::synced_tables())
            .oauth_clients(crate::oauth::clients())
            .migrations(crate::migrations::all())
            .open()
            .map_err(|error| CreateLibraryError::Open(Box::new(error)))?;
        handle
            .initialize_identity()
            .map_err(|error| CreateLibraryError::Identity(Box::new(error)))?;
        Ok(config)
    })();

    creation.map_err(|failure| failure.with_rollback(library_dir.remove_tree()))
}

#[cfg(any(test, feature = "test-utils"))]
pub fn create_library_in_bae_dir_for_test(
    bae_dir: &std::path::Path,
    name: crate::library_name::LibraryName,
    ids: &dyn coven::IdProvider,
) -> Result<Config, CreateLibraryError> {
    create_library_in_bae_dir(bae_dir, name, ids)
}

/// coven's restore/join returns the recovered Config; wrap it in bae's Config
/// (which adds Discogs fields) and persist it.
fn save_coven_library(coven_config: coven::Config, store_dir: StoreDir) -> Result<Config, String> {
    let config = Config::from_coven(coven_config, store_dir.to_path_buf());
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
    layout: &coven::StoreLayout,
    bridge: Option<tokio::task::JoinHandle<()>>,
) -> Result<Config, LibraryCodeOperationError> {
    if let Some(handle) = bridge {
        handle.abort();
    }
    match result {
        Ok(coven_config) => {
            let store_dir = layout.store_dir(&coven_config.store_id);
            save_coven_library(coven_config, store_dir).map_err(LibraryCodeOperationError::Failed)
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
pub enum JoinDevicePairingError {
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
    /// The pairing session ran past the deadline stamped into the code. The
    /// code itself is spent — a retry needs a fresh one from the other device,
    /// which is different advice from "open bae over there".
    #[error("the pairing code expired")]
    Expired,
    #[error("{0}")]
    Join(String),
}

impl From<LibraryCodeOperationError> for JoinDevicePairingError {
    fn from(error: LibraryCodeOperationError) -> Self {
        match error {
            LibraryCodeOperationError::Cancelled => JoinDevicePairingError::Cancelled,
            LibraryCodeOperationError::Failed(error) => JoinDevicePairingError::Join(error),
        }
    }
}

#[derive(Clone)]
pub struct PreparedDevicePairingJoin {
    pairing: coven::PreparedDevicePairing,
    layout: coven::StoreLayout,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
}

impl PreparedDevicePairingJoin {
    pub fn fingerprint(&self) -> String {
        crate::sync::membership::pubkey_fingerprint(self.pairing.request().public_key())
    }

    pub fn abandon(&self) -> Result<(), JoinDevicePairingError> {
        self.pairing
            .clone()
            .abandon(&self.layout)
            .map_err(|error| JoinDevicePairingError::Join(error.to_string()))
    }
}

pub async fn prepare_device_pairing_join(
    pairing_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
) -> Result<PreparedDevicePairingJoin, JoinDevicePairingError> {
    let app_dir = crate::config::bae_dir()
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    prepare_device_pairing_join_at(
        pairing_code,
        oauth_tokens,
        cloudkit_ops,
        library_layout(app_dir),
    )
    .await
}

pub fn pending_device_pairing_join(
) -> Result<Option<PendingDevicePairingJoinInfo>, JoinDevicePairingError> {
    let app_dir = crate::config::bae_dir()
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    pending_device_pairing_join_at(library_layout(app_dir))
}

fn pending_device_pairing_join_at(
    layout: coven::StoreLayout,
) -> Result<Option<PendingDevicePairingJoinInfo>, JoinDevicePairingError> {
    Ok(
        pending_device_pairing_at(&layout)?.map(|pairing| PendingDevicePairingJoinInfo {
            pairing_code: pairing.offer().encode(),
            offer: DevicePairingOfferInfo::from_offer(pairing.offer()),
            fingerprint: crate::sync::membership::pubkey_fingerprint(
                pairing.request().public_key(),
            ),
            phase: pairing.phase(),
        }),
    )
}

pub fn abandon_pending_device_pairing_join() -> Result<(), JoinDevicePairingError> {
    let app_dir = crate::config::bae_dir()
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    abandon_pending_device_pairing_join_at(library_layout(app_dir))
}

fn abandon_pending_device_pairing_join_at(
    layout: coven::StoreLayout,
) -> Result<(), JoinDevicePairingError> {
    if let Some(pairing) = pending_device_pairing_at(&layout)? {
        pairing
            .abandon(&layout)
            .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    }
    Ok(())
}

fn pending_device_pairing_at(
    layout: &coven::StoreLayout,
) -> Result<Option<coven::PreparedDevicePairing>, JoinDevicePairingError> {
    let mut pending = coven::PreparedDevicePairing::pending(layout)
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    match pending.len() {
        0 => Ok(None),
        1 => Ok(pending.pop()),
        count => Err(JoinDevicePairingError::Join(format!(
            "found {count} pending device pairing attempts; cancel one before continuing"
        ))),
    }
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
    let layout = library_layout(app_dir);
    let result = crate::sync::restore_from_code(
        code,
        &crate::sync::synced_tables(),
        &crate::migrations::all(),
        coven::CovenMigrationPolicy::ApplyPending,
        // Upload verification is local host policy and does not come from the
        // restore code.
        coven::ExactUploadVerification::MetadataHash,
        crate::config::default_transfer_limits(),
        coven::KeyCustody::Keyring,
        coven::IdentityCustody::Keyring,
        crate::oauth::clients(),
        oauth_tokens,
        cloudkit_ops,
        &layout,
        std::sync::Arc::new(coven::SystemClock),
        std::sync::Arc::new(coven::UuidProvider),
        on_status,
        &rx,
    )
    .await;
    finish_code_operation(result, &layout, bridge)
}

async fn prepare_device_pairing_join_at(
    pairing_code: &str,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    layout: coven::StoreLayout,
) -> Result<PreparedDevicePairingJoin, JoinDevicePairingError> {
    let offer = coven::DevicePairingOffer::decode(pairing_code)
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    let provider_account_email =
        pairing_provider_account_email(offer.cloud_provider().clone(), oauth_tokens.as_ref())
            .await?;
    let pairing =
        coven::PreparedDevicePairing::open_or_create(pairing_code, provider_account_email, &layout)
            .map_err(|error| JoinDevicePairingError::Join(error.to_string()))?;
    Ok(PreparedDevicePairingJoin {
        pairing,
        layout,
        oauth_tokens,
        cloudkit_ops,
    })
}

pub async fn join_prepared_device_pairing_cancellable(
    prepared: PreparedDevicePairingJoin,
    cancel: CancellationToken,
    on_progress: coven::JoiningDeviceJoinProgressObserver,
) -> Result<Config, JoinDevicePairingError> {
    let PreparedDevicePairingJoin {
        pairing,
        layout,
        oauth_tokens,
        cloudkit_ops,
    } = prepared;
    let local_cancel = cancel.clone();
    let (rx, bridge) = cancel_receiver(Some(cancel));
    let result = coven::join_with_device_pairing(
        &pairing,
        layout.clone(),
        crate::sync::synced_tables(),
        crate::migrations::all(),
        coven::CovenMigrationPolicy::ApplyPending,
        // Upload verification is local host policy and does not come from the
        // scanned pairing offer.
        coven::ExactUploadVerification::MetadataHash,
        crate::config::default_transfer_limits(),
        coven::KeyCustody::Keyring,
        coven::IdentityCustody::Keyring,
        crate::oauth::clients(),
        oauth_tokens,
        cloudkit_ops,
        std::sync::Arc::new(coven::SystemClock),
        on_progress,
        &rx,
    )
    .await;
    if let Some(handle) = bridge {
        handle.abort();
    }
    match result {
        Ok(coven::DeviceJoinTransportOutcome::Joined(coven_config)) => {
            let store_dir = layout.store_dir(&coven_config.store_id);
            save_coven_library(coven_config, store_dir).map_err(JoinDevicePairingError::Join)
        }
        // The owner gave up on this attempt before it completed. Not a failure of
        // this device — a distinct end the UI reports as such.
        Ok(coven::DeviceJoinTransportOutcome::Abandoned(_)) => {
            pairing
                .abandon(&layout)
                .map_err(|cleanup| JoinDevicePairingError::Join(cleanup.to_string()))?;
            Err(JoinDevicePairingError::Abandoned)
        }
        Err(error) => {
            let error = if local_cancel.is_cancelled() {
                JoinDevicePairingError::Cancelled
            } else {
                classify_join_error(error)
            };
            // Every end that will not be resumed drops the durable pairing
            // journal. An expired session especially: leaving it on disk makes
            // `pending_device_pairing_join` offer to resume a code that can
            // never complete, and the next launch walks back into the failure.
            if matches!(
                error,
                JoinDevicePairingError::Cancelled
                    | JoinDevicePairingError::Abandoned
                    | JoinDevicePairingError::Expired
            ) {
                pairing
                    .abandon(&layout)
                    .map_err(|cleanup| JoinDevicePairingError::Join(cleanup.to_string()))?;
            }
            Err(error)
        }
    }
}

/// Map coven's bootstrap failure onto bae's join outcome. Coven types the ends a
/// join can come to; each one the user can act on gets its own arm, because the
/// advice differs — reopen bae on the other device, ask for a fresh code, or
/// nothing at all. Whatever is left is a genuine fault and reads as one.
fn classify_join_error(error: coven::BootstrapError) -> JoinDevicePairingError {
    match &error {
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Unavailable(_)) => {
            JoinDevicePairingError::OwnerOffline
        }
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Cancelled) => {
            JoinDevicePairingError::Abandoned
        }
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Expired) => {
            JoinDevicePairingError::Expired
        }
        // Only reached when this device's own cancel token was NOT tripped —
        // the caller checks that first. So a cancellation arriving here came
        // from the other end, which is an abandonment the user is owed a reason
        // for, not the silent "you pressed cancel" case.
        coven::BootstrapError::Cancelled => JoinDevicePairingError::Abandoned,
        _ => JoinDevicePairingError::Join(error.to_string()),
    }
}

#[cfg(feature = "oauth-providers")]
async fn pairing_provider_account_email(
    provider: crate::config::CloudProvider,
    oauth_tokens: Option<&coven::OAuthTokens>,
) -> Result<Option<String>, JoinDevicePairingError> {
    if !provider.needs_oauth() {
        return Ok(None);
    }
    let tokens = oauth_tokens.ok_or_else(|| {
        JoinDevicePairingError::Join(format!("{provider:?} pairing requires OAuth authorization"))
    })?;
    coven::fetch_account_email(provider, tokens)
        .await
        .map(Some)
        .map_err(|error| JoinDevicePairingError::Join(error.to_string()))
}

#[cfg(not(feature = "oauth-providers"))]
async fn pairing_provider_account_email(
    provider: crate::config::CloudProvider,
    _oauth_tokens: Option<&coven::OAuthTokens>,
) -> Result<Option<String>, JoinDevicePairingError> {
    if provider.needs_oauth() {
        return Err(JoinDevicePairingError::Join(format!(
            "{provider:?} pairing requires an OAuth-enabled build"
        )));
    }
    Ok(None)
}
