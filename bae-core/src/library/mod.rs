mod album_selection;
pub mod app_services;
mod artist_search;
mod bootstrap_auth;
mod browse;
mod detail;
pub mod download_snapshot;
mod file_transfers;
mod library_status;
mod live_read;
mod local_lifecycle;
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
pub(crate) mod storage_transitions;
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
pub use detail::{DetailSnapshot, DetailSubscription};
pub use download_snapshot::{
    DownloadEntry, DownloadPhase, DownloadProgress, DownloadRow, DownloadSnapshot,
    DownloadTransferProgress, QueuedDownload,
};
pub use file_transfers::{FileTransferChange, FileTransfersWatch};
pub use library_status::{
    LibraryStatusSnapshot, LibraryStatusSubscription, LibraryStatusSubscriptionError,
};
pub use live_read::{LiveRead, LiveReadError};
pub use local_lifecycle::{
    finish_removals, remove_local_library, remove_unopenable_library, unsent_before_removal,
    LibraryRemovalError, UnfinishedRemoval,
};
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
    DownloadTransferChange, DownloadTransfersWatch, OutputTransferChange, OutputTransfersWatch,
    ReleaseTransferChange, ReleaseTransfersWatch,
};
pub use search::{
    LibrarySearchQuery, LibrarySearchSnapshot, LibrarySearchSubscription,
    LibrarySearchSubscriptionError, SEARCH_RESULT_LIMIT,
};
pub use storage_browse::{
    ReleaseQueueRun, StorageBrowseSnapshot, StorageBrowseSubscription, StorageBrowseView,
    StorageQueues, UploadQueueRun,
};
pub use sync_activity::{
    DownloadsInFlight, SyncActivity, SyncActivityStream, UploadQueueCounts, UploadsInFlight,
};
pub use upload_progress::{UploadActivity, UploadBar, UploadProgress, UploadState};

#[cfg(test)]
mod creation_tests;
#[cfg(test)]
mod local_lifecycle_tests;
#[cfg(test)]
mod saved_codes_tests;

