mod album_selection;
pub mod app_services;
mod artist_search;
mod bootstrap_auth;
mod browse;
mod closing_step;
pub(crate) mod detach_cancels;
mod detail;
pub mod download_snapshot;
mod file_transfers;
mod library_status;
mod live_read;
mod local_lifecycle;
pub(crate) mod make_local_transfers;
pub mod manager;
mod outbox_watch;
pub mod output_snapshot;
mod queue_upcoming;
pub mod queued_releases;
#[cfg(desktop)]
mod release_files;
pub mod release_queue;
mod release_transfers;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod save;
pub mod search;
mod storage_browse;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod storage_inspector;
mod sync_activity;
pub(crate) mod sync_controller;
mod upload_progress;
mod upload_throughput;
pub use album_selection::{
    AlbumSelectionSnapshot, AlbumSelectionSubscription, AlbumSelectionSubscriptionError,
};
pub use app_services::*;
pub use artist_search::{ArtistSearchSnapshot, ArtistSearchSubscription, ArtistSearchView};
pub use browse::*;
pub use closing_step::ClosingStep;

/// Where a storage setup reports each step coven runs, as coven reports it.
pub type StorageSetupSteps<'a> =
    &'a (dyn Fn(coven::Progress<coven::StorageSetupStep>) + Send + Sync);
pub use detail::{DetailSnapshot, DetailSubscription};
pub use download_snapshot::{
    DownloadEntry, DownloadPhase, DownloadProgress, DownloadRow, DownloadSnapshot,
    DownloadTransferProgress, QueuedDownload,
};
pub use file_transfers::{FileTransferChange, FileTransfersWatch};
pub use library_status::{
    LibraryStatusSnapshot, LibraryStatusSubscription, LibraryStatusSubscriptionError,
};
pub(crate) use live_read::{live_read_channel, SubscriptionReader};
pub use live_read::{LiveRead, LiveReadError};
pub use local_lifecycle::{
    finish_removals, remove_local_library, remove_unopenable_library, unsent_before_removal,
    LibraryRemovalError, UnfinishedRemoval,
};
pub use make_local_transfers::MakeLocalTransferProgress;
pub use manager::*;
pub use outbox_watch::{
    OutboxTotalTransferWatch, OutboxTransferChange, OutboxTransfers, OutboxTransfersWatch,
    ReleaseUploadTransfer, UploadTransfer,
};
pub use output_snapshot::{
    OutputEntry, OutputKind, OutputOp, OutputPhase, OutputPhaseRow, OutputPhaseSnapshot,
    OutputProgress, OutputRow, OutputSnapshot, OutputState,
};
pub use queue_upcoming::{QueueUpcomingSnapshot, QueueUpcomingSubscription, QueueUpcomingWindow};
pub(crate) use queued_releases::QueuedReleases;
#[cfg(desktop)]
pub use release_files::{
    ReleaseFileColumn, ReleaseFileOrder, ReleaseFilesRequest, ReleaseFilesSnapshot,
    ReleaseFilesSubscription,
};
#[cfg(desktop)]
pub use release_queue::ReleaseQueueEntries;
pub use release_queue::{
    CountLabel, QueuedRelease, ReleaseQueueCounts, ReleaseQueueWindows, TransferCount,
    TransferStage,
};
pub use release_transfers::{
    DownloadTransferChange, DownloadTransfersWatch, MakeLocalTransfersWatch, OutputTransferChange,
    OutputTransfersWatch, ReleaseTransferChange, ReleaseTransfersWatch,
};
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LibrarySearchSubscription,
    LibrarySearchSubscriptionError, SEARCH_RESULT_LIMIT,
};
pub use storage_browse::{
    ReleaseQueueRun, StorageBrowseSnapshot, StorageBrowseSubscription, StorageBrowseView,
    StorageQueues, UploadQueueRun,
};
pub use sync_activity::{DownloadsInFlight, SyncActivity, UploadQueueCounts, UploadsInFlight};
pub use upload_progress::{UploadActivity, UploadBar, UploadProgress, UploadState};

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod local_lifecycle_tests;
#[cfg(test)]
mod saved_codes_tests;

