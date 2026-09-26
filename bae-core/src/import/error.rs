//! The typed error the import pipeline threads through every fallible step.
//!
//! Wire-level source errors (`MusicBrainzError`, `DiscogsError`) are kept
//! structurally rather than flattened to a string, so a consumer that needs to
//! tell a MusicBrainz timeout from a malformed payload can read it off the
//! chain. `do_import`, the terminal consumer, is the only place that turns this
//! back into a string, for the user-facing `ImportProgress::Failed { error }`.

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::import::Catalog;

/// Two exact provider identities that currently belong to different library
/// artists. Import stops until a person confirms that the two rows represent
/// one artist or corrects the source metadata.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artist '{incoming_artist_name}' has source IDs belonging to different library artists")]
pub struct ArtistIdentityConflict {
    pub incoming_artist_name: String,
    pub discogs_artist_id: String,
    pub musicbrainz_artist_id: String,
    pub discogs_artist: crate::import::ExistingArtist,
    pub musicbrainz_artist: crate::import::ExistingArtist,
}

/// Whether every provider ID both artists carry agrees. An absent ID makes no
/// claim; two present IDs for the same provider must be equal.
pub(crate) fn artist_source_ids_are_compatible(
    artist: &crate::db::DbArtist,
    discogs_artist_id: Option<&str>,
    musicbrainz_artist_id: Option<&str>,
) -> bool {
    source_ids_are_compatible(artist.discogs_artist_id.as_deref(), discogs_artist_id)
        && source_ids_are_compatible(
            artist.musicbrainz_artist_id.as_deref(),
            musicbrainz_artist_id,
        )
}

fn source_ids_are_compatible(left: Option<&str>, right: Option<&str>) -> bool {
    !matches!((left, right), (Some(left), Some(right)) if left != right)
}