use crate::config::AppDir;
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
    /// This device's preferences for the library, which set its transfer
    /// limits, couldn't be read.
    #[error("read the library's preferences: {0}")]
    Preferences(#[source] crate::config::ConfigError),
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
            Self::Preferences(_) => crate::ui::UiErrorCategory::Config,
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

/// Why the folder for SQLite's temporary tables couldn't be named.
#[cfg(target_os = "android")]
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct TemporaryDirectoryError(#[from] coven::TemporaryDirectoryError);

#[cfg(target_os = "android")]
impl TemporaryDirectoryError {
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        crate::ui::UiErrorCategory::Config
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
    builder
        .max_concurrent_uploads(limits.uploads)
        .max_concurrent_downloads(limits.downloads)
}

/// The transfer limits this device's preferences for `store` set, which the
/// library runs under once open. A restore or join runs under them too: it
/// reads the library's history from storage under the download limit.
fn library_transfers(
    app_dir: &AppDir,
    store: coven::StoreId,
) -> Result<coven::TransferLimits, RestoreFromCodeError> {
    Ok(crate::config::Config::load(app_dir, store)
        .map_err(RestoreFromCodeError::Preferences)?
        .prefs
        .transfer_limits())
}

/// `builder` configured as bae's store for loading `store` from storage,
/// under [`library_transfers`].
fn loading_builder(
    builder: coven::CovenBuilder,
    app_dir: &AppDir,
    store: coven::StoreId,
) -> Result<coven::CovenBuilder, RestoreFromCodeError> {
    Ok(with_transfer_limits(
        configure_store(builder),
        library_transfers(app_dir, store)?,
    ))
}

/// bae's store as every handle opens it: its synced tables, its schema, and
/// the collation and SQL functions the schema names.
pub fn configure_store(builder: coven::CovenBuilder) -> coven::CovenBuilder {
    builder
        .synced_tables(crate::sync::synced_tables())
        .namespaces(crate::sync::namespaces())
        .migrations(crate::migrations::all())
        .collation(
            crate::migrations::NATURAL_ORDER,
            crate::migrations::NATURAL_ORDER_VERSION,
            crate::migrations::natural_order,
        )
        .function(
            "bae_fold_case",
            1,
            coven::Determinism::Deterministic { version: 1 },
            |arguments| Ok(fold_case_value(arguments.get(0)?)),
        )
        .function(
            "bae_search_pattern",
            1,
            coven::Determinism::Deterministic { version: 1 },
            |arguments| Ok(search_pattern_value(arguments.get(0)?)),
        )
        .function(
            "bae_catalog_rank",
            1,
            coven::Determinism::Deterministic { version: 1 },
            |arguments| catalog_rank_value(arguments.get(0)?),
        )
        .function(
            "bae_folder_below",
            2,
            coven::Determinism::Deterministic { version: 2 },
            |arguments| folder_below_value(arguments.get(0)?, arguments.get(1)?),
        )
        .function(
            "bae_folder_relative",
            2,
            coven::Determinism::Deterministic { version: 2 },
            |arguments| folder_relative_value(arguments.get(0)?, arguments.get(1)?),
        )
}

/// `bae_fold_case`: a text folded as library search folds it; NULL stays NULL.
fn fold_case_value(text: Option<String>) -> Option<String> {
    text.map(|text| search::fold_case(&text))
}

/// `bae_search_pattern`: the `GLOB` pattern library search matches a query
/// by; NULL stays NULL.
fn search_pattern_value(query: Option<String>) -> Option<String> {
    query.map(|query| search::search_pattern(&query))
}

/// `bae_catalog_rank`: a stored catalog's place in the order surfaces list
/// catalogs (`Catalog::ALL`). Part of the schema once an index uses it: the
/// order must not change.
fn catalog_rank_value(catalog: String) -> rusqlite::Result<i64> {
    let catalog: bae_catalogs::catalog::Catalog = catalog
        .parse()
        .map_err(|error: String| rusqlite::Error::UserFunctionError(error.into()))?;
    Ok(catalog.rank() as i64)
}

/// `bae_folder_below`: the absolute path's bytes of a folder below a watched
/// folder, from the root's bytes and the folder's [`RelativeFolder`] bytes,
/// spelled as a scan spells it, which is how a stored reading names the
/// folder it reads. Takes BLOBs only: rusqlite refuses TEXT for a `Vec<u8>`,
/// so a path bound as text fails rather than matching nothing.
///
/// [`RelativeFolder`]: crate::import::RelativeFolder
fn folder_below_value(root: Vec<u8>, relative: Vec<u8>) -> rusqlite::Result<Vec<u8>> {
    let root = coven::path_from_bytes(root)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
    let relative = crate::import::RelativeFolder::from_bytes(relative)
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))?;
    Ok(coven::path_bytes(
        &crate::import::relative_folder::folder_below(&root, &relative),
    ))
}

/// `bae_folder_relative`: the [`RelativeFolder`] bytes of a folder below a
/// watched folder, empty for the watched folder itself, NULL for one outside
/// it. Takes BLOBs only, as `bae_folder_below` does.
///
/// [`RelativeFolder`]: crate::import::RelativeFolder
fn folder_relative_value(root: Vec<u8>, folder: Vec<u8>) -> rusqlite::Result<Option<Vec<u8>>> {
    let to_error =
        |error: coven::PathBytesError| rusqlite::Error::UserFunctionError(Box::new(error));
    let root = coven::path_from_bytes(root).map_err(to_error)?;
    let folder = coven::path_from_bytes(folder).map_err(to_error)?;
    crate::import::relative_folder::folder_relative_below(&root, &folder)
        .map(|relative| relative.map(|relative| relative.as_bytes().to_vec()))
        .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
}

