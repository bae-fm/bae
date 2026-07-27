//! The typed error the import pipeline threads through every fallible step.
//!
//! Wire-level source errors (`MusicBrainzError`, `DiscogsError`) are kept
//! structurally rather than flattened to a string, so a consumer that needs to
//! tell a MusicBrainz timeout from a malformed payload can read it off the
//! chain. `do_import`, the terminal consumer, is the only place that turns this
//! back into a string, for the user-facing `ImportProgress::Failed { error }`.

use crate::import::MetadataSource;

/// Why an import failed. One class per distinguishable failure the pipeline
/// actually produces.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Walking the candidate folder failed (I/O during the tree walk).
    #[error("folder scan failed: {0}")]
    Scan(#[from] crate::import::folder_scanner::FolderScanError),

    /// The folder is structurally unimportable (corrupt/zero-byte audio,
    /// CUE referencing missing audio, no valid audio). Typed reason reused
    /// from the scanner.
    #[error("folder cannot be imported: {0}")]
    InvalidFolder(#[from] crate::import::folder_scanner::InvalidReason),

    /// A MusicBrainz request failed. Network/Timeout/Provider{status}
    /// distinctions preserved from the wire.
    #[error("MusicBrainz request failed: {0}")]
    MusicBrainz(#[from] crate::musicbrainz::MusicBrainzError),

    /// A Discogs request failed. RateLimit/InvalidApiKey/NotFound preserved.
    #[error("Discogs request failed: {0}")]
    Discogs(#[from] crate::discogs::client::DiscogsError),

    /// A Discogs operation was requested but no API key is configured.
    #[error("Discogs API key not configured")]
    DiscogsNotConfigured,

    /// The source responded, but its payload can't be mapped to a release (no
    /// artist credits, missing release_group, multi-side track with no side
    /// letter, medium with no tracks, no track title, ...).
    ///
    /// The field is `metadata_source`, not `source`: thiserror reserves a field
    /// literally named `source` for the error-chain source, which a
    /// `MetadataSource` (not an `Error`) can't be.
    #[error("{} release data cannot be mapped: {detail}", metadata_source.as_str())]
    SourceData {
        metadata_source: MetadataSource,
        detail: String,
    },

    /// Local file-tag evidence can't seed an Unknown import (no audio files,
    /// a file failed to open / parse, embedded-cover read failure).
    #[error("file tags cannot be read: {detail}")]
    FileTags { detail: String },

    /// A file the import must read cannot be used: audio that will not decode,
    /// a codec bae can't play, bytes that could not be hashed, or audio a track
    /// slot named that is no longer in the folder.
    ///
    /// The import's only remaining refusal. A disagreement between the source's
    /// tracklist and the folder's audio is a track slot to look at, not a
    /// failure — see [`TrackSlot`](crate::import::TrackSlot).
    #[error("{detail}")]
    UnusableFile { detail: String },

    /// Cover art selection/fetch/decode failed (download after retries,
    /// unrecognized image bytes, selected local cover missing, read/resize
    /// failure).
    #[error("cover art failed: {detail}")]
    CoverArt { detail: String },

    /// verify_decode_on_import found tracks that would import but not play.
    #[error("decode verification failed for {} track(s): {}", broken.len(), broken.join("; "))]
    DecodeVerification { broken: Vec<String> },

    /// Per-pressing duplicate rejection: an Exact identity already in the
    /// library. The Display text is user-facing — the UI renders it verbatim.
    #[error("This release is already in your library as \"{album_title}\"")]
    AlreadyInLibrary { album_title: String },

    /// The user-edit overlay is invalid (reuses the editor's typed error).
    #[error(transparent)]
    Edit(#[from] crate::import::EditValidationError),

    /// A library/DB operation failed (insert_import, finalize_import_atomic,
    /// resolve_artists_for_import, replacement plans, coven_make_remote).
    #[error(transparent)]
    Db(#[from] crate::library::LibraryError),

    /// A sheet-binding change could not be applied: the candidate, the sheet,
    /// or the audio named is not what it was when the picker offered it. Every
    /// offerable set that crosses to a UI is already filtered to what the sheet
    /// can use, so this is a folder that changed under the choice rather than a
    /// UI that offered something it should not have.
    #[error("that audio can no longer back this sheet: {detail}")]
    SheetBinding { detail: String },

    /// The watched-folder registry could not persist (YAML serialize / fs write).
    #[error("watched-folder registry: {detail}")]
    Registry { detail: String },

    /// A folder's OS-level filesystem watch could not be installed or
    /// removed (missing path, permissions). An OS/user condition, not a
    /// broken invariant.
    #[error("folder watch failed: {detail}")]
    Watch { detail: String },

    /// Config/keyring plumbing failed (Discogs key store/read).
    #[error("configuration error: {detail}")]
    Config { detail: String },

    /// A broken invariant, not a user condition: artist-id remap miss,
    /// non-UTF-8 path, spawn_blocking join failure, closed command channel,
    /// missing library-status row.
    #[error("internal import error: {detail}")]
    Internal { detail: String },
}

#[cfg(test)]
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

        let err: ImportError = MusicBrainzError::Provider { status: Some(503) }.into();
        assert!(matches!(
            err,
            ImportError::MusicBrainz(MusicBrainzError::Provider { status: Some(503) })
        ));
    }

    /// A Discogs rate-limit likewise survives the conversion so a retry policy
    /// can distinguish it from a hard failure.
    #[test]
    fn discogs_error_is_preserved_unflattened() {
        let err: ImportError = crate::discogs::client::DiscogsError::RateLimit.into();
        assert!(matches!(
            err,
            ImportError::Discogs(crate::discogs::client::DiscogsError::RateLimit)
        ));
    }
}
