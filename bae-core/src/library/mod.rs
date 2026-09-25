mod album_selection;
pub mod app_services;
mod browse;
mod device_pairing;
pub mod download_snapshot;
mod library_status;
pub(crate) mod live_uploads;
mod local_lifecycle;
pub mod manager;
pub mod outbox_snapshot;
mod outbox_snapshot_summary;
pub mod output_snapshot;
mod queue_upcoming;
pub mod queued_releases;
pub mod release_queue;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod save;
pub mod search;
mod storage_browse;
pub mod storage_inspector;
pub(crate) mod storage_transitions;
pub(crate) mod sync_controller;
pub mod upload_throughput;
pub use album_selection::{
    AlbumSelectionSnapshot, AlbumSelectionSubscription, AlbumSelectionSubscriptionError,
};
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
pub use library_status::{
    LibraryStatusSnapshot, LibraryStatusSubscription, LibraryStatusSubscriptionError,
};
pub use local_lifecycle::remove_local_library;
pub use manager::*;
pub use outbox_snapshot::{
    OutboxPauseState, OutboxSnapshot, UploadActivity, UploadBar, UploadFileLabel, UploadFileOp,
    UploadIssue, UploadPhase, UploadProgress, UploadReleaseGroup, UploadState,
};
pub use output_snapshot::{OutputKind, OutputOp, OutputProgress, OutputSnapshot, OutputState};
pub use queue_upcoming::{
    QueueUpcomingSnapshot, QueueUpcomingSubscription, QueueUpcomingSubscriptionError,
    QueueUpcomingWindow,
};
pub use queued_releases::QueuedReleases;
pub use release_queue::{CountLabel, ReleaseQueue};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use save::SaveService;
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LibrarySearchSubscription,
    LibrarySearchSubscriptionError, SEARCH_RESULT_LIMIT,
};
/// How a device join this library invited ended. The controller itself stays
/// crate-private; this outcome is part of the public sharing surface.
pub use storage_browse::{
    StorageBrowseSnapshot, StorageBrowseSubscription, StorageBrowseSubscriptionError,
    StorageBrowseView,
};
pub use upload_throughput::UploadThroughput;

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod device_pairing_tests;
#[cfg(test)]
mod local_lifecycle_tests;

use crate::config::{AppDir, Config, ConfigError};
use coven::StoreDir;
use std::sync::Arc;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

/// The pin queue with the Downloads pane's stream.
pub type Downloads = QueuedReleases<(), DownloadTransferProgress, DownloadSnapshot>;
/// The export and save queue with the Exporting pane's stream.
pub type Outputs = QueuedReleases<output_snapshot::OutputRequest, u8, OutputSnapshot>;

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
    #[error("restore failed: {0}")]
    Restore(#[source] Box<coven::BootstrapError>),
}

impl RestoreFromCodeError {
    /// What the person can do about the failure: fix the code, the cloud
    /// credentials, or the keyring, or retry once the cloud is reachable.
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        match self {
            Self::Cancelled => crate::ui::UiErrorCategory::Internal,
            Self::Restore(error) => manager::bootstrap_category(error),
        }
    }
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

/// Create a library under a generated name and establish its device identity.
pub fn create_library_default(
    app_dir: &AppDir,
    ids: &dyn coven::IdProvider,
) -> Result<Config, CreateLibraryError> {
    create_library(app_dir, crate::library_name::generate_library_name(), ids)
}