/// An in-memory database on bae's schema outside coven, with the SQL
/// functions the schema calls: for unit tests of SQL that runs on a bare
/// connection.
#[cfg(test)]
pub(crate) fn bare_library_connection() -> rusqlite::Connection {
    use rusqlite::functions::FunctionFlags;
    let conn = rusqlite::Connection::open_in_memory().expect("an in-memory database opens");
    conn.create_scalar_function(
        "bae_fold_case",
        1,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_DETERMINISTIC
            | FunctionFlags::SQLITE_INNOCUOUS,
        |context| Ok(fold_case_value(context.get(0)?)),
    )
    .expect("bae_fold_case registers");
    conn.create_scalar_function(
        "bae_search_pattern",
        1,
        FunctionFlags::SQLITE_UTF8
            | FunctionFlags::SQLITE_DETERMINISTIC
            | FunctionFlags::SQLITE_INNOCUOUS,
        |context| Ok(search_pattern_value(context.get(0)?)),
    )
    .expect("bae_search_pattern registers");
    let deterministic = FunctionFlags::SQLITE_UTF8
        | FunctionFlags::SQLITE_DETERMINISTIC
        | FunctionFlags::SQLITE_INNOCUOUS;
    conn.create_scalar_function("bae_catalog_rank", 1, deterministic, |context| {
        catalog_rank_value(context.get(0)?)
    })
    .expect("bae_catalog_rank registers");
    conn.create_scalar_function("bae_folder_below", 2, deterministic, |context| {
        folder_below_value(context.get(0)?, context.get(1)?)
    })
    .expect("bae_folder_below registers");
    conn.create_scalar_function("bae_folder_relative", 2, deterministic, |context| {
        folder_relative_value(context.get(0)?, context.get(1)?)
    })
    .expect("bae_folder_relative registers");
    // coven registers `coven_path_text` on every connection it opens; this one
    // shows a path's bytes the same way, through coven's exports.
    conn.create_scalar_function("coven_path_text", 1, deterministic, |context| {
        context
            .get::<Option<Vec<u8>>>(0)?
            .map(|bytes| {
                coven::path_from_bytes(bytes)
                    .map(|path| coven::path_text(&path))
                    .map_err(|error| rusqlite::Error::UserFunctionError(Box::new(error)))
            })
            .transpose()
    })
    .expect("coven_path_text registers");
    conn.create_collation(
        crate::migrations::NATURAL_ORDER,
        crate::migrations::natural_order,
    )
    .expect("natural_order registers");
    // The schema's triggers follow coven's `coven_files`, `coven_pins` and
    // `coven_upload_queue`, which coven creates before it migrates; these
    // tests record, pin and queue nothing, so empty tables of the same shape
    // let the triggers exist.
    conn.execute_batch(
        "CREATE TABLE coven_files (id TEXT PRIMARY KEY NOT NULL, audience TEXT NOT NULL, \
         size INTEGER NOT NULL, hash BLOB, coordinates TEXT NOT NULL, \
         namespace TEXT NOT NULL) STRICT, WITHOUT ROWID; \
         CREATE TABLE coven_pins (file TEXT PRIMARY KEY NOT NULL) STRICT, WITHOUT ROWID; \
         CREATE TABLE coven_upload_queue (file TEXT PRIMARY KEY NOT NULL, \
         position INTEGER NOT NULL) STRICT, WITHOUT ROWID;",
    )
    .expect("the stand-in coven tables create");
    conn.execute_batch(include_str!("../../migrations/001_initial.sql"))
        .expect("the schema applies");
    conn
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
    app_dir: &AppDir,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    let metadata = bootstrap_auth::code_metadata(code)?;
    bootstrap_auth::authenticate(&mut builder, metadata.oauth_access, cancel).await?;
    coven::restore_from_code(
        loading_builder(builder, app_dir, metadata.store_id)?,
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
    app_dir: &AppDir,
    saved: &coven::SavedRestoreCode,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
) -> Result<Option<coven::CovenHandle>, RestoreFromCodeError> {
    bootstrap_auth::authenticate(&mut builder, saved.location.oauth_access(), cancel).await?;
    coven::restore_from_saved_code(
        loading_builder(builder, app_dir, saved.store_id)?,
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
/// The profile is `None` for an invite that replaced an existing member's
/// keys: that person keeps the row they already have (§5.4).
pub type JoinedLibrary = (
    coven::CovenHandle,
    Option<Result<crate::people::PersonProfile, crate::people::ProfileError>>,
);

/// The app's hold on a running join: cancel it before its request is sent, or
/// check for approval at once. One control covers coven's join and any
/// provider sign-in bae runs for it.
pub struct JoinControl {
    sign_in: watch::Sender<bool>,
    join: coven::JoinControl,
}

impl JoinControl {
    pub fn new() -> Self {
        Self {
            sign_in: watch::channel(false).0,
            join: coven::JoinControl::new(),
        }
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
    app_dir: &AppDir,
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
        loading_builder(builder, app_dir, invite.metadata.store_id)?,
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
    app_dir: &AppDir,
    join: &coven::InviteInfo,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let (store, on_status, coven_control) = (join.metadata.store_id, &on_status, &control.join);
    let limits = library_transfers(app_dir, store)?;
    let resume = move |builder: coven::CovenBuilder| {
        Box::pin(
            with_transfer_limits(configure_store(builder), limits).resume_join(
                store,
                on_status,
                coven_control,
            ),
        )
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
        Ok(coven::CodeInfo::Restore(_)) => Err(restore_error(coven::BootstrapError::Code(
            coven::CodeError::WrongKind {
                expected: coven::CodeKind::Invite,
                actual: coven::CodeKind::Restore,
            },
        ))),
        Err(error) => Err(restore_error(coven::BootstrapError::Code(error))),
    }
}

/// Write the joined person's profile with the invite's name; an invite that
/// replaced an existing member's keys names nobody, and writes nothing.
async fn with_profile(
    handle: coven::CovenHandle,
    invitee: Option<&str>,
    clock: coven::ClockRef,
) -> JoinedLibrary {
    let profile = match invitee {
        Some(name) => Some(
            crate::db::Database::from_handle(handle.clone(), clock)
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
/// nothing. The returned handle is ready to open in bae; there is no post-reset
/// notification.
pub async fn reset_device(
    builder: coven::CovenBuilder,
    app_dir: &AppDir,
    store: coven::StoreId,
    device_name: &str,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    loading_builder(builder, app_dir, store)?
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
    app_dir: &AppDir,
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
    loading_builder(builder, app_dir, store)?
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

/// Whether listing the saved codes failed only because this device has
/// nowhere to keep them: coven answers `Unsupported` there, which says the
/// device keeps no saved codes, not that reading them failed.
fn keeps_no_saved_codes(error: &coven::BootstrapError) -> bool {
    matches!(
        error,
        coven::BootstrapError::SecureStorage(coven::KeyError::Unsupported)
    )
}

async fn restorable_codes(
    codes: Vec<coven::SavedRestoreEntry>,
    layout: &coven::StoreLayout,
) -> Result<Vec<coven::SavedRestoreEntry>, SavedRestoreCodesError> {
    let here: std::collections::HashSet<_> = layout
        .stores()
        .await?
        .into_iter()
        .map(|store| store.id)
        .collect();
    Ok(codes
        .into_iter()
        .filter(|code| !here.contains(&code.store_id()))
        .collect())
}

fn restore_error(error: coven::BootstrapError) -> RestoreFromCodeError {
    match error {
        coven::BootstrapError::Cancelled => RestoreFromCodeError::Cancelled,
        error => RestoreFromCodeError::Restore(Box::new(error)),
    }
}