use crate::config::AppDir;
use crate::db::configure_store;
use tokio::sync::watch;

pub use tokio_util::sync::CancellationToken;

/// The pin queue: its table, and the bytes its active download has moved.
pub(crate) type Downloads = QueuedReleases<(), DownloadTransferProgress>;
/// The export and save queue: its table, and the percent its active one has
/// reached.
#[cfg(desktop)]
pub(crate) type Outputs = QueuedReleases<output_snapshot::OutputRequest, u8>;

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
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        match self {
            Self::Profile(error) => error.category(),
            Self::Cancelled => crate::diagnostics::UiErrorCategory::Internal,
            Self::Restore(error) => crate::diagnostics::bootstrap_category(error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateLibraryError {
    #[error(transparent)]
    Store(#[from] coven::StoreCreationError),
}

impl CreateLibraryError {
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        match self {
            Self::Store(error) => crate::diagnostics::creation_category(error),
        }
    }
}

/// Why the folder for SQLite's temporary tables couldn't be named.
#[cfg(target_os = "android")]
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TemporaryDirectoryError(#[from] coven::TemporaryDirectoryError);

#[cfg(target_os = "android")]
impl TemporaryDirectoryError {
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        crate::diagnostics::UiErrorCategory::Config
    }
}

/// Keep SQLite's temporary tables in `directory`, the app's cache folder, for
/// every library this process opens. SQLite finds no such folder on Android
/// by itself, and a temporary table that outgrows its connection's cache, as
/// a staged snapshot does, fails without one. Called at launch, before the
/// first library opens.
#[cfg(target_os = "android")]
pub fn set_temporary_directory(directory: &std::path::Path) -> Result<(), TemporaryDirectoryError> {
    Ok(coven::Coven::set_temporary_directory(directory)?)
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

    /// A coven builder keeping keys in this keychain, for the functions here
    /// that create, open, restore or join a library: each configures it as
    /// bae's store (`configure_store`).
    pub fn store_builder(&self, app_dir: &AppDir) -> coven::CovenBuilder {
        let layout = app_dir.store_layout();
        match self {
            Self::System => coven::Coven::builder(layout),
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.builder(layout),
        }
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

    /// The restore codes saved for the person (iCloud Keychain on Apple,
    /// this phone's Block Store on Android) for libraries not already on this device: a library this device has
    /// is opened, not restored. An entry coven can't restore from is listed
    /// as unreadable, with its library's id and why, so the person can
    /// delete it. A phone without Google Play services has no Block Store
    /// and keeps no saved codes, so it lists none.
    pub async fn saved_restore_codes(
        &self,
        app_dir: &AppDir,
    ) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
        let codes = match self {
            Self::System => match coven::saved_restore_codes().await {
                Err(error) if keeps_no_saved_codes(&error) => {
                    tracing::info!(
                        "this device keeps no saved restore codes: it has no Block Store"
                    );
                    Vec::new()
                }
                listed => listed?,
            },
            #[cfg(any(test, feature = "test-utils"))]
            Self::InMemory(installation) => installation.saved_restore_codes().await?,
        };
        restorable_codes(codes, &app_dir.store_layout()).await
    }

    /// Delete a library's saved restore code, and the S3 key saved beside it,
    /// from where it is saved: on Apple, from iCloud Keychain, and so from
    /// every one of the person's Apple devices; on Android, from this phone's
    /// Block Store. It changes no library, storage or device's copy, and
    /// needs no open library. An unreadable entry is deleted the same way. An
    /// entry already gone succeeds, and calling again finishes one that
    /// failed partway.
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

/// `builder` with its file transfers limited as `limits` says, per direction.
pub(crate) fn with_transfer_limits(
    builder: coven::CovenBuilder,
    limits: coven::TransferLimits,
) -> coven::CovenBuilder {
    builder.max_concurrent_uploads(limits.uploads).max_concurrent_downloads(limits.downloads)
}

/// `builder` configured as bae's store for loading a library from storage,
/// under the default transfer limits: the library's own preferences are in
/// the store being loaded, so they apply once it opens.
fn loading_builder(builder: coven::CovenBuilder) -> coven::CovenBuilder {
    with_transfer_limits(configure_store(builder), crate::config::default_transfer_limits())
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
    let metadata = bootstrap_auth::code_metadata(code)?;
    bootstrap_auth::authenticate(&mut builder, metadata.oauth_access, cancel).await?;
    coven::restore_from_code(
        loading_builder(builder),
        code,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(restore_error)
}

pub async fn restore_from_saved_code(
    mut builder: coven::CovenBuilder,
    saved: &coven::SavedRestoreCode,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<Option<coven::CovenHandle>, RestoreFromCodeError> {
    bootstrap_auth::authenticate(&mut builder, saved.location.oauth_access(), cancel).await?;
    coven::restore_from_saved_code(
        loading_builder(builder),
        saved,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(restore_error)
}

/// The membership is established even if asking for its profile failed.
/// Retain this handle and retry profile creation; do not join as another
/// identity. The profile request is `None` for an invite that replaced an
/// existing member's keys: that person keeps the row they already have
/// (§5.4). An asked-for profile is written once sync has made the person's
/// private circle.
pub type JoinedLibrary = (coven::CovenHandle, Option<Result<(), crate::people::ProfileError>>);

/// The app's hold on a running join: cancel it before its request is sent, or
/// check for approval at once. One control covers coven's join and any
/// provider sign-in bae runs for it.
pub struct JoinControl {
    sign_in: watch::Sender<bool>,
    join: coven::JoinControl,
}

impl JoinControl {
    pub fn new() -> Self {
        Self { sign_in: watch::channel(false).0, join: coven::JoinControl::new() }
    }

    /// Cancel the join. Once its request may have been sent, coven keeps the
    /// join and the call ends with `BootstrapError::JoinRequestSent`.
    pub fn cancel(&self) {
        self.sign_in.send_replace(true);
        self.join.cancel();
    }

    /// End the join's wait so the next check for approval runs now.
    pub fn check_now(&self) {
        self.join.check_now();
    }
}

impl Default for JoinControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Join a library from an invite. The person's profile takes the name the
/// admin typed in the invite (§5.4). Dropping the call keeps the join for
/// `resume_join`.
pub async fn join_with_invite(
    mut builder: coven::CovenBuilder,
    code: &str,
    device_name: &str,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let invite = invite_info(code)?;
    bootstrap_auth::authenticate(
        &mut builder,
        invite.metadata.oauth_access,
        &control.sign_in.subscribe(),
    )
    .await?;
    let joined = Box::pin(coven::join_with_invite(
        loading_builder(builder),
        code,
        device_name,
        on_status,
        &control.join,
    ))
    .await
    .map_err(restore_error)?;
    Ok(match joined {
        Some(handle) => Some(with_profile(handle, invite.invitee.as_deref(), clock).await),
        None => None,
    })
}

/// The joins this device has under way, each as its saved invite reads.
pub async fn unfinished_joins(
    builder: &coven::CovenBuilder,
) -> Result<Vec<coven::InviteInfo>, RestoreFromCodeError> {
    builder.unfinished_joins().await.map_err(restore_error)
}

/// Continue a join from `unfinished_joins` after bae restarted. coven keeps
/// the join's provider sign-in on the device beside it, so this signs in only
/// when coven asks for one: when the kept sign-in is gone, or the provider
/// refuses it. It then signs in with the access the listed join names and
/// resumes once more. Each call consumes its builder, so `builder` makes one
/// for each (§5.6).
pub async fn resume_join(
    builder: impl Fn() -> coven::CovenBuilder,
    join: &coven::InviteInfo,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let (store, on_status, coven_control) = (join.metadata.store_id, &on_status, &control.join);
    let resume = move |builder: coven::CovenBuilder| {
        Box::pin(loading_builder(builder).resume_join(store, on_status, coven_control))
    };
    let joined = match resume(builder()).await {
        Err(coven::BootstrapError::OAuth(coven::OAuthError::Reauthorize(_))) => {
            let mut signed_in = builder();
            bootstrap_auth::authenticate(
                &mut signed_in,
                join.metadata.oauth_access,
                &control.sign_in.subscribe(),
            )
            .await?;
            resume(signed_in).await
        }
        resumed => resumed,
    }
    .map_err(restore_error)?;
    Ok(match joined {
        Some(handle) => Some(with_profile(handle, join.invitee.as_deref(), clock).await),
        None => None,
    })
}

/// The invite a code holds; a restore code is refused before any sign-in.
fn invite_info(code: &str) -> Result<coven::InviteInfo, RestoreFromCodeError> {
    match coven::decode_code_info(code) {
        Ok(coven::CodeInfo::Invite(invite)) => Ok(invite),
        Ok(coven::CodeInfo::Restore(_)) => {
            Err(restore_error(coven::BootstrapError::Code(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Invite,
                actual: coven::CodeKind::Restore,
            })))
        }
        Err(error) => Err(restore_error(coven::BootstrapError::Code(error))),
    }
}

/// Ask for the joined person's profile with the invite's name; an invite
/// that replaced an existing member's keys names nobody, and asks for
/// nothing.
async fn with_profile(
    handle: coven::CovenHandle,
    invitee: Option<&str>,
    clock: coven::ClockRef,
) -> JoinedLibrary {
    let profile = match invitee {
        Some(name) => Some(
            crate::db::Database::from_handle(
                handle.clone(),
                clock,
                std::sync::Arc::new(coven::UuidIds),
            )
            .create_profile(name)
            .await,
        ),
        None => None,
    };
    (handle, profile)
}

/// Replace this device after the caller has warned that unsent edits are discarded.
/// Coven checks storage and custody before replacing the installation. Calling
/// it again finishes a reset that stopped after the new copy took the old
/// one's place; for a restore that stopped there, coven refuses it and changes
/// nothing. The reset loads the whole store from storage, as a restore does,
/// and reports how far it has come to `on_status`. The returned handle is
/// ready to open in bae; there is no post-reset notification.
pub async fn reset_device(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    loading_builder(builder)
        .reset_device(store, device_name, on_status)
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
/// synced are discarded. When this device's custody already holds the code's
/// keys, the call just opens the library, and that open's steps go to
/// `on_open_progress`, as any open's do; every other status goes to
/// `on_status`.
pub async fn restore_replacing_device(
    mut builder: coven::CovenBuilder,
    store: coven::StoreId,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>),
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
    let status = |status| match status {
        coven::BootstrapStatus::Opening(progress) => on_open_progress(progress),
        status => on_status(status),
    };
    loading_builder(builder)
        .restore_device(store, code, s3_credentials, device_name, status, cancel)
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
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        match self {
            Self::Keychain(error) => crate::diagnostics::bootstrap_category(error),
            Self::Stores(_) => crate::diagnostics::UiErrorCategory::Config,
        }
    }
}

/// Why a saved restore code couldn't be deleted from the keychain.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct ForgetSavedRestoreCodeError(#[from] coven::BootstrapError);

impl ForgetSavedRestoreCodeError {
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        crate::diagnostics::bootstrap_category(&self.0)
    }
}

/// Whether listing the saved codes failed only because this device has
/// nowhere to keep them: coven answers `Unsupported` there, which says the
/// device keeps no saved codes, not that reading them failed.
fn keeps_no_saved_codes(error: &coven::BootstrapError) -> bool {
    matches!(error, coven::BootstrapError::SecureStorage(coven::KeyError::Unsupported))
}

async fn restorable_codes(
    codes: Vec<coven::SavedRestoreEntry>,
    layout: &coven::StoreLayout,
) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
    let here: std::collections::HashSet<_> =
        layout.stores().await?.into_iter().map(|store| store.id).collect();
    Ok(codes.into_iter().filter(|code| !here.contains(&code.store_id())).collect())
}

fn restore_error(error: coven::BootstrapError) -> RestoreFromCodeError {
    match error {
        coven::BootstrapError::Cancelled => RestoreFromCodeError::Cancelled,
        error => RestoreFromCodeError::Restore(Box::new(error)),
    }
}