/// Why an import failed. One class per distinguishable failure the pipeline
/// actually produces.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Walking the candidate folder failed (I/O during the tree walk).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("folder scan failed: {0}")]
    Scan(#[from] crate::import::folder_scanner::FolderScanError),

    /// The folder is structurally unimportable (corrupt/zero-byte audio,
    /// CUE referencing missing audio, no valid audio). Typed reason reused
    /// from the scanner.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("folder cannot be imported: {0}")]
    InvalidFolder(#[from] crate::import::folder_scanner::InvalidReason),

    /// A MusicBrainz request failed. Network/Timeout/Provider{status}
    /// distinctions preserved from the wire.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("MusicBrainz request failed: {0}")]
    MusicBrainz(#[from] crate::musicbrainz::MusicBrainzError),

    /// A Discogs request failed. RateLimit/InvalidApiKey/NotFound preserved.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("Discogs request failed: {0}")]
    Discogs(#[from] crate::discogs::client::DiscogsError),

    /// A Discogs operation was requested but no API key is configured.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("Discogs API key not configured")]
    DiscogsNotConfigured,

    /// The source responded, but its payload can't be mapped to a release (no
    /// artist credits, missing release_group, medium with no tracks, no track title, ...).
    ///
    /// The field is `catalog`, not `source`: thiserror reserves a field
    /// literally named `source` for the error-chain source, which a `Catalog`
    /// (not an `Error`) can't be.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("{} release data cannot be mapped: {detail}", catalog.as_str())]
    SourceData { catalog: Catalog, detail: String },

    /// Metadata cannot describe the included audio without adding or losing tracks.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("release has {metadata_tracks} tracks; the draft includes {audio_tracks}")]
    MetadataTrackCount {
        metadata_tracks: usize,
        audio_tracks: usize,
    },

    /// Local file-tag evidence can't seed a file-metadata import (no audio files,
    /// a file failed to open / parse, embedded-cover read failure).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("file tags cannot be read: {detail}")]
    FileTags { detail: String },

    /// A file the import must read cannot be used: audio that will not decode,
    /// a codec bae can't play, bytes that could not be hashed, or audio a track
    /// slot named that is no longer in the folder.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("{detail}")]
    UnusableFile { detail: String },

    /// The downloaded or selected cover cannot be parsed, decoded, or resized.
    #[error("cover art failed: {detail}")]
    CoverArt { detail: String },

    /// An artwork request failed with a known network or provider condition.
    /// The reason remains typed for lookup surfaces; detail retains its context.
    #[error("cover art request failed: {detail}")]
    CoverArtRequest {
        failure: crate::signals::LookupFailure,
        detail: String,
    },

    /// The selected local cover disappeared or could not be read.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("cover art failed: {detail}")]
    LocalCover { detail: String },

    /// verify_decode_on_import found tracks that would import but not play.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("decode verification failed for {} track(s): {}", broken.len(), broken.join("; "))]
    DecodeVerification { broken: Vec<String> },

    /// A source file could not be read while the import decoded it (loudness
    /// and decode verification). Carries the read's own error — a missing
    /// file, a full disk, a dropped network volume — and says nothing about
    /// the audio, unlike `DecodeVerification`: importing again once the source
    /// is readable is the remedy.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("the source audio for {track} could not be read: {error}")]
    SourceRead {
        track: String,
        error: std::sync::Arc<crate::playback::PlaybackError>,
    },

    /// Per-pressing duplicate rejection: an Exact identity already in the
    /// library. The Display text is user-facing — the UI renders it verbatim.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("This release is already in your library as \"{album_title}\"")]
    AlreadyInLibrary { album_title: String },

    /// Candidate preparation is immutable from the moment import owns it.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("This release is being imported and can no longer be edited")]
    CandidateImportInProgress,

    /// A bulk import reached a candidate identification is still answering: a
    /// run is queued for it, running, or writing what it found. The import
    /// would cancel the run and commit what the run is about to replace, so
    /// the candidate is left for the person to import once it is settled.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("This release is still being identified; import it once identification finishes")]
    CandidateBeingIdentified,

    /// A person cancelled the import before it wrote anything. The worker's
    /// own signal to stop, never a failure it records.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("the import was cancelled")]
    ImportCancelled,

    /// A cancel reached an import that is already writing its release, which
    /// is one transaction and completes.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("This release is already being written to the library and can no longer be cancelled")]
    ImportWriting,

    /// A release read from several folders cannot be worked on as it stands,
    /// or cannot be made: one of its folders changed or is gone, or the files
    /// of the folder they sit in go with another release or are still
    /// downloading.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("{reason}")]
    GroupingBlocked {
        reason: crate::import::grouping::GroupingBlock,
    },

    /// A completed import is edited through the persisted release editor, not
    /// through the candidate preparation it was created from.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("This release has already been imported; edit it in the library")]
    CandidateAlreadyImported,

    /// The user-edit overlay is invalid (reuses the editor's typed error).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error(transparent)]
    Edit(#[from] crate::import::EditValidationError),

    /// A library/DB operation failed (insert_import, finalize_import_atomic,
    /// the artist credits it resolves, replacement plans, coven_make_remote).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error(transparent)]
    Db(#[from] crate::library::LibraryError),

    /// A sheet-binding change could not be applied: the candidate, the sheet,
    /// or the audio named is not what it was when the picker offered it. Every
    /// offerable set that crosses to a UI is already filtered to what the sheet
    /// can use, so this is a folder that changed under the choice rather than a
    /// UI that offered something it should not have.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("that audio can no longer back this sheet: {detail}")]
    SheetBinding { detail: String },

    /// A file-role change could not be applied: the candidate or the file named
    /// is not what it was when the roles table offered it, or the change would
    /// leave the release with no tracks at all. Taking out the last of a
    /// folder's audio is the one role change that is refused — the folder would
    /// stop being a release, and there would be nothing left to import.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("that file's role can't be changed: {detail}")]
    FileRole { detail: String },

    /// A watched root or a candidate path under one is spelled in a way the
    /// store refuses to key by: relative, climbing out of itself, or not the
    /// one canonical spelling.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("watched folder: {detail}")]
    WatchedFolder { detail: String },

    /// A folder's OS-level filesystem watch could not be installed or
    /// removed (missing path, permissions). An OS/user condition, not a
    /// broken invariant.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("folder watch failed: {detail}")]
    Watch { detail: String },

    /// A folder someone chose to import could not be read: the read of its
    /// watched root failed, so what is stored for it says nothing about what
    /// it holds now.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("{path} could not be read: {detail}")]
    FolderUnread { path: String, detail: String },

    /// Config/keyring plumbing failed (Discogs key store/read).
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    #[error("configuration error: {detail}")]
    Config { detail: String },

    /// A broken invariant, not a user condition: artist-id remap miss,
    /// non-UTF-8 path, spawn_blocking join failure, closed command channel,
    /// missing library-status row.
    #[error("internal import error: {detail}")]
    Internal { detail: String },
}

/// A write the import handle ran to completion on its own task did not report
/// back: the task panicked, or the runtime is shutting down under it.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl From<tokio::task::JoinError> for ImportError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Internal {
            detail: format!("a candidate write did not complete: {error}"),
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ImportError {
    /// A file-tag read that failed at the file itself, named by what was being
    /// done to it: `file_tags("open", path, error)` reads "failed to open
    /// <path>: <error>".
    pub(crate) fn file_tags(
        action: &str,
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::FileTags {
            detail: format!("failed to {action} {}: {error}", path.display()),
        }
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;
    use crate::musicbrainz::MusicBrainzError;

    /// A MusicBrainz timeout raised with `?` arrives at the terminal consumer as
    /// a matchable `MusicBrainz(Timeout)`, so a retry policy can read the
    /// retryability straight off the variant.
    #[test]
    fn musicbrainz_error_is_preserved_unflattened() {
        let err: ImportError = MusicBrainzError::Timeout.into();
        assert!(matches!(
            err,
            ImportError::MusicBrainz(MusicBrainzError::Timeout)
        ));

        let err: ImportError = MusicBrainzError::Provider {
            status: Some(503),
            told_wait: None,
        }
        .into();
        assert!(matches!(
            err,
            ImportError::MusicBrainz(MusicBrainzError::Provider {
                status: Some(503),
                ..
            })
        ));
    }

    /// A Discogs rate-limit likewise survives the conversion so a retry policy
    /// can distinguish it from a hard failure.
    #[test]
    fn discogs_error_is_preserved_unflattened() {
        let err: ImportError =
            crate::discogs::client::DiscogsError::RateLimit { told_wait: None }.into();
        assert!(matches!(
            err,
            ImportError::Discogs(crate::discogs::client::DiscogsError::RateLimit { .. })
        ));
    }
    #[test]
    fn invalid_request_preserves_the_discogs_transport_cause() {
        let request_error = reqwest::Client::new().get("not a URL").build().unwrap_err();
        let error = super::ImportError::from(crate::discogs::client::DiscogsError::Transport(
            request_error,
        ));
        assert!(
            error.to_string().contains("RelativeUrlWithoutBase"),
            "{error}"
        );
        let crate::signals::LookupFailure::Diagnostic { detail } =
            crate::import::search::import_error_to_lookup_failure(&error)
        else {
            panic!("invalid request should be diagnostic");
        };
        assert!(detail.contains("RelativeUrlWithoutBase"), "{detail}");
    }
}
