//! Orchestrator layer: the one place where raw DB aggregates meet util
//! formatters and filesystem paths to produce resolved types.
//!
//! `LibraryManager` owns the database handle. Its surface is split into
//! per-entity modules — `release`, `album`, `track`, `artist`, `identity`,
//! `image`, `import`, `export`, `storage` — each holding that entity's I/O
//! operations, reading `&Database` / `&CovenHandle` for the covers, pin state,
//! and joins a resolved shape needs. The pure projection from a `Db*` aggregate
//! to its resolved counterpart lives on the produced type, as a `from_raw`
//! constructor in `crate::album_detail`. Public methods return resolved
//! shapes, never the raw `Db*` aggregates.
//!
//! Rule for additions: a view's reads, and the shapes a screen shows them in,
//! are `crate::library::views::LibraryViews`'; every other resolved shape is
//! made in this layer. A new resolved shape means a raw type in
//! `crate::db::models` (or `crate::library::views` for what a view reads), a
//! resolved type in `crate::album_detail` (or a sibling like `crate::queue`)
//! with its `from_raw` constructor, and the I/O method in the matching entity
//! module here (or in the view's module).

use std::future::Future;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use thiserror::Error;
use tracing::{debug, warn};

use crate::album_detail::GallerySource;
use crate::db::{
    Database, DbAlbum, DbArtist, DbFile, DbLibraryImage, DbRelease, DbTrack, ImageRef,
    LibraryImageType,
};
use crate::diagnostics::{Diagnostics, SyncOperation, TelemetryEvent};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::library::save::SaveService;
use crate::library::sync_controller::SyncController;
use crate::sync::S3ConfigData;
use bae_types::release_storage::ReleaseStorageAction;
use bae_util::task_failure::{blocking, caught, joined, TaskFailed};
use coven::ClockRef;
use coven::IdSourceRef;

mod service;

mod album;
mod artist;
mod cast_devices;
mod collections;
mod config;
mod coven_files;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod discogs;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
mod fixture;
#[cfg(all(
    any(test, feature = "test-utils", debug_assertions),
    not(any(target_os = "ios", target_os = "android"))
))]
pub use fixture::{
    FixtureAlbum, FixtureCandidate, FixtureCandidateState, FixturePlaying, FixtureWatchedFolder,
    LibraryFixture, LibraryFixtureError, FIXTURE_IMPORT_FAILURE, FIXTURE_LOOKUP_FAILURE,
};
/// Desktop-only, under the same predicate as the rest of the export surface (the
/// queue field below, and `library::export`). Exporting writes a directory tree
/// next to the user's chosen folder — a hidden staging sibling, a marker file, a
/// rename into place — which a sandboxed iOS/Android document URL does not offer,
/// and the preset half of it needs the desktop-only `library::export` encoder.
/// Compiling the producers out with the worker is what makes a mobile export call
/// a compile error rather than a release that sits `Queued` with nothing to run it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod export;
mod image;
mod lifecycle;
mod locality;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod output;
mod people;
mod pin;
mod playback_state;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod records;
mod release;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod release_edit;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use release_edit::ReleaseEdit;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod save;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use save::SaveWindow;
#[cfg(test)]
mod hold;
mod pending_item;
mod storage;
mod sync;
mod sync_status;
use crate::diagnostics::{
    cloud_home_category, cloud_setup_category, cloud_unlock_category, device_only_category,
    file_read_category, key_category, sync_category,
};
mod track;

pub use pending_item::{PendingItem, PendingSubjectName, PendingWait};
pub(crate) use sync_status::sync_status_on;
pub use sync_status::{pending_action, PendingAction, SyncRetryOutcome};

/// Outcome of `resolve_records_target_album` — where a release should
/// land after a `set_records` call. `new_album` carries the album row
/// to insert when the target is brand-new; otherwise the target is an
/// existing album and `new_album` is `None`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
struct RecordsTargetAlbum {
    album_id: String,
    new_album: Option<DbAlbum>,
}

