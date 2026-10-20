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
pub use local_lifecycle::{
    finish_removals, remove_local_library, remove_unopenable_library, unsent_before_removal,
    LibraryRemovalError, UnfinishedRemoval,
};
pub use manager::*;
pub use outbox_snapshot::{
    OutboxPauseState, OutboxSnapshot, UploadActivity, UploadBar, UploadFileLabel, UploadFileOp,
    UploadProgress, UploadReleaseGroup, UploadState, UploadSubject,
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

    /// Delete a closed store without opening it: this keychain's entries for
    /// it, then its directory.
    async fn discard_store(
        &self,
        directory: &coven::StoreDir,
    ) -> Result<(), coven::StoreDeletionError> {
        match self {
            Self::System => coven::Coven::discard_store(directory).await,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.discard_store(directory).await,
        }
    }

    /// The restore codes saved in this keychain (iCloud Keychain on Apple)
    /// for libraries not already on this device: a library this device has
    /// is opened, not restored.
    pub async fn saved_restore_codes(
        &self,
        app_dir: &AppDir,
    ) -> Result<Vec<coven::SavedRestoreCode>, SavedRestoreCodesError> {
        let codes = match self {
            Self::System => coven::saved_restore_codes().await?,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.saved_restore_codes().await?,
        };
        restorable_codes(codes, &app_dir.store_layout()).await
    }

    /// Delete a library's saved restore code, and the S3 key saved beside it,
    /// from this keychain: on Apple, from iCloud Keychain, and so from every
    /// one of the person's Apple devices. It changes no library, storage or
    /// device's copy, and needs no open library. An entry already gone
    /// succeeds, and calling again finishes one that failed partway.
    pub async fn forget_saved_restore_code(
        &self,
        store: coven::StoreId,
    ) -> Result<(), ForgetSavedRestoreCodeError> {
        match self {
            Self::System => coven::forget_saved_restore_code(store).await?,
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.forget_saved_restore_code(store).await?,
        }
        Ok(())
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
/// Coven checks storage and custody before replacing the installation. Calling
/// it again finishes a reset that stopped after the new copy took the old
/// one's place; for a restore that stopped there, coven refuses it and changes
/// nothing. The returned handle is ready to open in bae; there is no post-reset
/// notification.
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

/// Restore a library this device holds but can no longer send for, because
/// another of the person's devices replaced their keys. The code must be the
/// person's current restore code for this library. Coven loads the library
/// from storage under a new device id beside this device's copy, which stays
/// and opens as before until the load is complete, then takes its place. A
/// restore that fails leaves this device's copy, and calling again with the
/// same code retries it, or finishes one that stopped after the swap; for a
/// reset that stopped there, coven refuses it and changes nothing. The caller
/// closed the library and warned that edits on this device that haven't
/// synced are discarded.
pub async fn restore_replacing_device(
    mut builder: coven::CovenBuilder,
    store: coven::StoreId,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    // Coven refuses an invite or another library's code too, but only after
    // the provider sign-in below; refusing it first asks for no sign-in the
    // restore would never use.
    let refused = |error| restore_error(coven::BootstrapError::Code(error));
    let access = match coven::decode_code_info(code).map_err(refused)? {
        coven::CodeInfo::Restore(metadata) if metadata.store_id == store => metadata.oauth_access,
        coven::CodeInfo::Restore(metadata) => {
            return Err(refused(coven::CodeError::WrongStore {
                expected: store,
                actual: metadata.store_id,
            }))
        }
        coven::CodeInfo::Invite { .. } => {
            return Err(refused(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Restore,
                actual: coven::CodeKind::Invite,
            }))
        }
    };
    bootstrap_auth::authenticate(&mut builder, access, cancel).await?;
    configure_store(builder)
        .restore_device(store, code, s3_credentials, device_name, on_status, cancel)
        .await
        .map_err(restore_error)
}

#[derive(Debug, thiserror::Error)]
pub enum SavedRestoreCodesError {
    #[error(transparent)]
    Keychain(#[from] coven::BootstrapError),
    #[error(transparent)]
    Stores(#[from] coven::StoreLayoutError),
}

impl SavedRestoreCodesError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        match self {
            Self::Keychain(error) => manager::bootstrap_category(error),
            Self::Stores(_) => crate::ui::UiErrorCategory::Config,
        }
    }
}

/// Why a saved restore code couldn't be deleted from the keychain.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ForgetSavedRestoreCodeError(#[from] coven::BootstrapError);

impl ForgetSavedRestoreCodeError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        manager::bootstrap_category(&self.0)
    }
}

async fn restorable_codes(
    codes: Vec<coven::SavedRestoreCode>,
    layout: &coven::StoreLayout,
) -> Result<Vec<coven::SavedRestoreCode>, SavedRestoreCodesError> {
    let here: std::collections::HashSet<_> = layout
        .stores()
        .await?
        .into_iter()
        .map(|store| store.id)
        .collect();
    Ok(codes
        .into_iter()
        .filter(|code| !here.contains(&code.store_id))
        .collect())
}

fn restore_error(error: coven::BootstrapError) -> RestoreFromCodeError {
    match error {
        coven::BootstrapError::Cancelled => RestoreFromCodeError::Cancelled,
        error => RestoreFromCodeError::Restore(Box::new(error)),
    }
}