/// Create a library registered under `app_dir` and establish its device
/// identity.
pub fn create_library(
    app_dir: &AppDir,
    name: crate::library_name::LibraryName,
    ids: &dyn coven::IdProvider,
) -> Result<Config, CreateLibraryError> {
    let library_id = ids.new_id();

    let library_dir = StoreDir::new(app_dir.registered_library(&library_id));
    let device_id = ids.new_id();
    let config = Config::with_defaults(library_id, device_id, &library_dir, name.into_string());
    let creation: Result<Config, CreateLibraryError> = (|| {
        config.save_store_config()?;
        let config_handle = Arc::new(crate::config::ConfigHandle::new(config.clone()));
        let handle = config_handle
            .coven_builder()
            .synced_tables(crate::sync::synced_tables())
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

/// Finish a restore: stop the cancel bridge, then map coven's outcome.
/// `BootstrapError::Cancelled` (coven cancelled cooperatively at a phase
/// boundary and already removed its partial store dir) becomes our `Cancelled`;
/// any other error keeps coven's typed reason; success wraps the config coven
/// already wrote to the store's `config.yaml`.
fn finish_code_operation(
    result: Result<coven::Config, coven::BootstrapError>,
    layout: &coven::StoreLayout,
    bridge: Option<tokio::task::JoinHandle<()>>,
) -> Result<Config, RestoreFromCodeError> {
    if let Some(handle) = bridge {
        handle.abort();
    }
    match result {
        Ok(coven_config) => {
            let store_dir = layout.store_dir(&coven_config.store_id);
            Ok(Config::from_coven(coven_config, store_dir.to_path_buf()))
        }
        Err(coven::BootstrapError::Cancelled) => Err(RestoreFromCodeError::Cancelled),
        Err(error) => Err(RestoreFromCodeError::Restore(Box::new(error))),
    }
}

/// Restore a library from a restore code. Wraps coven's `restore_from_code`.
pub async fn restore_from_code(
    app_dir: &AppDir,
    code: &str,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    restore_from_code_inner(
        app_dir,
        code,
        oauth_clients,
        oauth_tokens,
        cloudkit_ops,
        None,
        on_status,
    )
    .await
}

pub async fn restore_from_code_cancellable(
    app_dir: &AppDir,
    code: &str,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: CancellationToken,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    restore_from_code_inner(
        app_dir,
        code,
        oauth_clients,
        oauth_tokens,
        cloudkit_ops,
        Some(cancel),
        on_status,
    )
    .await
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
    /// Bootstrapping the store from the cloud failed for a reason with no arm
    /// of its own; coven's typed reason says whether the cloud, the keyring,
    /// or the handshake is at fault.
    #[error("join failed: {0}")]
    Bootstrap(#[source] Box<coven::BootstrapError>),
    /// The pairing code does not decode, or the durable pairing attempt it
    /// names could not be opened, resumed, or discarded.
    #[error("pairing attempt: {0}")]
    Pairing(#[from] coven::DevicePairingError),
    /// Looking up the provider account the pairing request names failed.
    #[cfg(feature = "oauth-providers")]
    #[error("provider account: {0}")]
    ProviderAccount(#[from] coven::OAuthError),
    /// The provider signs in through OAuth, and this join has no authorization
    /// for it (or this build has no OAuth providers).
    #[error("{0:?} pairing requires OAuth authorization")]
    ProviderAuthorizationMissing(crate::config::CloudProvider),
    /// More than one pairing attempt is journaled; one has to be cancelled
    /// before either can continue.
    #[error("found {0} pending device pairing attempts; cancel one before continuing")]
    SeveralPendingAttempts(usize),
}

impl JoinDevicePairingError {
    /// The class of a failure without an arm of its own on the join screen.
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory as C;
        match self {
            Self::Cancelled
            | Self::OwnerOffline
            | Self::Abandoned
            | Self::Expired
            | Self::SeveralPendingAttempts(_) => C::Membership,
            Self::Bootstrap(error) => manager::bootstrap_category(error),
            Self::Pairing(coven::DevicePairingError::Key(error)) => manager::key_category(error),
            Self::Pairing(
                coven::DevicePairingError::Journal(_) | coven::DevicePairingError::JournalPath(_),
            ) => C::Internal,
            // Everything else is a code or request that does not decode as
            // this pairing: the person scans a fresh code.
            Self::Pairing(_) => C::Config,
            #[cfg(feature = "oauth-providers")]
            Self::ProviderAccount(error) => oauth_category(error),
            Self::ProviderAuthorizationMissing(_) => C::Credentials,
        }
    }
}

/// An OAuth request that never reached the provider is a network failure the
/// person retries; any answer the provider gave is about the account.
#[cfg(feature = "oauth-providers")]
fn oauth_category(error: &coven::OAuthError) -> crate::ui::UiErrorCategory {
    match error {
        coven::OAuthError::TokenRequest { .. } => crate::ui::UiErrorCategory::Network,
        _ => crate::ui::UiErrorCategory::Credentials,
    }
}

#[derive(Clone)]
pub struct PreparedDevicePairingJoin {
    pairing: coven::PreparedDevicePairing,
    layout: coven::StoreLayout,
    oauth_clients: coven::OAuthClients,
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
            .map_err(JoinDevicePairingError::from)
    }
}

/// The one pairing attempt retained under `app_dir` that can continue without
/// rescanning the existing device's code.
pub fn pending_device_pairing_join(
    app_dir: &AppDir,
) -> Result<Option<PendingDevicePairingJoinInfo>, JoinDevicePairingError> {
    let layout = app_dir.store_layout();
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

/// Discard the joining identity and journal of the one pending pairing attempt
/// under `app_dir`, if there is one.
pub fn abandon_pending_device_pairing_join(app_dir: &AppDir) -> Result<(), JoinDevicePairingError> {
    let layout = app_dir.store_layout();
    if let Some(pairing) = pending_device_pairing_at(&layout)? {
        pairing.abandon(&layout)?;
    }
    Ok(())
}

fn pending_device_pairing_at(
    layout: &coven::StoreLayout,
) -> Result<Option<coven::PreparedDevicePairing>, JoinDevicePairingError> {
    let mut pending = coven::PreparedDevicePairing::pending(layout)?;
    match pending.len() {
        0 => Ok(None),
        1 => Ok(pending.pop()),
        count => Err(JoinDevicePairingError::SeveralPendingAttempts(count)),
    }
}

async fn restore_from_code_inner(
    app_dir: &AppDir,
    code: &str,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
    cancel: Option<CancellationToken>,
    on_status: impl Fn(&str),
) -> Result<Config, RestoreFromCodeError> {
    let (rx, bridge) = cancel_receiver(cancel);
    // The default custody for both the master key and this device's identity —
    // the OS keyring, mirroring what `Coven::builder` itself defaults to for a
    // library opened the ordinary way (bae never overrides either).
    let layout = app_dir.store_layout();
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
        oauth_clients,
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

/// Open or create the pairing attempt `pairing_code` names, journaled under
/// `app_dir`.
pub async fn prepare_device_pairing_join(
    app_dir: &AppDir,
    pairing_code: &str,
    oauth_clients: coven::OAuthClients,
    oauth_tokens: Option<coven::OAuthTokens>,
    cloudkit_ops: Option<Arc<dyn coven::CloudKitOps>>,
) -> Result<PreparedDevicePairingJoin, JoinDevicePairingError> {
    let layout = app_dir.store_layout();
    let offer = coven::DevicePairingOffer::decode(pairing_code)?;
    let provider_account_email =
        pairing_provider_account_email(offer.cloud_provider().clone(), oauth_tokens.as_ref())
            .await?;
    let pairing = coven::PreparedDevicePairing::open_or_create(
        pairing_code,
        provider_account_email,
        &layout,
    )?;
    Ok(PreparedDevicePairingJoin {
        pairing,
        layout,
        oauth_clients,
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
        oauth_clients,
        oauth_tokens,
        cloudkit_ops,
    } = prepared;
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
        oauth_clients,
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
            Ok(Config::from_coven(coven_config, store_dir.to_path_buf()))
        }
        // The owner gave up on this attempt before it completed. Not a failure of
        // this device — a distinct end the UI reports as such.
        Ok(coven::DeviceJoinTransportOutcome::Abandoned(_)) => {
            pairing.abandon(&layout)?;
            Err(JoinDevicePairingError::Abandoned)
        }
        Err(error) => {
            let error = classify_join_error(error);
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
                pairing.abandon(&layout)?;
            }
            Err(error)
        }
    }
}

/// Map coven's bootstrap failure onto bae's join outcome. Coven types the ends a
/// join can come to; each one the user can act on gets its own arm, because the
/// advice differs — reopen bae on the other device, ask for a fresh code, or
/// nothing at all. Whatever is left keeps coven's typed reason, which says
/// whether the cloud, the keyring, or the handshake failed.
fn classify_join_error(error: coven::BootstrapError) -> JoinDevicePairingError {
    match &error {
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Unavailable(_)) => {
            JoinDevicePairingError::OwnerOffline
        }
        // The owner cancelled the session: an abandonment the user is owed a
        // reason for.
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::SessionCancelled) => {
            JoinDevicePairingError::Abandoned
        }
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::Expired) => {
            JoinDevicePairingError::Expired
        }
        // This device's own cancel: while waiting for the invitation, or at a
        // bootstrap phase boundary. The silent "you pressed cancel" end.
        coven::BootstrapError::Pairing(coven::DevicePairingTransportError::WaitCancelled)
        | coven::BootstrapError::Cancelled => JoinDevicePairingError::Cancelled,
        _ => JoinDevicePairingError::Bootstrap(Box::new(error)),
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
    let tokens = oauth_tokens
        .ok_or_else(|| JoinDevicePairingError::ProviderAuthorizationMissing(provider.clone()))?;
    coven::fetch_account_email(provider, tokens)
        .await
        .map(Some)
        .map_err(JoinDevicePairingError::from)
}

#[cfg(not(feature = "oauth-providers"))]
async fn pairing_provider_account_email(
    provider: crate::config::CloudProvider,
    _oauth_tokens: Option<&coven::OAuthTokens>,
) -> Result<Option<String>, JoinDevicePairingError> {
    if provider.needs_oauth() {
        return Err(JoinDevicePairingError::ProviderAuthorizationMissing(
            provider,
        ));
    }
    Ok(None)
}
