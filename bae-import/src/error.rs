//! The typed error the import pipeline threads through every fallible step.
//!
//! Wire-level source errors (`MusicBrainzError`, `DiscogsError`) are kept
//! structurally rather than flattened to a string, so a consumer that needs to
//! tell a MusicBrainz timeout from a malformed payload can read it off the
//! chain. `do_import`, the terminal consumer, is the only place that turns this
//! back into a string, for the user-facing `ImportProgress::Failed { error }`.

/// Why an import failed. One class per distinguishable failure the pipeline
/// actually produces.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Walking the candidate folder failed (I/O during the tree walk).
    #[cfg(desktop)]
    #[error("folder scan failed: {0}")]
    Scan(#[from] crate::folder_scanner::FolderScanError),

    /// The folder is structurally unimportable (damaged audio, no valid
    /// audio). Typed reason reused from the scanner.
    #[cfg(desktop)]
    #[error("folder cannot be imported: {0}")]
    InvalidFolder(#[from] crate::folder_scanner::InvalidReason),

    /// A track sheet is bound to audio that holds nothing bae can read.
    #[cfg(desktop)]
    #[error("folder cannot be imported: {0}")]
    UnreadableAudio(#[from] crate::folder_scanner::UnreadableAudio),

    /// A catalog could not be asked, or what it answered could not be read.
    #[error(transparent)]
    Catalog(#[from] bae_catalogs::CatalogError),

    /// The release lists another number of tracks than the folder holds, so
    /// it cannot describe the folder's audio without adding or losing tracks.
    #[cfg(desktop)]
    #[error("{folder_tracks} tracks in the folder, {release_tracks} on the release")]
    MetadataTrackCount { folder_tracks: u32, release_tracks: u32 },

    /// Several sets of the release's mediums fit the folder's audio equally —
    /// by Disc ID, carrier and lengths — and read as different tracks, so
    /// which of them the folder is a rip of is not known. `mediums` names
    /// every medium of those sets as the release numbers them, ascending.
    #[cfg(desktop)]
    #[error("discs {mediums:?} of the release fit the folder's audio equally")]
    TiedMediums { mediums: Vec<u32> },

    /// Local file-tag evidence can't seed a file-metadata import (no audio files,
    /// a file failed to open / parse, embedded-cover read failure).
    #[cfg(desktop)]
    #[error("file tags cannot be read: {detail}")]
    FileTags { detail: String },

    /// A file the import must read cannot be used: audio that will not decode,
    /// a codec bae can't play, bytes that could not be hashed, or audio a track
    /// named that is no longer in the folder.
    #[cfg(desktop)]
    #[error("{detail}")]
    UnusableFile { detail: String },

    /// A document of the folder the person opened to read could not be read:
    /// it left the folder, or the disk refused it.
    #[cfg(desktop)]
    #[error("{file} could not be read: {error}")]
    DocumentRead { file: String, error: std::io::Error },

    /// The downloaded or selected cover cannot be parsed, decoded, or resized.
    #[error("cover art failed: {detail}")]
    CoverArt { detail: String },

    /// The selected local cover disappeared or could not be read.
    #[cfg(desktop)]
    #[error("cover art failed: {detail}")]
    LocalCover { detail: String },

    /// The import's decode found tracks that would import but not play.
    #[cfg(desktop)]
    #[error("decode verification failed for {} track(s): {}", broken.len(), broken.join("; "))]
    DecodeVerification { broken: Vec<String> },

    /// A source file could not be read while the import decoded it (loudness
    /// and decode verification). Carries the read's own error — a missing
    /// file, a full disk, a dropped network volume — and says nothing about
    /// the audio, unlike `DecodeVerification`: importing again once the source
    /// is readable is the remedy.
    #[cfg(desktop)]
    #[error("the source audio for {track} could not be read: {error}")]
    SourceRead { track: String, error: bae_audio::sparse_buffer::SourceReadError },

    /// Candidate preparation is immutable from the moment import owns it.
    #[cfg(desktop)]
    #[error("This release is being imported and can no longer be edited")]
    CandidateImportInProgress,

    /// A bulk import reached a candidate identification is still answering: a
    /// run is queued for it, running, or writing what it found. The import
    /// would cancel the run and commit what the run is about to replace, so
    /// the candidate is left for the person to import once it is settled.
    #[cfg(desktop)]
    #[error("This release is still being identified; import it once identification finishes")]
    CandidateBeingIdentified,

    /// This device already imported a release from these files: one folder
    /// hash is one release on a device (`local_release_origin`). Refused when
    /// the import is asked for, and by the commit when another import of the
    /// same files landed first.
    #[cfg(desktop)]
    #[error("this device already imported these files, as release {release_id}")]
    FilesAlreadyImported { release_id: String },

    /// A cancel reached an import that is already writing its release, which
    /// is one transaction and completes.
    #[cfg(desktop)]
    #[error("This release is already being written to the library and can no longer be cancelled")]
    ImportWriting,

    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made: one of its folders changed or is gone, or the files
    /// of the folder they sit in go with another release or are still
    /// downloading.
    #[cfg(desktop)]
    #[error("{reason}")]
    GroupingBlocked { reason: crate::grouping::GroupingBlock },

    /// A save of a candidate's whole state was refused.
    #[cfg(desktop)]
    #[error(transparent)]
    Preparation(#[from] crate::tables::PreparationRefusal),

    /// A change to the watched folders, or a write about a folder under one,
    /// was refused.
    #[cfg(desktop)]
    #[error(transparent)]
    WatchedFolders(#[from] crate::tables::WatchedFolderRefusal),

    /// Storing a folder's reading was refused.
    #[cfg(desktop)]
    #[error(transparent)]
    FolderReading(#[from] crate::tables::FolderReadingRefusal),

    /// Reading one folder of a watched folder again was refused.
    #[cfg(desktop)]
    #[error(transparent)]
    FolderReread(#[from] crate::tables::FolderRereadRefusal),

    /// A list opened for one read was found closed under it, though only
    /// the read that opened it closes it.
    #[cfg(desktop)]
    #[error("import list {0:?} closed under the read that opened it")]
    ListClosed(crate::ImportListId),

    /// A release of folders picked together could not be read apart again.
    #[cfg(desktop)]
    #[error(transparent)]
    Separation(#[from] crate::tables::SeparationRefusal),

    /// A write about a candidate's state, or the read its run starts from,
    /// was refused.
    #[cfg(desktop)]
    #[error(transparent)]
    CandidateState(#[from] crate::tables::CandidateStateRefusal),

    /// Releases picked to be read as one could not be combined.
    #[cfg(desktop)]
    #[error(transparent)]
    Combine(#[from] crate::grouping::CombineRefusal),

    /// The cover chosen names a file or tag artwork the candidate's scan did
    /// not store.
    #[cfg(desktop)]
    #[error(transparent)]
    Cover(#[from] crate::CoverRefusal),

    /// The draft or edit does not shape into a savable one (the editor's
    /// typed error).
    #[cfg(desktop)]
    #[error(transparent)]
    Edit(#[from] crate::EditValidationError),

    /// A change to a candidate could not go ahead as asked: what it names
    /// moved under it, or is not there, with why as text.
    #[cfg(desktop)]
    #[error("{0}")]
    Refused(String),

    /// The release's credits or labels cannot be written as the library
    /// stands, for the reason the text gives: a picked artist no longer
    /// exists, or the write's own statements contradict each other.
    #[cfg(desktop)]
    #[error("{detail}")]
    Unresolvable { detail: String },

    /// The library's artists changed between preparing the import's write
    /// and committing it, in a way that makes what was prepared wrong.
    /// Nothing was written; importing again prepares against the library as
    /// it now stands.
    #[cfg(desktop)]
    #[error("the library's artists changed while this import was being written; import it again")]
    LibraryChanged,

    /// The candidate changed, or left, between preparing the import and
    /// committing it. Nothing was written; importing again prepares from the
    /// candidate as it now stands.
    #[cfg(desktop)]
    #[error(transparent)]
    CandidateMoved(crate::service::CandidateMoved),

    /// A candidate's preparation contradicts itself: its draft's author
    /// could not have read it from where it says. No writer here builds one,
    /// so the save that met it wrote nothing.
    #[cfg(desktop)]
    #[error("candidate {content_hash}: {contradiction}")]
    Provenance {
        content_hash: String,
        #[source]
        contradiction: crate::ProvenanceContradiction,
    },

    /// A sheet-binding change could not be applied: the candidate, the sheet,
    /// or the audio named is not what it was when the picker offered it. Every
    /// offerable set that crosses to a UI is already filtered to what the sheet
    /// can use, so this is a folder that changed under the choice rather than a
    /// UI that offered something it should not have.
    #[cfg(desktop)]
    #[error("that audio can no longer back this sheet: {detail}")]
    SheetBinding { detail: String },

    /// A watched root or a candidate path under one is spelled in a way the
    /// store refuses to key by: relative, climbing out of itself, or not the
    /// one canonical spelling.
    #[cfg(desktop)]
    #[error(transparent)]
    WatchedFolder(crate::WatchedPathError),

    /// A stored file or folder name of a release that names no file: the row
    /// an import's write was for is unusable.
    #[cfg(desktop)]
    #[error(transparent)]
    StoredName(bae_records::stored_name::StoredNameError),

    /// A folder's OS-level filesystem watch could not be installed or
    /// removed (missing path, permissions). An OS/user condition, not a
    /// broken invariant.
    #[cfg(desktop)]
    #[error("folder watch failed: {detail}")]
    Watch { detail: String },

    /// The library's store failed a read or a write import asked of it.
    #[cfg(desktop)]
    #[error(transparent)]
    Database(#[from] bae_store::DbError),

    /// An import's write failed, and rolling it back failed too.
    #[cfg(desktop)]
    #[error("{operation}; rollback failed: {rollback}")]
    Rollback {
        operation: Box<ImportError>,
        #[source]
        rollback: rusqlite::Error,
    },

    /// An import's write failed, and removing the bytes it staged failed
    /// too, each failure kept.
    #[cfg(desktop)]
    #[error("file cleanup failed: {failures:?}; write: {write}")]
    FileCleanup { write: Box<ImportError>, failures: Vec<coven::DbError> },

    /// Where the library syncs to could not be read, which decides where an
    /// import's files go.
    #[cfg(desktop)]
    #[error(transparent)]
    Sync(#[from] coven::SyncError),

    /// The session a catalog is asked with could not be had: the library's
    /// preferences or its keyring could not be read.
    #[cfg(desktop)]
    #[error(transparent)]
    CatalogSession(#[from] crate::CatalogSessionError),

    /// The Discogs key, or what Discogs said of it, could not be written.
    #[cfg(desktop)]
    #[error(transparent)]
    DiscogsKey(Box<crate::DiscogsKeyError>),

    /// The import service stopped, as it does when the library closes,
    /// before the import could be queued: nothing is wrong with the
    /// candidate, and nothing was claimed for it.
    #[cfg(desktop)]
    #[error("the import service has stopped")]
    Stopped,

    /// A worker of the import service — the import worker or the folder scan
    /// coordinator — ended on this bug, reported as it happened, before the
    /// command could reach it: the service ended with it. Nothing was
    /// claimed for the candidate.
    #[cfg(desktop)]
    #[error("the import service ended on a bug")]
    WorkerFailed(#[source] coven::task::Bug),

    /// A broken invariant, not a user condition: artist-id remap miss,
    /// non-UTF-8 path, work that panicked apart from its caller, missing
    /// library-status row.
    #[error("internal import error: {detail}")]
    Internal { detail: String },
}

/// A catalog client's or the image cache's failure is a catalog's failure.
#[cfg(desktop)]
impl From<bae_catalogs::musicbrainz::MusicBrainzError> for ImportError {
    fn from(error: bae_catalogs::musicbrainz::MusicBrainzError) -> Self {
        Self::Catalog(error.into())
    }
}

#[cfg(desktop)]
impl From<bae_catalogs::wikidata::WikidataError> for ImportError {
    fn from(error: bae_catalogs::wikidata::WikidataError) -> Self {
        Self::Catalog(error.into())
    }
}

#[cfg(desktop)]
impl From<bae_catalogs::discogs::client::DiscogsError> for ImportError {
    fn from(error: bae_catalogs::discogs::client::DiscogsError) -> Self {
        Self::Catalog(error.into())
    }
}

impl From<bae_catalogs::remote_image::RemoteImageError> for ImportError {
    fn from(error: bae_catalogs::remote_image::RemoteImageError) -> Self {
        Self::Catalog(error.into())
    }
}

#[cfg(desktop)]
impl From<crate::DiscogsKeyError> for ImportError {
    fn from(error: crate::DiscogsKeyError) -> Self {
        Self::DiscogsKey(Box::new(error))
    }
}

/// Work run apart from its caller gave no result: it panicked (a bug the
/// caller has reported), or its runtime is shutting down under it.
#[cfg(desktop)]
impl From<bae_util::task_failure::TaskFailed> for ImportError {
    fn from(failed: bae_util::task_failure::TaskFailed) -> Self {
        Self::Internal { detail: failed.to_string() }
    }
}

#[cfg(desktop)]
impl ImportError {
    /// The failure a catalog answered with, which is a lookup that failed, or
    /// `None` for an error bae met on its own side — a request it could not
    /// build, an answer it could not parse, a store read.
    pub(crate) fn provider_failure(&self) -> Option<bae_catalogs::LookupFailure> {
        match self {
            Self::Catalog(error) => bae_catalogs::search::provider_failure(error),
            _ => None,
        }
    }

    /// Why a step that asked a catalog did not answer: what a provider
    /// answered with, or what bae met on its own side, logged as it is named.
    pub(crate) fn failure(&self, what: &str) -> bae_catalogs::Failure {
        bae_catalogs::search::lookup_or_internal(self.provider_failure(), what, self)
    }
}

/// An import's write to the library's records failed: credits or labels
/// that cannot be written, or the store.
#[cfg(desktop)]
impl From<bae_records::IdentityWriteError> for ImportError {
    fn from(error: bae_records::IdentityWriteError) -> Self {
        use bae_records::IdentityWriteError as E;
        match error {
            E::Unresolvable(detail) => Self::Unresolvable { detail },
            E::StoredName(error) => Self::StoredName(error),
            E::Db(error) => Self::Database(error),
            E::Rollback { operation, rollback } => {
                Self::Rollback { operation: Box::new((*operation).into()), rollback }
            }
            E::FileCleanup { write, failures } => {
                Self::FileCleanup { write: Box::new((*write).into()), failures }
            }
        }
    }
}

/// An import's commit failed: its write failed, the library's artists changed
/// under it, or another import of the same files landed first.
#[cfg(desktop)]
impl From<crate::tables::ImportCommitError> for ImportError {
    fn from(error: crate::tables::ImportCommitError) -> Self {
        use crate::tables::ImportCommitError as E;
        match error {
            E::Write(error) => error.into(),
            E::LibraryChanged => Self::LibraryChanged,
            E::FilesAlreadyImported { release_id } => Self::FilesAlreadyImported { release_id },
            E::CandidateMoved(moved) => Self::CandidateMoved(moved),
            E::Rollback { operation, rollback } => {
                Self::Rollback { operation: Box::new((*operation).into()), rollback }
            }
            E::FileCleanup { write, failures } => {
                Self::FileCleanup { write: Box::new((*write).into()), failures }
            }
        }
    }
}

/// A bug a coven call import made carried back, which the store reported as
/// it returned, one coven repeats, or the bug an import worker ended on,
/// which its thread reported as it happened.
impl bae_util::task_failure::ReportedBug for ImportError {
    fn reported_bug(&self) -> Option<&coven::task::Bug> {
        match self {
            #[cfg(desktop)]
            Self::Database(error)
            | Self::CatalogSession(crate::CatalogSessionError::Preferences(error)) => {
                error.reported_bug()
            }
            #[cfg(desktop)]
            Self::Sync(error) => error.reported_bug(),
            #[cfg(desktop)]
            Self::WorkerFailed(bug) => Some(bug),
            #[cfg(desktop)]
            Self::Rollback { operation, .. } => operation.reported_bug(),
            #[cfg(desktop)]
            Self::FileCleanup { write, failures } => write.reported_bug().or_else(|| {
                failures.iter().find_map(bae_util::task_failure::ReportedBug::reported_bug)
            }),
            _ => None,
        }
    }
}

/// A worker of the service that ended before a command reached it: one
/// that returned was stopped as the library closed, and one that panicked
/// ended the service on its bug.
#[cfg(desktop)]
impl From<bae_util::worker_thread::WorkerEnd> for ImportError {
    fn from(end: bae_util::worker_thread::WorkerEnd) -> Self {
        match end {
            bae_util::worker_thread::WorkerEnd::Returned => Self::Stopped,
            bae_util::worker_thread::WorkerEnd::Failed(never) => match never {},
            bae_util::worker_thread::WorkerEnd::Panicked(bug) => Self::WorkerFailed(bug),
        }
    }
}

/// A list found closed under the read that opened it is bae's own fault; its
/// store failure is the library's store failure.
#[cfg(desktop)]
impl From<crate::tables::ImportListError> for ImportError {
    fn from(error: crate::tables::ImportListError) -> Self {
        match error {
            crate::tables::ImportListError::Closed(list) => Self::ListClosed(list),
            crate::tables::ImportListError::Database(error) => Self::Database(error),
        }
    }
}

/// A store failure an import's read or write met is the library's store
/// failure.
#[cfg(desktop)]
impl From<coven::DbError> for ImportError {
    fn from(error: coven::DbError) -> Self {
        Self::Database(error.into())
    }
}

#[cfg(desktop)]
impl From<rusqlite::Error> for ImportError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error.into())
    }
}

/// A bug a coven call import made carries back: the store's, through the
/// failure that holds it.
#[cfg(desktop)]
impl coven::task::CarriesBug for ImportError {
    fn bug(&self) -> Option<&coven::task::Bug> {
        use coven::task::CarriesBug;
        match self {
            Self::Database(error)
            | Self::CatalogSession(crate::CatalogSessionError::Preferences(error)) => error.bug(),
            Self::Sync(error) => error.bug(),
            Self::Rollback { operation, .. } => operation.bug(),
            Self::FileCleanup { write, failures } => {
                write.bug().or_else(|| failures.iter().find_map(CarriesBug::bug))
            }
            _ => None,
        }
    }
}

/// A failed rollback or byte removal is kept beside the failure it followed:
/// a store failure keeps it in its own, any other failure stays what it is.
#[cfg(desktop)]
impl coven::WriteFailure for ImportError {
    fn with_rollback(self, rollback: rusqlite::Error) -> Self {
        match self {
            Self::Database(error) => {
                Self::Database(coven::WriteFailure::with_rollback(error, rollback))
            }
            operation => Self::Rollback { operation: Box::new(operation), rollback },
        }
    }

    fn with_cleanup(write: Self, failures: Vec<coven::DbError>) -> Self {
        match write {
            Self::Database(error) => {
                Self::Database(coven::WriteFailure::with_cleanup(error, failures))
            }
            write => Self::FileCleanup { write: Box::new(write), failures },
        }
    }
}

impl ImportError {
    /// This failure as a surface states it: its class, which the surface
    /// words in the person's language, and its own text as the untranslated
    /// detail.
    pub fn ui_error(&self) -> bae_diagnostics::UiError {
        bae_diagnostics::UiError::diagnostic(self.category(), self)
    }

    /// The class a surface words this failure in. Expected refusals and
    /// provider failures are kept apart from unusable source data and
    /// internal faults, and every variant chooses its class, so a new failure
    /// has to say how it is shown.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            #[cfg(desktop)]
            Self::GroupingBlocked { reason } => reason.category(),
            #[cfg(desktop)]
            Self::Refused(_)
            | Self::Unresolvable { .. }
            | Self::LibraryChanged
            | Self::CandidateMoved(_) => C::Import,
            #[cfg(desktop)]
            Self::Database(error) => bae_diagnostics::database_category(error),
            #[cfg(desktop)]
            Self::StoredName(_) => C::Database,
            #[cfg(desktop)]
            Self::CatalogSession(crate::CatalogSessionError::Preferences(error)) => {
                bae_diagnostics::database_category(error)
            }
            #[cfg(desktop)]
            Self::CatalogSession(crate::CatalogSessionError::Keyring(error)) => {
                bae_diagnostics::key_category(error)
            }
            #[cfg(desktop)]
            Self::Sync(error) => bae_diagnostics::sync_category(error),
            // What failed first is what the person can act on; the failed
            // rollback or byte removal rides in the detail.
            #[cfg(desktop)]
            Self::Rollback { operation: failed, .. } | Self::FileCleanup { write: failed, .. } => {
                failed.category()
            }
            #[cfg(desktop)]
            Self::ImportWriting => C::Import,
            #[cfg(desktop)]
            Self::CandidateImportInProgress => C::CandidateImportInProgress,
            #[cfg(desktop)]
            Self::CandidateBeingIdentified => C::CandidateBeingIdentified,
            #[cfg(desktop)]
            Self::FilesAlreadyImported { .. } => C::FilesAlreadyImported,
            #[cfg(desktop)]
            Self::MetadataTrackCount { folder_tracks, release_tracks } => C::MetadataTrackCount {
                folder_tracks: *folder_tracks,
                release_tracks: *release_tracks,
            },
            #[cfg(desktop)]
            Self::TiedMediums { mediums } => C::TiedMediums { mediums: mediums.clone() },
            Self::CoverArt { .. } => C::ImportData,
            Self::Catalog(error) => catalog_category(error),
            Self::Internal { .. } => C::Internal,
            #[cfg(desktop)]
            Self::Stopped | Self::WorkerFailed(_) | Self::Provenance { .. } => C::Internal,
            #[cfg(desktop)]
            Self::ListClosed(_) => C::Internal,
            // A candidate that left, was imported, or changed under the
            // save is the person's to work on as it now stands; a save
            // handed what no caller builds is bae's own fault.
            #[cfg(desktop)]
            Self::Preparation(refusal) => match refusal {
                crate::tables::PreparationRefusal::NoState(_)
                | crate::tables::PreparationRefusal::Imported(_)
                | crate::tables::PreparationRefusal::TagsChanged(_)
                | crate::tables::PreparationRefusal::Changed(_) => C::Import,
                crate::tables::PreparationRefusal::DuplicateScanKeys
                | crate::tables::PreparationRefusal::DraftNotOverAudio(_)
                | crate::tables::PreparationRefusal::SnapshotOfUnscanned => C::Internal,
            },
            // The watched folders changed under the request, or the person
            // asked for a folder that overlaps one; a removal naming no
            // folder is bae's own fault.
            #[cfg(desktop)]
            Self::WatchedFolders(refusal) => match refusal {
                crate::tables::WatchedFolderRefusal::NoneToRemove => C::Internal,
                crate::tables::WatchedFolderRefusal::Unwatched(_)
                | crate::tables::WatchedFolderRefusal::AddedMeanwhile(_)
                | crate::tables::WatchedFolderRefusal::Overlap { .. }
                | crate::tables::WatchedFolderRefusal::NotWatched(_)
                | crate::tables::WatchedFolderRefusal::ParentWatched { .. }
                | crate::tables::WatchedFolderRefusal::InsideChanged { .. } => C::Import,
            },
            // The watched folders or the folder's grouping changed after the
            // reading was taken; a reading of another watched folder's
            // folder is bae's own fault.
            #[cfg(desktop)]
            Self::FolderReading(refusal) => match refusal {
                crate::tables::FolderReadingRefusal::WatchedElsewhere { .. }
                | crate::tables::FolderReadingRefusal::GroupedElsewhere { .. } => C::Import,
                crate::tables::FolderReadingRefusal::OtherRoot { .. } => C::Internal,
            },
            // The watched folder was removed, or read again, under the
            // reading; a reading the walk could not have handed over is
            // bae's own fault.
            #[cfg(desktop)]
            Self::FolderReread(refusal) => match refusal {
                crate::tables::FolderRereadRefusal::NeverRead(_)
                | crate::tables::FolderRereadRefusal::RootMoved(_) => C::Import,
                crate::tables::FolderRereadRefusal::NotDirectlyUnder { .. }
                | crate::tables::FolderRereadRefusal::DecisionOutside { .. }
                | crate::tables::FolderRereadRefusal::EntryOutside { .. } => C::Internal,
            },
            // The import pane combines only what it lists, at least two, so
            // a short pick or one that made nothing is bae's own fault; a
            // release that moved or was taken meanwhile is the person's to
            // pick again.
            #[cfg(desktop)]
            Self::Combine(refusal) => match refusal {
                crate::grouping::CombineRefusal::TooFewFolders
                | crate::grouping::CombineRefusal::BuiltNothing(_) => C::Internal,
                crate::grouping::CombineRefusal::AlreadyPresent
                | crate::grouping::CombineRefusal::MemberChanged(_)
                | crate::grouping::CombineRefusal::MemberTaken(_) => C::Import,
            },
            // The release was read apart meanwhile.
            #[cfg(desktop)]
            Self::Separation(crate::tables::SeparationRefusal::NotPicked(_)) => C::Import,
            // The candidate left, was imported or changed under the request,
            // or the import asking ended meanwhile.
            #[cfg(desktop)]
            Self::CandidateState(
                crate::tables::CandidateStateRefusal::NoState(_)
                | crate::tables::CandidateStateRefusal::FilesChanged { .. }
                | crate::tables::CandidateStateRefusal::NotScanned(_)
                | crate::tables::CandidateStateRefusal::NotOwner { .. }
                | crate::tables::CandidateStateRefusal::NoDraft(_),
            ) => C::Import,
            #[cfg(desktop)]
            Self::DiscogsKey(error) => match **error {
                crate::DiscogsKeyError::Keyring(_) | crate::DiscogsKeyError::Preferences(_) => {
                    C::Config
                }
                crate::DiscogsKeyError::Task(_) | crate::DiscogsKeyError::Missing => C::Internal,
            },
            #[cfg(desktop)]
            Self::Scan(_)
            | Self::InvalidFolder(_)
            | Self::UnreadableAudio(_)
            | Self::FileTags { .. }
            | Self::UnusableFile { .. }
            | Self::DocumentRead { .. }
            | Self::LocalCover { .. }
            | Self::DecodeVerification { .. }
            | Self::SourceRead { .. }
            | Self::Edit(_)
            | Self::Cover(_)
            | Self::SheetBinding { .. }
            | Self::WatchedFolder(_)
            | Self::Watch { .. } => C::Import,
        }
    }
}

/// The class a surface words a catalog's failure in: provider failures apart
/// from data bae cannot read and its own faults.
fn catalog_category(error: &bae_catalogs::CatalogError) -> bae_diagnostics::UiErrorCategory {
    #[cfg(desktop)]
    use bae_catalogs::discogs::client::DiscogsError;
    #[cfg(desktop)]
    use bae_catalogs::musicbrainz::MusicBrainzError;
    use bae_catalogs::remote_image::RemoteImageError;
    #[cfg(desktop)]
    use bae_catalogs::wikidata::WikidataError;
    use bae_catalogs::CatalogError;
    use bae_diagnostics::UiErrorCategory as C;
    use bae_util::http::TransportFailure;
    match error {
        #[cfg(desktop)]
        CatalogError::SourceData { .. } => C::ImportData,
        #[cfg(desktop)]
        CatalogError::DiscogsNotConfigured => C::Import,
        #[cfg(desktop)]
        CatalogError::MusicBrainz(error) => match error {
            MusicBrainzError::Other(_) => C::ImportData,
            MusicBrainzError::Request(_) => C::Internal,
            MusicBrainzError::NotFound(_)
            | MusicBrainzError::Network(_)
            | MusicBrainzError::Timeout
            | MusicBrainzError::Provider { .. } => C::Import,
        },
        #[cfg(desktop)]
        CatalogError::Wikidata(error) => match error {
            WikidataError::Other(_) => C::ImportData,
            WikidataError::Request(_) => C::Internal,
            WikidataError::NotFound(_)
            | WikidataError::Network(_)
            | WikidataError::Timeout
            | WikidataError::Provider { .. } => C::Import,
        },
        #[cfg(desktop)]
        CatalogError::Discogs(error) => match error {
            DiscogsError::Serialization(_) => C::ImportData,
            DiscogsError::Transport(error)
                if TransportFailure::of(error) == TransportFailure::Unmakeable =>
            {
                C::Internal
            }
            DiscogsError::Transport(_)
            | DiscogsError::Provider { .. }
            | DiscogsError::InvalidApiKey
            | DiscogsError::NotFound => C::Import,
        },
        CatalogError::RemoteImage(error) => match error {
            RemoteImageError::Unusable { .. } => C::ImportData,
            RemoteImageError::Transport { error, .. }
                if TransportFailure::of(error) == TransportFailure::Unmakeable =>
            {
                C::Internal
            }
            RemoteImageError::Transport { .. }
            | RemoteImageError::Status { .. }
            | RemoteImageError::NoImage { .. } => C::Import,
            RemoteImageError::CacheTask(_) => C::Internal,
        },
    }
}

/// A release that cannot be read onto the audio refuses the import that
/// would apply it.
#[cfg(desktop)]
impl From<bae_catalogs::reading::ReadingRefusal> for ImportError {
    fn from(refusal: bae_catalogs::reading::ReadingRefusal) -> Self {
        use bae_catalogs::reading::ReadingRefusal;
        match refusal {
            ReadingRefusal::TrackCount { folder, release } => {
                Self::MetadataTrackCount { folder_tracks: folder, release_tracks: release }
            }
            ReadingRefusal::Tied { mediums } => Self::TiedMediums { mediums },
            ReadingRefusal::Unreadable { catalog, detail } => {
                bae_catalogs::CatalogError::SourceData { catalog, detail }.into()
            }
        }
    }
}

#[cfg(all(test, desktop))]
#[path = "error_tests.rs"]
mod tests;
