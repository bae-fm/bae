//! The typed error the import pipeline threads through every fallible step.
//!
//! Wire-level source errors (`MusicBrainzError`, `DiscogsError`) are kept
//! structurally rather than flattened to a string, so a consumer that needs to
//! tell a MusicBrainz timeout from a malformed payload can read it off the
//! chain. `do_import`, the terminal consumer, is the only place that turns this
//! back into a string, for the user-facing `ImportProgress::Failed { error }`.

#[cfg(desktop)]
use bae_catalogs::catalog::Catalog;

/// Two exact provider identities that currently belong to different library
/// artists. Import stops until the person chooses how this credit is used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artist '{incoming_artist_name}' has source IDs belonging to different library artists")]
pub struct ArtistIdentityConflict {
    pub incoming_artist_name: String,
    pub discogs_artist_id: String,
    pub musicbrainz_artist_id: String,
    pub discogs_artist: crate::import::ExistingArtist,
    pub musicbrainz_artist: crate::import::ExistingArtist,
}

/// The person's answer to an import's conflicting artist identities. Nothing
/// selects an answer automatically. The first four answers decide this credit;
/// changing the match opens the candidate's existing release picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistIdentityConflictChoice {
    /// Merge into the Discogs library artist, retaining both provider ids.
    SameArtist,
    /// Credit the Discogs library artist without attaching the incoming MusicBrainz id.
    DiscogsArtist,
    /// Credit the MusicBrainz library artist without attaching the Discogs id.
    MusicBrainzArtist,
    /// Create an independent artist without either provider id when importing.
    Neither,
    ChangeMatch,
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
    #[cfg(desktop)]
    #[error("folder scan failed: {0}")]
    Scan(#[from] crate::import::folder_scanner::FolderScanError),

    /// The folder is structurally unimportable (damaged audio, no valid
    /// audio). Typed reason reused from the scanner.
    #[cfg(desktop)]
    #[error("folder cannot be imported: {0}")]
    InvalidFolder(#[from] crate::import::folder_scanner::InvalidReason),

    /// A track sheet is bound to audio that holds nothing bae can read.
    #[cfg(desktop)]
    #[error("folder cannot be imported: {0}")]
    UnreadableAudio(#[from] crate::import::folder_scanner::UnreadableAudio),

    /// A MusicBrainz request failed. Network/Timeout/Provider{status}
    /// distinctions preserved from the wire.
    #[cfg(desktop)]
    #[error("MusicBrainz request failed: {0}")]
    MusicBrainz(#[from] bae_catalogs::musicbrainz::MusicBrainzError),

    /// A Wikidata request failed, with the same wire-level distinctions as
    /// a MusicBrainz one.
    #[cfg(desktop)]
    #[error("Wikidata request failed: {0}")]
    Wikidata(#[from] bae_catalogs::wikidata::WikidataError),

    /// A Discogs request failed. InvalidApiKey/NotFound preserved.
    #[cfg(desktop)]
    #[error("Discogs request failed: {0}")]
    Discogs(#[from] bae_catalogs::discogs::client::DiscogsError),

    /// A Discogs operation was requested but no API key is configured.
    #[cfg(desktop)]
    #[error("Discogs API key not configured")]
    DiscogsNotConfigured,

    /// The source responded, but its payload can't be mapped to a release (no
    /// artist credits, missing release_group, medium with no tracks, no track title, ...).
    ///
    /// The field is `catalog`, not `source`: thiserror reserves a field
    /// literally named `source` for the error-chain source, which a `Catalog`
    /// (not an `Error`) can't be.
    #[cfg(desktop)]
    #[error("{} release data cannot be mapped: {detail}", catalog.as_str())]
    SourceData { catalog: Catalog, detail: String },

    /// The release lists another number of tracks than the folder holds, so
    /// it cannot describe the folder's audio without adding or losing tracks.
    #[cfg(desktop)]
    #[error("{folder_tracks} tracks in the folder, {release_tracks} on the release")]
    MetadataTrackCount {
        folder_tracks: u32,
        release_tracks: u32,
    },

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

    /// The downloaded or selected cover cannot be parsed, decoded, or resized.
    #[error("cover art failed: {detail}")]
    CoverArt { detail: String },

    /// A catalog's image or an archive gallery could not be read.
    #[error(transparent)]
    RemoteImage(#[from] bae_catalogs::remote_image::RemoteImageError),

    /// The selected local cover disappeared or could not be read.
    #[cfg(desktop)]
    #[error("cover art failed: {detail}")]
    LocalCover { detail: String },

    /// verify_decode_on_import found tracks that would import but not play.
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
    SourceRead {
        track: String,
        error: bae_audio::sparse_buffer::SourceReadError,
    },

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
    /// same files landed first. Not desktop-only: the commit's write error
    /// carries it on every platform.
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
    GroupingBlocked {
        reason: crate::import::grouping::GroupingBlock,
    },

    /// The draft or edit does not shape into a savable one (the editor's
    /// typed error).
    #[cfg(desktop)]
    #[error(transparent)]
    Edit(#[from] crate::import::EditValidationError),

    /// A library/DB operation failed (insert_import, finalize_import_atomic,
    /// the artist credits it resolves, upload_release). Never a library
    /// error that is itself an import error: that converts back to the
    /// import error it carries.
    #[cfg(desktop)]
    #[error(transparent)]
    Db(crate::library::LibraryError),

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
    #[error("watched folder: {detail}")]
    WatchedFolder { detail: String },

    /// A folder's OS-level filesystem watch could not be installed or
    /// removed (missing path, permissions). An OS/user condition, not a
    /// broken invariant.
    #[cfg(desktop)]
    #[error("folder watch failed: {detail}")]
    Watch { detail: String },

    /// Config/keyring plumbing failed (Discogs key store/read).
    #[cfg(desktop)]
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
#[cfg(desktop)]
impl From<tokio::task::JoinError> for ImportError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::Internal {
            detail: format!("a candidate write did not complete: {error}"),
        }
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
        Self::FileTags {
            detail: format!("failed to {action} {}: {error}", path.display()),
        }
    }
}

/// A library error that is an import error is that import error again, so
/// the two never wrap each other.
#[cfg(desktop)]
impl From<crate::library::LibraryError> for ImportError {
    fn from(error: crate::library::LibraryError) -> Self {
        match error {
            crate::library::LibraryError::Import(error) => *error,
            other => Self::Db(other),
        }
    }
}

impl ImportError {
    /// This failure as a surface states it: its class, which the surface
    /// words in the person's language, and its own text as the untranslated
    /// detail.
    pub fn ui_error(&self) -> crate::ui::UiError {
        crate::ui::UiError::diagnostic(self.category(), self)
    }

    /// The class a surface words this failure in. Expected refusals and
    /// provider failures are kept apart from unusable source data and
    /// internal faults, and every variant chooses its class, so a new failure
    /// has to say how it is shown.
    pub fn category(&self) -> crate::ui::UiErrorCategory {
        use crate::ui::UiErrorCategory as C;
        #[cfg(desktop)]
        use bae_catalogs::discogs::client::DiscogsError;
        #[cfg(desktop)]
        use bae_catalogs::musicbrainz::MusicBrainzError;
        use bae_catalogs::remote_image::RemoteImageError;
        #[cfg(desktop)]
        use bae_catalogs::wikidata::WikidataError;
        match self {
            #[cfg(desktop)]
            Self::GroupingBlocked { reason } => reason.category(),
            #[cfg(desktop)]
            Self::Db(error) => error.category(),
            #[cfg(desktop)]
            Self::ImportWriting => C::Import,
            #[cfg(desktop)]
            Self::CandidateImportInProgress => C::CandidateImportInProgress,
            #[cfg(desktop)]
            Self::CandidateBeingIdentified => C::CandidateBeingIdentified,
            Self::FilesAlreadyImported { .. } => C::FilesAlreadyImported,
            #[cfg(desktop)]
            Self::MetadataTrackCount {
                folder_tracks,
                release_tracks,
            } => C::MetadataTrackCount {
                folder_tracks: *folder_tracks,
                release_tracks: *release_tracks,
            },
            #[cfg(desktop)]
            Self::TiedMediums { mediums } => C::TiedMediums {
                mediums: mediums.clone(),
            },
            #[cfg(desktop)]
            Self::SourceData { .. } => C::ImportData,
            Self::CoverArt { .. } => C::ImportData,
            #[cfg(desktop)]
            Self::MusicBrainz(error) => match error {
                MusicBrainzError::Other(_) => C::ImportData,
                MusicBrainzError::Request(_) => C::Internal,
                MusicBrainzError::NotFound(_)
                | MusicBrainzError::Network(_)
                | MusicBrainzError::Timeout
                | MusicBrainzError::Provider { .. } => C::Import,
            },
            #[cfg(desktop)]
            Self::Wikidata(error) => match error {
                WikidataError::Other(_) => C::ImportData,
                WikidataError::Request(_) => C::Internal,
                WikidataError::NotFound(_)
                | WikidataError::Network(_)
                | WikidataError::Timeout
                | WikidataError::Provider { .. } => C::Import,
            },
            #[cfg(desktop)]
            Self::Discogs(error) => match error {
                DiscogsError::Serialization(_) => C::ImportData,
                DiscogsError::Transport(error) if error.is_builder() || error.is_redirect() => {
                    C::Internal
                }
                DiscogsError::Transport(_)
                | DiscogsError::Provider { .. }
                | DiscogsError::InvalidApiKey
                | DiscogsError::NotFound => C::Import,
            },
            Self::RemoteImage(error) => match error {
                RemoteImageError::Unusable { .. } => C::ImportData,
                RemoteImageError::Transport { error, .. }
                    if error.is_builder() || error.is_redirect() =>
                {
                    C::Internal
                }
                RemoteImageError::Transport { .. } | RemoteImageError::Status { .. } => C::Import,
                RemoteImageError::CacheTask { .. } => C::Internal,
            },
            Self::Internal { .. } => C::Internal,
            #[cfg(desktop)]
            Self::Config { .. } => C::Config,
            #[cfg(desktop)]
            Self::Scan(_)
            | Self::InvalidFolder(_)
            | Self::UnreadableAudio(_)
            | Self::DiscogsNotConfigured
            | Self::FileTags { .. }
            | Self::UnusableFile { .. }
            | Self::LocalCover { .. }
            | Self::DecodeVerification { .. }
            | Self::SourceRead { .. }
            | Self::Edit(_)
            | Self::SheetBinding { .. }
            | Self::WatchedFolder { .. }
            | Self::Watch { .. } => C::Import,
        }
    }
}

#[cfg(all(test, desktop))]
mod tests {
    use super::*;
    use bae_catalogs::musicbrainz::MusicBrainzError;

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

    /// A Wikidata request that never got an answer is a provider failure,
    /// as a MusicBrainz one is, not release data that could not be mapped.
    #[test]
    fn wikidata_error_is_a_provider_failure() {
        let err: ImportError = bae_catalogs::wikidata::WikidataError::Timeout.into();
        assert!(matches!(
            err,
            ImportError::Wikidata(bae_catalogs::wikidata::WikidataError::Timeout)
        ));
        assert_eq!(err.category(), crate::ui::UiErrorCategory::Import);
    }

    /// A Discogs server error likewise survives the conversion so a retry
    /// policy can distinguish it from a hard failure.
    #[test]
    fn discogs_error_is_preserved_unflattened() {
        let err: ImportError = bae_catalogs::discogs::client::DiscogsError::Provider {
            status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
            told_wait: None,
        }
        .into();
        assert!(matches!(
            err,
            ImportError::Discogs(bae_catalogs::discogs::client::DiscogsError::Provider {
                status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
                ..
            })
        ));
    }
    #[test]
    fn invalid_request_preserves_the_discogs_transport_cause() {
        let request_error = reqwest::Client::new().get("not a URL").build().unwrap_err();
        let error = super::ImportError::from(
            bae_catalogs::discogs::client::DiscogsError::Transport(request_error),
        );
        assert!(
            error.to_string().contains("relative URL without a base"),
            "{error}"
        );
        let crate::signals::Failure::Internal(failure) =
            crate::import::search::failure_of(&error, "asking Discogs")
        else {
            panic!("a request bae could not build is its own failure");
        };
        assert!(
            failure.detail.contains("relative URL without a base"),
            "{}",
            failure.detail
        );
    }

    /// An import error that crosses into a library error stays the refusal it
    /// is, so the library's category is the import's own; crossing back gives
    /// the same import error, and a library error the import carried is that
    /// library error again rather than a wrapped one.
    #[test]
    fn an_import_error_keeps_its_kind_through_a_library_error() {
        use crate::library::LibraryError;
        use crate::ui::UiErrorCategory;
        let library = LibraryError::from(ImportError::TiedMediums {
            mediums: vec![1, 3],
        });
        assert_eq!(
            library.category(),
            UiErrorCategory::TiedMediums {
                mediums: vec![1, 3]
            }
        );
        assert!(matches!(
            ImportError::from(library),
            ImportError::TiedMediums { mediums } if mediums == [1, 3]
        ));

        let library = LibraryError::from(ImportError::MetadataTrackCount {
            folder_tracks: 10,
            release_tracks: 12,
        });
        assert_eq!(
            library.category(),
            UiErrorCategory::MetadataTrackCount {
                folder_tracks: 10,
                release_tracks: 12,
            }
        );

        let from_db = LibraryError::from(ImportError::Db(LibraryError::Database(
            crate::db::DbError::Message("read failed".into()),
        )));
        assert!(matches!(from_db, LibraryError::Database(_)), "{from_db:?}");
    }

    /// Only an error a provider answered with is a lookup that failed; one
    /// bae met on its own side is not, however it happened on the way to a
    /// lookup.
    #[test]
    fn only_a_provider_s_error_is_a_failed_lookup() {
        let refused: ImportError = bae_catalogs::musicbrainz::MusicBrainzError::Timeout.into();
        assert_eq!(
            crate::import::search::provider_failure(&refused),
            Some(crate::signals::LookupFailure::Timeout)
        );
        let local = ImportError::Internal {
            detail: "the release was stored and did not read back".to_string(),
        };
        assert_eq!(crate::import::search::provider_failure(&local), None);
    }

    /// A lookup that never reached MusicBrainz names what failed underneath — the
    /// DNS lookup, the refused connection — in the detail a surface shows, where
    /// reqwest's own text says only that sending failed. The request's path and
    /// query stay out of it.
    #[tokio::test]
    async fn a_musicbrainz_network_failure_names_its_cause_in_the_detail_a_surface_shows() {
        for (origin, cause) in [
            ("http://bae-test.invalid", "dns error"),
            ("http://127.0.0.1:1", "tcp connect error"),
        ] {
            let musicbrainz = bae_catalogs::musicbrainz::MusicBrainz::for_test(
                bae_util::http::Http::for_test().serve_every_host(origin),
            );
            let error = musicbrainz
                .lookup_by_discid(
                    "disc-unreachable",
                    bae_catalogs::rate_limiter::CallPriority::Interactive,
                )
                .await
                .expect_err("nothing answers there");
            assert!(matches!(error, MusicBrainzError::Network(_)), "{error}");
            let crate::ui::UiError::Diagnostic { category, detail } =
                ImportError::from(error).ui_error();
            assert_eq!(category, crate::ui::UiErrorCategory::Import);
            assert!(detail.contains(cause), "{detail}");
            assert!(!detail.contains("disc-unreachable"), "{detail}");
        }
    }

    /// A request that never reached Discogs names what failed underneath in the
    /// detail a surface shows, where reqwest's own text says only that sending
    /// failed.
    #[tokio::test]
    async fn a_discogs_transport_failure_names_its_cause_in_the_detail_a_surface_shows() {
        let client = bae_catalogs::discogs::client::DiscogsClient::new(
            std::sync::Arc::new(bae_catalogs::discogs::client::Discogs::for_test(
                bae_util::http::Http::for_test().serve("api.discogs.com", "http://127.0.0.1:1"),
            )),
            "token".to_string(),
        );
        let error = client
            .get_release(
                "510001",
                bae_catalogs::rate_limiter::CallPriority::Interactive,
            )
            .await
            .expect_err("nothing listens on port 1");
        assert!(
            matches!(
                error,
                bae_catalogs::discogs::client::DiscogsError::Transport(_)
            ),
            "{error}"
        );
        let crate::ui::UiError::Diagnostic { category, detail } =
            ImportError::from(error).ui_error();
        assert_eq!(category, crate::ui::UiErrorCategory::Import);
        assert!(detail.contains("tcp connect error"), "{detail}");
    }
}
