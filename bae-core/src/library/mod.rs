mod album_selection;
pub mod app_services;
mod bootstrap_auth;
mod browse;
mod detail;
pub mod download_snapshot;
mod library_status;
mod live_read;
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
mod upload_throughput;
pub use album_selection::{
    AlbumSelectionSnapshot, AlbumSelectionSubscription, AlbumSelectionSubscriptionError,
};
pub use app_services::*;
pub use browse::*;
pub use coven::{EagerCacheFillStatus, PinProgress};
pub use detail::{DetailSnapshot, DetailSubscription};
pub use download_snapshot::{
    DownloadOp, DownloadProgress, DownloadQueueContents, DownloadRow, DownloadSnapshot,
    DownloadState, DownloadTransferProgress,
};
pub use library_status::{
    LibraryStatusSnapshot, LibraryStatusSubscription, LibraryStatusSubscriptionError,
};
pub use live_read::{LiveRead, LiveReadError};
pub use local_lifecycle::{remove_local_library, LibraryRemovalError, RemovedLibrary};
pub use manager::*;
pub use outbox_snapshot::{
    OutboxPauseState, OutboxSnapshot, UploadActivity, UploadBar, UploadFileLabel, UploadFileOp,
    UploadProgress, UploadReleaseGroup, UploadState,
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

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod local_lifecycle_tests;

use crate::config::AppDir;
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
    #[error(transparent)]
    Profile(#[from] crate::people::ProfileError),
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
            Self::Profile(error) => error.category(),
            Self::Cancelled => crate::ui::UiErrorCategory::Internal,
            Self::Restore(error) => manager::bootstrap_category(error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateLibraryError {
    #[error(transparent)]
    Store(#[from] coven::StoreCreationError),
}

impl CreateLibraryError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        match self {
            Self::Store(error) => manager::creation_category(error),
        }
    }
}

/// Where this process keeps coven's keys, credentials and saved restore
/// codes: the OS keychain, or for UI and bridge tests an in-memory keychain of
/// their own, so a test never reads or writes the person's login keychain.
#[derive(Clone)]
pub enum Keychain {
    System,
    #[cfg(any(test, feature = "test-utils"))]
    InMemory(std::sync::Arc<coven::TestCoven>),
}

impl Keychain {
    /// An empty in-memory keychain, kept for this process only.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn in_memory() -> Self {
        Self::InMemory(std::sync::Arc::new(coven::TestCoven::new()))
    }

    /// Configure coven with the declarations owned by bae, keeping keys in
    /// this keychain.
    pub fn store_builder(&self, app_dir: &AppDir) -> coven::CovenBuilder {
        let layout = app_dir.store_layout();
        configure_store(match self {
            Self::System => coven::Coven::builder(layout),
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.builder(layout),
        })
    }

    /// The restore codes saved in this keychain (iCloud Keychain on Apple).
    pub async fn saved_restore_codes(
        &self,
    ) -> Result<Vec<coven::SavedRestoreCode>, coven::BootstrapError> {
        match self {
            Self::System => coven::saved_restore_codes().await,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.saved_restore_codes().await,
        }
    }
}

pub(crate) fn configure_store(builder: coven::CovenBuilder) -> coven::CovenBuilder {
    builder
        .synced_tables(crate::sync::synced_tables())
        .migrations(crate::migrations::all())
}

/// Create a store and retain its open handle, including its identity custody.
pub async fn create_library(
    builder: coven::CovenBuilder,
    name: crate::library_name::LibraryName,
) -> Result<coven::CovenHandle, CreateLibraryError> {
    Ok(configure_store(builder).create_store(name.as_str()).await?)
}

pub async fn create_library_default(
    app_dir: &AppDir,
) -> Result<coven::CovenHandle, CreateLibraryError> {
    create_library(
        Keychain::System.store_builder(app_dir),
        crate::library_name::generate_library_name(),
    )
    .await
}

/// Restore with a separately entered S3 key and coven's own OAuth presentation.
/// The returned handle retains session custody; the caller keeps it for the
/// running library instead of reopening from a second configuration copy.
pub async fn restore_from_code(
    mut builder: coven::CovenBuilder,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    bootstrap_auth::authenticate(&mut builder, bootstrap_auth::code_access(code)?, cancel).await?;
    coven::restore_from_code(
        configure_store(builder),
        code,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(restore_error)
}

pub async fn restore_from_keychain(
    mut builder: coven::CovenBuilder,
    saved: &coven::SavedRestoreCode,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<Option<coven::CovenHandle>, RestoreFromCodeError> {
    bootstrap_auth::authenticate(&mut builder, saved.location.oauth_access(), cancel).await?;
    coven::restore_from_keychain(
        configure_store(builder),
        saved,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(restore_error)
}

/// The membership is established even if writing its profile failed. Retain
/// this handle and retry profile creation; do not join as another identity.
pub type JoinedLibrary = (
    coven::CovenHandle,
    Result<crate::people::PersonProfile, crate::people::ProfileError>,
);

pub async fn join_with_invite(
    mut builder: coven::CovenBuilder,
    code: &str,
    device_name: &str,
    person_name: &str,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    if person_name.trim().is_empty() {
        return Err(crate::people::ProfileError::EmptyName.into());
    }
    bootstrap_auth::authenticate(&mut builder, bootstrap_auth::code_access(code)?, cancel).await?;
    let Some(handle) = Box::pin(coven::join_with_invite(
        configure_store(builder),
        code,
        device_name,
        on_status,
        cancel,
    ))
    .await
    .map_err(restore_error)?
    else {
        return Ok(None);
    };
    let profile = crate::db::Database::from_handle(handle.clone(), clock)
        .create_profile(person_name)
        .await;
    Ok(Some((handle, profile)))
}

/// Replace this device after the caller has warned that unsent edits are discarded.
/// Coven checks storage and custody before replacing the installation. The
/// returned handle is ready to open in bae; there is no post-reset notification.
pub async fn reset_device(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    device_name: &str,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    configure_store(builder)
        .reset_device(store, device_name)
        .await
        .map_err(restore_error)
}

fn restore_error(error: coven::BootstrapError) -> RestoreFromCodeError {
    match error {
        coven::BootstrapError::Cancelled => RestoreFromCodeError::Cancelled,
        error => RestoreFromCodeError::Restore(Box::new(error)),
    }
}
