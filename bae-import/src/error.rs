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

    /// Two library artists each hold one of a credit's catalog ids: the
    /// conflict a person resolves on the import page.
    #[cfg(desktop)]
    #[error("Import error: {0}")]
    ArtistIdentityConflict(Box<bae_db::ArtistIdentityConflict>),

    /// Two library labels each hold one of a label's catalog ids: the
    /// conflict a person resolves on the import page.
    #[cfg(desktop)]
    #[error("Import error: {0}")]
    LabelIdentityConflict(Box<bae_db::LabelIdentityConflict>),

    /// The release's credits cannot be linked to the library as it stands:
    /// their ids contradict the artist or label they match, or a picked
    /// artist no longer exists.
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
    WatchedFolder(bae_db::WatchedPathError),

    /// A folder's OS-level filesystem watch could not be installed or
    /// removed (missing path, permissions). An OS/user condition, not a
    /// broken invariant.
    #[cfg(desktop)]
    #[error("folder watch failed: {detail}")]
    Watch { detail: String },

    /// The library's store failed a read or a write import asked of it.
    #[cfg(desktop)]
    #[error(transparent)]
    Database(#[from] bae_db::DbError),

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
    /// A file-tag read that failed at the file itself, named by what was being
    /// done to it: `file_tags("open", path, error)` reads "failed to open
    /// <path>: <error>".
    pub(crate) fn file_tags(
        action: &str,
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::FileTags { detail: format!("failed to {action} {}: {error}", path.display()) }
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

/// An import's write to the library's records failed: a conflict a person
/// resolves, credits or labels that cannot be linked, or the store.
#[cfg(desktop)]
impl From<bae_db::IdentityWriteError> for ImportError {
    fn from(error: bae_db::IdentityWriteError) -> Self {
        use bae_db::IdentityWriteError as E;
        match error {
            E::ArtistIdentityConflict(conflict) => Self::ArtistIdentityConflict(conflict),
            E::LabelIdentityConflict(conflict) => Self::LabelIdentityConflict(conflict),
            E::Unresolvable(detail) => Self::Unresolvable { detail },
            E::Db(error) => Self::Database(error),
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
            | Self::ArtistIdentityConflict(_)
            | Self::LabelIdentityConflict(_)
            | Self::Unresolvable { .. }
            | Self::LibraryChanged => C::Import,
            #[cfg(desktop)]
            Self::Database(_)
            | Self::CatalogSession(crate::CatalogSessionError::Preferences(_)) => C::Database,
            #[cfg(desktop)]
            Self::CatalogSession(crate::CatalogSessionError::Keyring(error)) => {
                bae_diagnostics::key_category(error)
            }
            #[cfg(desktop)]
            Self::Sync(error) => bae_diagnostics::sync_category(error),
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
            Self::Stopped => C::Internal,
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
            DiscogsError::Transport(error) if error.is_builder() || error.is_redirect() => {
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
                if error.is_builder() || error.is_redirect() =>
            {
                C::Internal
            }
            RemoteImageError::Transport { .. } | RemoteImageError::Status { .. } => C::Import,
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