#[derive(Error, Debug)]
pub enum LibraryError {
    #[error("Database error: {0}")]
    Database(#[from] crate::db::DbError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// An import step the library ran failed — reading a catalog release
    /// against a release's tracks, fetching a cover, a candidate's paths —
    /// as the import error itself, so every surface words the refusal it is
    /// (another number of tracks, discs that fit alike) rather than a
    /// generic line.
    #[error(transparent)]
    Import(Box<crate::import::ImportError>),
    /// An operation on a release, an album, an artist or an import
    /// candidate could not go ahead as asked — what it names is not in the
    /// library, the candidate moved under it, an input is blank — with why
    /// as text. Shown as the generic import line.
    #[error("{0}")]
    Refused(String),
    #[error("Import error: {0}")]
    ArtistIdentityConflict(#[source] Box<crate::db::ArtistIdentityConflict>),
    #[error("Import error: {0}")]
    LabelIdentityConflict(#[source] Box<crate::db::LabelIdentityConflict>),
    /// A verbatim release export (reproducing the imported file set) failed.
    #[error("Export error: {0}")]
    Export(String),
    /// A save (rendered output — track or release) failed.
    #[error("Save error: {0}")]
    Save(String),
    /// A user-submitted release metadata edit failed its invariants (blank album
    /// title, no album artist). The editor's typed error, so every surface reports
    /// the same rule rather than its own hand-written sentence.
    #[error(transparent)]
    Edit(#[from] crate::import::EditValidationError),
    #[error("Release editor cannot load: {0}")]
    ReleaseEditLoad(#[from] crate::import::ReleaseEditLoadError),
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
    /// The OS keyring couldn't be read or written (encryption key, cloud-home
    /// credentials, Discogs/MCP tokens).
    #[error("Keyring error: {0}")]
    Keyring(#[from] coven::KeyError),
    /// Reading or writing bae's directory failed: the library list, the
    /// active-library pointer, a removal record.
    #[error("Config error: {0}")]
    AppDir(#[from] crate::app_dir::AppDirError),
    /// Reading or writing this device's preferences failed, or a change to
    /// them was refused.
    #[error("Config error: {0}")]
    Preferences(#[from] crate::preferences::PreferencesError),
    /// Building or probing a cloud home failed — bad credentials/config
    /// (`Configuration`) or an unreachable backend (`Transport`).
    #[error("Cloud home error: {0}")]
    CloudHome(#[from] coven::StorageError),
    #[error("Cloud setup error: {0}")]
    CloudSetup(#[from] coven::StorageSetupError),
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

impl From<crate::db::TrackAudioError> for LibraryError {
    fn from(error: crate::db::TrackAudioError) -> Self {
        match error {
            crate::db::TrackAudioError::Database(error) => Self::Database(error),
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

impl From<crate::db::IdentityWriteError> for LibraryError {
    fn from(error: crate::db::IdentityWriteError) -> Self {
        match error {
            crate::db::IdentityWriteError::ArtistIdentityConflict(conflict) => {
                Self::ArtistIdentityConflict(conflict)
            }
            crate::db::IdentityWriteError::LabelIdentityConflict(conflict) => {
                Self::LabelIdentityConflict(conflict)
            }
            crate::db::IdentityWriteError::Db(error) => Self::Database(error),
            crate::db::IdentityWriteError::Unresolvable(detail) => Self::Refused(detail),
        }
    }
}

impl From<crate::db::ArtistIdentityConflict> for LibraryError {
    fn from(error: crate::db::ArtistIdentityConflict) -> Self {
        Self::ArtistIdentityConflict(Box::new(error))
    }
}

#[derive(Debug)]
pub struct UploadReceipt {
    pub release_ids: Vec<String>,
}

#[derive(Debug)]
pub struct UploadBatchFailure {
    pub release_ids: Vec<String>,
    pub error: crate::diagnostics::UiError,
}

#[derive(Debug)]
pub enum UploadReleasesOutcome {
    Complete { receipt: UploadReceipt },
    Partial { receipt: Option<UploadReceipt>, failure: UploadBatchFailure },
}

/// A stored name that names no file refuses the write it was for: the row it
/// came from is unusable, and the copy does not run.
impl From<crate::storage::stored_name::StoredNameError> for LibraryError {
    fn from(error: crate::storage::stored_name::StoredNameError) -> Self {
        LibraryError::Refused(error.to_string())
    }
}

impl LibraryError {
    /// The user-facing diagnostic class the bridge renders. Membership/setup
    /// failures carry the distinctions the cloud-setup and sharing flows show as
    /// different messages (bad credentials vs unreachable backend vs keyring vs
    /// membership); everything else maps to its general class.
    pub fn category(&self) -> crate::diagnostics::UiErrorCategory {
        use crate::diagnostics::UiErrorCategory as C;
        match self {
            LibraryError::Database(_) => C::Database,
            LibraryError::AppDir(error) => error.category(),
            LibraryError::Preferences(error) => error.category(),
            LibraryError::Validation(_) => C::Config,
            LibraryError::Keyring(error) => key_category(error),
            LibraryError::CloudHome(e) => cloud_home_category(e),
            LibraryError::CloudSetup(error) => cloud_setup_category(error),
            LibraryError::CloudUnlock(error) => cloud_unlock_category(error),
            LibraryError::Sync(e) => sync_category(e),
            LibraryError::InviteJoining(_) => C::InviteJoining,
            LibraryError::RetryBlockedOperation(_) => C::Membership,
            LibraryError::Import(error) => error.category(),
            LibraryError::Refused(_) | LibraryError::Edit(_) => C::Import,
            LibraryError::ReleaseEditStale { .. } => C::ReleaseChangedSinceOpened,
            LibraryError::ArtistIdentityConflict(_) | LibraryError::LabelIdentityConflict(_) => {
                C::Import
            }
            LibraryError::Export(_) => C::Export,
            LibraryError::Save(_) => C::Save,
            LibraryError::FileRead { error, .. } => file_read_category(error),
            LibraryError::MakeLocal(error) => device_only_category(error),
            LibraryError::Cleanup { operation, .. } => operation.category(),
            LibraryError::Io(_)
            | LibraryError::TrackMapping(_)
            | LibraryError::ReleaseEditLoad(_)
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl From<crate::import::CatalogSessionError> for LibraryError {
    fn from(error: crate::import::CatalogSessionError) -> Self {
        match error {
            crate::import::CatalogSessionError::Preferences(error) => Self::Database(error),
            crate::import::CatalogSessionError::Keyring(error) => Self::Keyring(error),
        }
    }
}

impl From<TaskFailed> for LibraryError {
    fn from(failed: TaskFailed) -> Self {
        Self::Internal(failed.to_string())
    }
}

/// Library operations that run import steps (a release editor's catalog
/// apply, a cover download) report the import error itself; a store failure
/// import met is the library's store failure.
impl From<crate::import::ImportError> for LibraryError {
    fn from(error: crate::import::ImportError) -> Self {
        match error {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            crate::import::ImportError::Database(error) => LibraryError::Database(error),
            other => LibraryError::Import(Box::new(other)),
        }
    }
}

/// Sync activity and the things coven is waiting on that need attention,
/// read from coven's tables and bae's own in one go.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncStatusSnapshot {
    pub status: coven::SyncStatus,
    pub pending: Vec<PendingItem>,
    /// Why this device's last attempt to start sync failed, while that
    /// stands.
    pub error: Option<crate::diagnostics::UiError>,
    /// What the person's retry would do now, as coven decides it: run a
    /// pass, or be answered at once without one.
    pub retry_effect: coven::RetryEffect,
}

/// The settings status line. Platforms localize these cases and the item count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncIndicator {
    /// Storage is set up, but the sync loop isn't running.
    Stopped,
    Syncing,
    Synced,
    Offline,
    /// The failure is the snapshot's `shown_failure()`.
    Failed,
    Waiting {
        items: usize,
    },
}

/// Tag fields to embed on an exported track file.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

/// Verb for a storage transition, used to name the operation in the "ended
/// without completion" guard.
fn verb(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::Pin => "Pin",
        ReleaseStorageAction::Unpin => "Unpin",
        ReleaseStorageAction::Upload => "Upload",
        ReleaseStorageAction::DetachAudio => "Detach Audio",
    }
}

/// Persistence and queries for albums, tracks, and files: import state
/// transitions, library browsing, and deletion with cloud-storage cleanup.
#[derive(Clone)]
pub struct LibraryManager {
    database: Database,
    /// The app directory this library is registered under: where renaming
    /// another library finds it, where the active-library pointer is written,
    /// and where forgetting this library removes it from.
    app_dir: crate::app_dir::AppDir,
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
    /// The readers of every deletion of tracks, each hearing all of them in
    /// order, as the ids deleted: the player drops them from its queue.
    track_deletions: Arc<std::sync::Mutex<Vec<tokio::sync::mpsc::UnboundedSender<Vec<String>>>>>,
    /// The cloud-sync responsibility: the provider connection and the
    /// upload pause.
    sync: SyncController,
    /// How to stop each transfer making a release local that this process
    /// runs.
    detach_cancels: crate::library::detach_cancels::DetachCancels,
    /// How far each release this process is making local has come.
    make_local_transfers: crate::library::make_local_transfers::MakeLocalTransfers,
    /// The "Pin for offline" queue: its table, which the Downloads pane and
    /// the detail views read, and the bytes its active download has moved. A
    /// single serial worker drains it one release at a time. Shared across
    /// manager clones; emptied as the library opens (a release that wasn't
    /// fully pinned stays cloud-only).
    downloads: crate::library::Downloads,
    /// The "Export…" and save queue: its table, which the inspector reads, and
    /// the percent its active output has reached. A single serial worker
    /// drains it one release at a time. Shared across manager clones; emptied
    /// as the library opens. An output changes no release state — it only
    /// reads, and writes to a user directory. Desktop-only: see the `output`
    /// module.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    outputs: crate::library::Outputs,
    /// Session cache of provider image responses, keyed by URL. Candidate
    /// preparation and library images own their bytes independently.
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
    /// The catalogs as this library may ask them, which import's services
    /// hold too.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    catalogs: crate::import::CatalogSessions,
    /// Held by every edit session's save and every field edit of a release,
    /// so a save compares the release with what its session opened on and
    /// writes with no other of them on this device between the two.
    release_writes: Arc<tokio::sync::Mutex<()>>,
    /// Where a test holds each run of a pin's query of its release's pins.
    #[cfg(test)]
    pin_reads_hold: Arc<hold::Hold>,
}

pub fn generate_mcp_token() -> String {
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

impl std::fmt::Debug for LibraryManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibraryManager").field("database", &self.database).finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
