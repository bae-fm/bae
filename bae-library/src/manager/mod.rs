//! The library's operations over its store: what a person does to the
//! library, and the work its transfers, sync and outputs run.
//!
//! `LibraryManager` owns the database handle. Its surface is split into
//! per-entity modules — `release`, `album`, `track`, `artist`, `image`,
//! `export`, `storage`, `sync` — each holding that entity's operations,
//! reading `&Database` / `&CovenHandle` for the covers, pin state and joins
//! an operation needs. It reads none of the library's views: what a screen
//! shows of the library is `bae_views`', which reads the same store.

use std::future::Future;
#[cfg(desktop)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use tracing::{debug, warn};

#[cfg(desktop)]
use crate::save::SaveService;
use crate::sync::S3ConfigData;
use bae_diagnostics::{Diagnostics, SyncOperation, TelemetryEvent};
use bae_records::GallerySource;
#[cfg(desktop)]
use bae_records::{DbAlbum, DbArtist, DbTrack};
use bae_records::{DbFile, DbLibraryImage, DbRelease, ImageRef, LibraryImageType};
use bae_store::Store;
use bae_sync_status::SyncStatus;
use bae_types::release_storage::ReleaseStorageAction;
use bae_util::task_failure::{caught, joined, Bug, TaskFailed};
use coven::ClockRef;
use coven::IdSourceRef;

mod service;

mod album;
mod artist;
mod collections;
mod config;
mod coven_files;
mod downloads;
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
mod fixture;
#[cfg(all(any(test, feature = "test-utils", debug_assertions), desktop))]
pub use fixture::{
    FixtureAlbum, FixtureCandidate, FixtureCandidateState, FixturePlaying, FixtureWatchedFolder,
    LibraryFixture, LibraryFixtureError, FIXTURE_IMPORT_FAILURE, FIXTURE_LOOKUP_FAILURE,
};
/// Desktop-only, under the same predicate as the rest of the export surface (the
/// queue field below, and `crate::save`). Exporting writes a directory tree
/// next to the user's chosen folder — a hidden staging sibling, a marker file, a
/// rename into place — which a sandboxed iOS/Android document URL does not offer,
/// and the preset half of it needs the desktop-only encoder in `crate::save`.
/// Compiling the producers out with the worker is what makes a mobile export call
/// a compile error rather than a release that sits `Queued` with nothing to run it.
#[cfg(desktop)]
mod export;
mod image;
mod lifecycle;
mod locality;
#[cfg(desktop)]
mod output;
mod people;
mod pin;
mod playback_state;
#[cfg(desktop)]
mod records;
mod release;
#[cfg(desktop)]
mod release_edit;
#[cfg(desktop)]
mod release_metadata;
pub use people::DeviceCutOff;
#[cfg(desktop)]
pub use release_edit::ReleaseEdit;
#[cfg(desktop)]
mod save;
#[cfg(desktop)]
pub(crate) use save::SaveWindow;
mod storage;
mod sync;
use bae_diagnostics::{
    cloud_unlock_category, device_only_category, file_read_category, key_category,
    storage_error_category, storage_setup_category, sync_category,
};
pub use sync::storage_setup_finishing;
mod track;
mod uploads;

/// Outcome of `resolve_records_target_album` — where a release should
/// land after a `set_records` call. `new_album` carries the album row
/// to insert when the target is brand-new; otherwise the target is an
/// existing album and `new_album` is `None`.
#[cfg(desktop)]
struct RecordsTargetAlbum {
    album_id: String,
    new_album: Option<DbAlbum>,
}

/// What asked for a release: an edit of it (its records, its metadata, its
/// deletion), an export, a save, or a transfer of its files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseWork {
    Edit,
    Export,
    Save,
    Transfer,
}

