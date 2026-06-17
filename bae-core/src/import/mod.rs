pub mod artist_image;
pub mod commit;
pub mod cover_art;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discid;
pub mod discogs_mapper;
pub mod file_tag_mapper;
mod file_validation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod folder_registry;
pub mod folder_scanner;
mod image_response;
// The import pipeline (scanning, transcoding, identify orchestration) is
// desktop-only; mobile is a sync/playback client. Only the shared domain types
// below (re-exported from `types`) compile on mobile.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod handle;
pub mod musicbrainz_mapper;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod progress;
pub mod search;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod service;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod track_to_file_mapper;
mod types;

use crate::db::{DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbTrack, DbTrackArtist};

/// Result of parsing a release (MusicBrainz or Discogs) into the
/// in-memory shape that flows into commit. Carries the per-source
/// identity rows alongside the DB-shape album/release/tracks; commit
/// turns identity into `release_identities` rows and the rest into
/// `albums` / `releases` / `tracks` writes.
#[derive(Debug, Clone)]
pub struct ParsedAlbum {
    pub album: DbAlbum,
    pub release: DbRelease,
    pub tracks: Vec<DbTrack>,
    pub artists: Vec<DbArtist>,
    pub album_artists: Vec<DbAlbumArtist>,
    pub track_artists: Vec<DbTrackArtist>,
    /// One element per source the parser resolved for this release.
    /// Empty for Unknown imports (file-tag-only).
    pub identities: Vec<crate::import::types::ReleaseIdentity>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use folder_registry::{ImportFolderRegistry, WatchedFolder};
pub use folder_scanner::{
    scan_for_candidates_with_callback, CategorizedFiles, FileEntry, FolderCandidate,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use handle::{
    parsed_album_to_user_edit, shape_user_edit_from_search_detail, DiscogsSaveOutcome, ImportEvent,
    ImportServiceHandle, ScanEvent, SearchQuery, SearchResultWithStatus,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use progress::ImportProgressHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use service::ImportService;
pub use types::{
    CoverSelection, EditValidationError, IdentityChoice, ImportCommand, ImportPhase,
    ImportProgress, MetadataPointer, MetadataRef, MetadataSource, PrepareStep, PressingEdit,
    RawPressingEdit, RawReleaseEdit, RawTrackEdit, ReleaseIdentity, ReleaseUserEdit, StorageMode,
    TrackFile, TrackUserEdit,
};
