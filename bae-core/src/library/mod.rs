mod album_selection;
pub mod app_services;
mod browse;
mod detail;
pub mod download_snapshot;
mod library_status;
mod live_read;
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
pub use detail::{DetailSnapshot, DetailSubscription};
pub use download_snapshot::{
    DownloadOp, DownloadProgress, DownloadQueueContents, DownloadRow, DownloadSnapshot,
    DownloadState, DownloadTransferProgress,
};
pub use library_status::{
    LibraryStatusSnapshot, LibraryStatusSubscription, LibraryStatusSubscriptionError,
};
pub use live_read::{LiveRead, LiveReadError};
pub use local_lifecycle::remove_local_library;
pub use manager::*;
pub use outbox_snapshot::{
    ByteProgress, OutboxPauseState, OutboxSnapshot, UploadActivity, UploadBar, UploadFileLabel,
    UploadFileOp, UploadIssue, UploadPhase, UploadProgress, UploadReleaseGroup, UploadState,
};
pub use output_snapshot::{
    OutputKind, OutputOp, OutputProgress, OutputQueueContents, OutputRow, OutputSnapshot,
    OutputState,
};
pub use queue_upcoming::{QueueUpcomingSnapshot, QueueUpcomingSubscription, QueueUpcomingWindow};
pub use queued_releases::QueuedReleases;
pub use release_queue::{CountLabel, QueuedRelease, ReleaseQueue};
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LibrarySearchSubscription,
    LibrarySearchSubscriptionError, SEARCH_RESULT_LIMIT,
};
pub use storage_browse::{StorageBrowseSnapshot, StorageBrowseSubscription, StorageBrowseView};
pub use upload_throughput::UploadThroughput;

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod local_lifecycle_tests;

use crate::config::{AppDir, Config, ConfigError};
use coven::StoreDir;
use std::sync::Arc;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

/// The pin queue with the stream of its contents.
pub type Downloads = QueuedReleases<(), DownloadTransferProgress>;
/// The export and save queue with the stream of its contents.
pub type Outputs = QueuedReleases<output_snapshot::OutputRequest, u8>;

/// One track of a save: its tags, its stored audio, and which of its audio
/// the saved file carries. Holds no open source; a save opens the files a
/// plan reads only while that track is being saved.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) struct SaveTrackPlan {
    resolved: manager::ResolvedSaveTags,
    audio_meta: manager::TrackAudioMeta,
    window: manager::SaveWindow,
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