impl LibraryError {
    /// The refusal of `work` asked of `release_id`, which the library no
    /// longer holds.
    pub fn release_missing(release_id: &str, work: ReleaseWork) -> Self {
        Self::ReleaseMissing { release_id: release_id.to_string(), work }
    }
}

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] bae_store::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// An import step the library ran failed — reading a catalog release
    /// against a release's tracks, fetching a cover, a candidate's paths —
    /// as the import error itself, so every surface words the refusal it is
    /// (another number of tracks, discs that fit alike) rather than a
    /// generic line.
    #[error(transparent)]
    #[cfg(desktop)]
    Import(Box<bae_import::ImportError>),
    /// An album an operation names is not in the library: it was deleted,
    /// here or on another device, or holds no release, before the operation
    /// read it.
    #[error("album {album_id} is not in the library")]
    AlbumMissing { album_id: String },
    /// A file an operation names is not one of the release's: it left the
    /// release before the operation read it.
    #[error("file {file_id} is not part of release {release_id}")]
    FileMissing { release_id: String, file_id: String },
    /// An edit lays out another number of tracks than the release holds:
    /// tracks came or went after the edit was composed.
    #[error("the release has {release_tracks} tracks, the edit supplies {edit_tracks}")]
    TrackCountChanged { release_tracks: usize, edit_tracks: usize },
    /// An edit session's editor named a track row its session doesn't hold.
    #[error("release edit has no track row '{0}'")]
    EditRowMissing(String),
    /// A release an operation names is not in the library: it was deleted,
    /// here or on another device, before the operation read it. `work` is
    /// what asked for it, whose line the refusal shows under: an edit's
    /// says what it acted on changed meanwhile.
    #[error("release {release_id} is not in the library")]
    ReleaseMissing { release_id: String, work: ReleaseWork },
    /// A verbatim release export (reproducing the imported file set) failed.
    #[error("Export error: {0}")]
    Export(String),
    /// A save (rendered output — track or release) failed.
    #[error("Save error: {0}")]
    Save(String),
    /// What the person typed failed its invariants (a blank album title, no
    /// album artist, a blank credit name). The editor's typed error, so every
    /// surface reports the same rule rather than its own hand-written sentence.
    #[error(transparent)]
    Edit(#[from] bae_types::edit_validation::EditValidationError),
    #[error("Release editor cannot load: {0}")]
    #[cfg(desktop)]
    ReleaseEditLoad(#[from] bae_import::ReleaseEditLoadError),
    /// A release edit session's save found the release changed since the
    /// session opened on it, and wrote nothing.
    #[error("Release '{release_id}' changed since its edit session opened")]
    ReleaseEditStale { release_id: String },
    #[error("Track mapping error: {0}")]
    TrackMapping(String),
    #[error("Encryption error: {0}")]
    Encryption(#[from] coven::CryptoError),
    #[error("Storage error: {0}")]
    Storage(String),
    /// Reading, pinning, or unpinning a file's bytes through coven failed.
    #[error("{operation}: {error}")]
    FileRead {
        operation: String,
        #[source]
        error: Box<coven::FileReadError>,
    },
    /// An image's replacement did not commit: another write of the image
    /// landed after it was planned, or the store failed.
    #[error(transparent)]
    ImageWrite(bae_records::ImageWriteError),
    /// A write's credits cannot be written as the library stands.
    #[error(transparent)]
    Identity(bae_records::IdentityRefusal),
    /// A deletion did not commit: what it deletes changed after it was
    /// planned, or the store failed.
    #[error(transparent)]
    Deletion(bae_records::DeletionError),
    /// A write moving releases to the cloud did not commit: no storage is set
    /// up, or the store failed.
    #[error(transparent)]
    StorageMove(bae_transfers::StorageMoveError),
    /// A dismissal of a storage setup that ended undone did not commit: the
    /// operation is no such setup, or the store failed.
    #[error(transparent)]
    SetupDismiss(bae_sync_status::SetupDismissError),
    /// A dismissal of a finished admin change did not commit: the operation
    /// is no admin change of this device's, or the store failed.
    #[error(transparent)]
    AdminChangeDismiss(bae_people::admin_change::AdminChangeDismissError),
    /// Making a release its album's primary did not commit: the release is
    /// not one of the album's, or the store failed.
    #[error(transparent)]
    PrimaryRelease(bae_records::PrimaryReleaseError),
    /// Adding or renaming an artist's other printed name did not commit:
    /// the artist is not in the library, or the store failed.
    #[error(transparent)]
    CreditName(bae_records::CreditNameError),
    /// Making a release local again failed; coven removed what the call
    /// made, so the release stays in the cloud.
    #[error("make release local: {0}")]
    MakeLocal(#[source] Box<coven::DeviceOnlyError>),
    #[error("Playback error: {0}")]
    Playback(String),
    /// Work asked of a library after it closed; nothing of it started.
    #[error("{0} cannot start: the library is closed")]
    Closed(&'static str),
    /// An internal invariant the caller can't act on (a missing platform driver,
    /// a provider used through the wrong entry point).
    #[error("{0}")]
    Internal(String),
    /// The OS keyring couldn't be read or written (encryption key, storage
    /// credentials, Discogs/MCP tokens).
    #[error("Keyring error: {0}")]
    Keyring(#[from] coven::KeyError),
    /// Reading or writing bae's directory failed: the library list, the
    /// active-library pointer, a removal record.
    #[error("Config error: {0}")]
    AppDir(#[from] bae_app_dir::AppDirError),
    /// Reading or writing this device's preferences failed, or a change to
    /// them was refused.
    #[error("Config error: {0}")]
    Preferences(#[from] bae_preferences::PreferencesError),
    /// Building or probing storage failed — bad credentials/config
    /// (`Configuration`) or an unreachable backend (`Transport`).
    #[error("Storage error: {0}")]
    StorageAccess(#[from] coven::StorageError),
    #[error("Storage setup error: {0}")]
    StorageSetup(#[from] coven::StorageSetupError),
    #[error("Cloud unlock error: {0}")]
    CloudUnlock(#[from] coven::StoreKeyUnlockError),
    /// A sync-manager connection or membership operation failed.
    #[error("Sync error: {0}")]
    Sync(#[from] coven::SyncError),
    /// The restore code coven made for the Add Device QR code doesn't decode.
    #[error("the restore code for the QR code does not decode: {0}")]
    TransferCode(#[source] coven::CodeError),
    /// An invite could no longer be cancelled: approval of its person has
    /// begun, and stopping would leave them half joined, or it has ended.
    #[error("the invite can no longer be cancelled: {0}")]
    InviteJoining(#[source] coven::SyncError),
    /// Handing one blocked sync operation back to the sync loop failed — most
    /// often because it is no longer blocked, so there is nothing to retry.
    #[error("Retry blocked operation error: {0}")]
    RetryBlockedOperation(#[source] Box<coven::OperationError>),
    /// A bae-level input validation failed (an empty library name). bae's own
    /// validation, not coven's — the detail is log-only, so the user sees the
    /// generic settings line.
    #[error("{0}")]
    Validation(String),
    #[error("{operation}; cleanup also failed: {cleanup}")]
    Cleanup { operation: Box<LibraryError>, cleanup: Box<LibraryError> },
}

impl LibraryError {
    pub(crate) fn file_read(operation: impl Into<String>, error: coven::FileReadError) -> Self {
        Self::FileRead { operation: operation.into(), error: Box::new(error) }
    }
}

impl From<bae_records::TrackAudioError> for LibraryError {
    fn from(error: bae_records::TrackAudioError) -> Self {
        match error {
            bae_records::TrackAudioError::Database(error) => Self::Database(error),
            error => Self::TrackMapping(error.to_string()),
        }
    }
}

impl From<coven::DbError> for LibraryError {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

impl From<coven::CovenError> for LibraryError {
    fn from(error: coven::CovenError) -> Self {
        Self::Database(error.into())
    }
}

/// A refused write whose rollback or byte removal failed too is the refusal
/// with each failed cleanup after it.
impl From<bae_records::IdentityWriteError> for LibraryError {
    fn from(error: bae_records::IdentityWriteError) -> Self {
        use bae_records::IdentityWriteRefusal as R;
        use bae_store::WriteError as E;
        match error {
            E::Database(error) => Self::Database(error),
            E::Refused(R::Identity(refusal)) => Self::Identity(refusal),
            E::Refused(R::StoredName(error)) => error.into(),
            E::Rollback { operation, rollback } => Self::Cleanup {
                operation: Box::new((*operation).into()),
                cleanup: Box::new(Self::Database(rollback.into())),
            },
            E::FileCleanup { write, failures } => {
                failures.into_iter().fold((*write).into(), |operation, failure| Self::Cleanup {
                    operation: Box::new(operation),
                    cleanup: Box::new(Self::Database(failure.into())),
                })
            }
        }
    }
}

#[derive(Debug)]
pub struct UploadReceipt {
    pub release_ids: Vec<String>,
}

#[derive(Debug)]
pub struct UploadBatchFailure {
    pub release_ids: Vec<String>,
    pub error: bae_diagnostics::UiError,
}

#[derive(Debug)]
pub enum UploadReleasesOutcome {
    Complete { receipt: UploadReceipt },
    Partial { receipt: Option<UploadReceipt>, failure: UploadBatchFailure },
}

/// A stored name that names no file is a row no writer here produces, and
/// what needed it does not run.
impl From<bae_records::stored_name::StoredNameError> for LibraryError {
    fn from(error: bae_records::stored_name::StoredNameError) -> Self {
        let what = format!("release {} {}", error.release_id, error.label);
        Self::Database(bae_store::DbError::unreadable(what, error.value))
    }
}

/// A bug a coven call carried back, which its owner reported as it
/// returned, or one coven repeats; whoever meets the failure above shows it
/// and doesn't report it again.
impl bae_util::task_failure::ReportedBug for LibraryError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        use coven::task::CarriesBug;
        match self {
            LibraryError::Database(error) => error.reported_bug(),
            LibraryError::Io(error) => error.reported_bug(),
            LibraryError::Preferences(error) => error.reported_bug(),
            #[cfg(desktop)]
            LibraryError::Import(error) => error.reported_bug(),
            LibraryError::FileRead { error, .. } => error.reported_bug(),
            LibraryError::MakeLocal(error) => error.reported_bug(),
            LibraryError::StorageMove(error) => error.reported_bug(),
            LibraryError::SetupDismiss(error) => error.reported_bug(),
            LibraryError::AdminChangeDismiss(error) => error.reported_bug(),
            LibraryError::PrimaryRelease(error) => error.reported_bug(),
            LibraryError::CreditName(error) => error.reported_bug(),
            LibraryError::Deletion(error) => error.reported_bug(),
            LibraryError::ImageWrite(error) => error.reported_bug(),
            LibraryError::Sync(error) | LibraryError::InviteJoining(error) => error.reported_bug(),
            LibraryError::Keyring(error) => error.bug(),
            LibraryError::AppDir(error) => error.bug(),
            LibraryError::StorageAccess(error) => error.bug(),
            LibraryError::StorageSetup(error) => error.bug(),
            LibraryError::CloudUnlock(error) => error.bug(),
            LibraryError::Cleanup { operation, cleanup } => {
                operation.reported_bug().or_else(|| cleanup.reported_bug())
            }
            _ => None,
        }
    }
}

impl LibraryError {
    /// The user-facing diagnostic class the bridge renders. Membership/setup
    /// failures carry the distinctions the cloud-setup and sharing flows show as
    /// different messages (bad credentials vs unreachable backend vs keyring vs
    /// membership); everything else maps to its general class.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            LibraryError::Database(error) => bae_diagnostics::database_category(error),
            LibraryError::AppDir(error) => error.category(),
            LibraryError::Preferences(error) => error.category(),
            LibraryError::Validation(_) => C::Config,
            LibraryError::Keyring(error) => key_category(error),
            LibraryError::StorageAccess(e) => storage_error_category(e),
            LibraryError::StorageSetup(error) => storage_setup_category(error),
            LibraryError::CloudUnlock(error) => cloud_unlock_category(error),
            LibraryError::Sync(e) => sync_category(e),
            LibraryError::InviteJoining(_) => C::InviteJoining,
            LibraryError::RetryBlockedOperation(_) => C::Membership,
            #[cfg(desktop)]
            LibraryError::Import(error) => error.category(),
            LibraryError::AlbumMissing { .. }
            | LibraryError::FileMissing { .. }
            | LibraryError::TrackCountChanged { .. } => C::ChangedMeanwhile,
            LibraryError::EditRowMissing(_) => C::Internal,
            LibraryError::Edit(reason) => C::InvalidEdit(*reason),
            LibraryError::ReleaseMissing { work, .. } => match work {
                ReleaseWork::Edit => C::ChangedMeanwhile,
                ReleaseWork::Export => C::Export,
                ReleaseWork::Save => C::Save,
                // A transfer has no line of its own, so its refusal shows
                // the internal one.
                ReleaseWork::Transfer => C::Internal,
            },
            LibraryError::ReleaseEditStale { .. } => C::ReleaseChangedSinceOpened,
            LibraryError::Export(_) => C::Export,
            LibraryError::Save(_) => C::Save,
            LibraryError::FileRead { error, .. } => file_read_category(error),
            LibraryError::MakeLocal(error) => device_only_category(error),
            LibraryError::StorageMove(error) => {
                write_error_category(error, storage_move_refusal_category)
            }
            LibraryError::SetupDismiss(error) => setup_dismiss_category(error),
            LibraryError::AdminChangeDismiss(error) => admin_change_dismiss_category(error),
            LibraryError::PrimaryRelease(error) => primary_release_category(error),
            LibraryError::CreditName(error) => credit_name_category(error),
            LibraryError::Deletion(error) => write_error_category(error, deletion_refusal_category),
            LibraryError::ImageWrite(error) => {
                write_error_category(error, image_write_refusal_category)
            }
            LibraryError::Identity(bae_records::IdentityRefusal::ArtistGone(_)) => {
                C::ChangedMeanwhile
            }
            LibraryError::Identity(bae_records::IdentityRefusal::ConflictingSourceIds(_)) => {
                C::ConflictingSourceIds
            }
            LibraryError::Cleanup { operation, .. } => operation.category(),
            #[cfg(desktop)]
            LibraryError::ReleaseEditLoad(_) => C::Internal,
            LibraryError::Io(_)
            | LibraryError::TrackMapping(_)
            | LibraryError::Encryption(_)
            | LibraryError::Storage(_)
            | LibraryError::TransferCode(_)
            | LibraryError::Playback(_)
            | LibraryError::Closed(_)
            | LibraryError::Internal(_) => C::Internal,
        }
    }
}

/// Work run apart from its caller gave no result: it panicked (a bug the
/// caller has reported), or its runtime is shutting down under it.
/// A catalog session that could not be had failed on the library's own
/// store: its preferences or its keyring.
#[cfg(desktop)]
impl From<bae_import::CatalogSessionError> for LibraryError {
    fn from(error: bae_import::CatalogSessionError) -> Self {
        match error {
            bae_import::CatalogSessionError::Preferences(error) => Self::Database(error),
            bae_import::CatalogSessionError::Keyring(error) => Self::Keyring(error),
        }
    }
}

/// The class a surface words an image replacement's refusal in: when another
/// write of the image beat it, the image changed meanwhile, and the
/// replacement is the person's to make again, over the image that landed.
fn image_write_refusal_category(
    refusal: &bae_records::ImageWriteRefusal,
) -> bae_diagnostics::UiErrorCategory {
    match refusal {
        bae_records::ImageWriteRefusal::Superseded { .. } => {
            bae_diagnostics::UiErrorCategory::ChangedMeanwhile
        }
    }
}

/// An image write that failed in the store is the library's store failure.
impl From<bae_records::ImageWriteError> for LibraryError {
    fn from(error: bae_records::ImageWriteError) -> Self {
        match error {
            bae_store::WriteError::Database(error) => Self::Database(error),
            error => Self::ImageWrite(error),
        }
    }
}

/// The class a surface words a store write's failure in: the store's own by
/// `database_category`, a refusal by `refusal`, and one with a failed rollback
/// or byte removal kept beside it by what failed first.
fn write_error_category<R>(
    error: &bae_store::WriteError<R>,
    refusal: impl Fn(&R) -> bae_diagnostics::UiErrorCategory + Copy,
) -> bae_diagnostics::UiErrorCategory {
    use bae_store::WriteError as E;
    match error {
        E::Refused(reason) => refusal(reason),
        E::Database(error) => bae_diagnostics::database_category(error),
        E::Rollback { operation: failed, .. } | E::FileCleanup { write: failed, .. } => {
            write_error_category(failed, refusal)
        }
    }
}

/// The class a surface words a deletion's failure in: one refused because
/// what it deletes changed meanwhile is the person's to ask for again, as an
/// edit of a release that is gone is.
fn deletion_refusal_category(
    refusal: &bae_records::DeletionRefusal,
) -> bae_diagnostics::UiErrorCategory {
    match refusal {
        bae_records::DeletionRefusal::PlanChanged(_) => {
            bae_diagnostics::UiErrorCategory::ChangedMeanwhile
        }
    }
}

/// A deletion that failed in the store is the library's store failure.
impl From<bae_records::DeletionError> for LibraryError {
    fn from(error: bae_records::DeletionError) -> Self {
        match error {
            bae_store::WriteError::Database(error) => Self::Database(error),
            error => Self::Deletion(error),
        }
    }
}

/// The class a surface words a refused move of releases' files in: a move
/// needs storage, which is for the person to set up.
fn storage_move_refusal_category(
    refusal: &bae_transfers::StorageMoveRefusal,
) -> bae_diagnostics::UiErrorCategory {
    match refusal {
        bae_transfers::StorageMoveRefusal::NoStorage => bae_diagnostics::UiErrorCategory::Config,
    }
}

/// A move's write that failed in the store is the library's store failure.
impl From<bae_transfers::StorageMoveError> for LibraryError {
    fn from(error: bae_transfers::StorageMoveError) -> Self {
        match error {
            bae_store::WriteError::Database(error) => Self::Database(error),
            error => Self::StorageMove(error),
        }
    }
}

/// The class a surface words a refused dismissal of an undone storage setup
/// in: the sync status offers only the setup that ended undone, so another
/// is bae's own fault.
fn setup_dismiss_category(
    error: &bae_sync_status::SetupDismissError,
) -> bae_diagnostics::UiErrorCategory {
    use bae_sync_status::SetupDismissError as E;
    match error {
        E::Database(error) => bae_diagnostics::database_category(error),
        E::NotUndone(_) => bae_diagnostics::UiErrorCategory::Internal,
    }
}

/// A dismissal that failed in the store is the library's store failure.
impl From<bae_sync_status::SetupDismissError> for LibraryError {
    fn from(error: bae_sync_status::SetupDismissError) -> Self {
        match error {
            bae_sync_status::SetupDismissError::Database(error) => Self::Database(error),
            error => Self::SetupDismiss(error),
        }
    }
}

/// The class a surface words a refused dismissal of an admin change in: the
/// list offers only this device's admin changes, so another is bae's own
/// fault.
fn admin_change_dismiss_category(
    error: &bae_people::admin_change::AdminChangeDismissError,
) -> bae_diagnostics::UiErrorCategory {
    use bae_people::admin_change::AdminChangeDismissError as E;
    match error {
        E::Database(error) => bae_diagnostics::database_category(error),
        E::NotAdminChange(_) => bae_diagnostics::UiErrorCategory::Internal,
    }
}

/// A dismissal that failed in the store is the library's store failure.
impl From<bae_people::admin_change::AdminChangeDismissError> for LibraryError {
    fn from(error: bae_people::admin_change::AdminChangeDismissError) -> Self {
        match error {
            bae_people::admin_change::AdminChangeDismissError::Database(error) => {
                Self::Database(error)
            }
            error => Self::AdminChangeDismiss(error),
        }
    }
}

/// The class a surface words a refused primary release in: a release that
/// moved to another album or left the library after the person picked it
/// is theirs to pick again, from the album as it now stands.
fn primary_release_category(
    error: &bae_records::PrimaryReleaseError,
) -> bae_diagnostics::UiErrorCategory {
    use bae_records::PrimaryReleaseError as E;
    match error {
        E::Database(error) => bae_diagnostics::database_category(error),
        E::NotAlbumRelease { .. } => bae_diagnostics::UiErrorCategory::ChangedMeanwhile,
    }
}

/// Making a release primary that failed in the store is the library's store
/// failure.
impl From<bae_records::PrimaryReleaseError> for LibraryError {
    fn from(error: bae_records::PrimaryReleaseError) -> Self {
        match error {
            bae_records::PrimaryReleaseError::Database(error) => Self::Database(error),
            error => Self::PrimaryRelease(error),
        }
    }
}

/// The class a surface words a refused printed name in: an artist deleted
/// or merged after the person opened it is theirs to find again as the
/// library now holds it.
fn credit_name_category(error: &bae_records::CreditNameError) -> bae_diagnostics::UiErrorCategory {
    use bae_records::CreditNameError as E;
    match error {
        E::Database(error) => bae_diagnostics::database_category(error),
        E::ArtistMissing(_) => bae_diagnostics::UiErrorCategory::ChangedMeanwhile,
    }
}

/// A printed name's write that failed in the store is the library's store
/// failure.
impl From<bae_records::CreditNameError> for LibraryError {
    fn from(error: bae_records::CreditNameError) -> Self {
        match error {
            bae_records::CreditNameError::Database(error) => Self::Database(error),
            error => Self::CreditName(error),
        }
    }
}

/// An edit session's save that found a track it rewrites gone found the
/// release changed since the session opened, which another device's write
/// did after the save checked it.
#[cfg(desktop)]
impl From<bae_records::ReleaseEditSaveError> for LibraryError {
    fn from(error: bae_records::ReleaseEditSaveError) -> Self {
        match error {
            bae_records::ReleaseEditSaveError::TrackGone { release_id, .. } => {
                Self::ReleaseEditStale { release_id }
            }
            bae_records::ReleaseEditSaveError::Write(error) => error.into(),
        }
    }
}

/// A pin's byte counts that make no progress are the store's to explain.
impl From<bae_transfers::DownloadProgressError> for LibraryError {
    fn from(error: bae_transfers::DownloadProgressError) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<TaskFailed> for LibraryError {
    fn from(failed: TaskFailed) -> Self {
        Self::Internal(failed.to_string())
    }
}

impl From<Bug> for LibraryError {
    fn from(bug: Bug) -> Self {
        Self::Internal(bug.to_string())
    }
}

/// Library operations that run import steps (a release editor's catalog
/// apply, a cover download) report the import error itself; a store failure
/// import met is the library's store failure.
#[cfg(desktop)]
impl From<bae_import::ImportError> for LibraryError {
    fn from(error: bae_import::ImportError) -> Self {
        match error {
            bae_import::ImportError::Database(error) => LibraryError::Database(error),
            other => LibraryError::Import(Box::new(other)),
        }
    }
}

/// Tag fields to embed on an exported track file.
#[cfg(desktop)]
pub struct SaveTags {
    pub title: String,
    /// The track's line: its own credits, else the album's (ARTIST).
    pub artist: String,
    /// The album's line (ALBUMARTIST).
    pub album_artist: String,
    pub album: String,
    pub year: Option<i32>,
    /// The track's medium among its release's, for a release of several
    /// mediums. `None` for a release of one, which writes no disc tag.
    pub disc: Option<u32>,
}

/// Tag data resolved for one track — everything a filename template or the
/// tag writer needs, before applying the user's metadata selection. Resolved
/// from the database alone: no audio or cover read, so the filename-suggestion
/// path can build it without touching a whole file or the cloud.
#[cfg(desktop)]
pub struct ResolvedSaveTags {
    pub tags: SaveTags,
    /// The track's number on its side.
    pub track_number: u32,
    pub total_tracks: usize,
}

/// Which image to use when changing an album's cover art.
pub enum CoverSelection {
    /// Use an image file already in the library.
    ReleaseImage { file_id: String },
    /// Download from a remote URL.
    RemoteCover { url: String },
}

/// Persistence and queries for albums, tracks, and files: import state
/// transitions, library browsing, and deletion with cloud-storage cleanup.
#[derive(Clone)]
pub struct LibraryManager {
    store: Store,
    preferences: bae_preferences::PreferencesRow,
    records: bae_records::Records,
    people: bae_people::People,
    membership: bae_people::Membership,
    sync_status: SyncStatus,
    /// The app directory this library is registered under: where renaming
    /// another library finds it, where the active-library pointer is written,
    /// and where forgetting this library removes it from.
    app_dir: bae_app_dir::AppDir,
    clock: ClockRef,
    ids: IdSourceRef,
    /// Typed telemetry sink, injected at bootstrap alongside the clock/id
    /// sources. The sync-failure path emits through it.
    diagnostics: Diagnostics,
    runtime_handle: tokio::runtime::Handle,
    /// The manager's own background tasks (queue workers, the sync status,
    /// pending subscriptions, startup sync), which each hold a clone of it and
    /// so of the store. [`LibraryManager::close`] ends them all.
    tasks: service::BackgroundTasks,
    /// The "Pin for offline" queue: its table, which the Downloads pane and
    /// the detail views read, and the bytes its active download has moved. A
    /// single serial worker drains it one release at a time. Shared across
    /// manager clones; kept, with its pause, as the library opens (a release
    /// that wasn't fully pinned stays as it was and its entry runs again).
    downloads: bae_transfers::Downloads,
    /// The "Export…" and save queue: its table, which the inspector reads, and
    /// the percent its active output has reached. A single serial worker
    /// drains it one release at a time. Shared across manager clones; emptied
    /// as the library opens. An output changes no release state — it only
    /// reads, and writes to a user directory. Desktop-only: see the `output`
    /// module.
    #[cfg(desktop)]
    outputs: bae_transfers::Outputs,
    /// Session cache of provider image responses, keyed by URL. Candidate
    /// preparation and library images own their bytes independently.
    #[cfg(desktop)]
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
    /// Where import reads a catalog release, which import's services hold
    /// too.
    #[cfg(desktop)]
    release_reads: bae_import::ReleaseReads,
    /// Held by every edit session's save and every field edit of a release,
    /// so a save compares the release with what its session opened on and
    /// writes with no other of them on this device between the two.
    #[cfg(desktop)]
    release_writes: Arc<tokio::sync::Mutex<()>>,
    /// Where a test holds each run of a pin's query of which of its
    /// release's files are kept.
    #[cfg(test)]
    pin_reads_hold: Arc<pin::Hold>,
}

pub fn generate_mcp_token() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

impl std::fmt::Debug for LibraryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryManager").field("store", &self.store).finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
