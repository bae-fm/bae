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
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
mod loudness;
pub mod musicbrainz_mapper;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod progress;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod release_group;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod search;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod service;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod track_to_file_mapper;
mod types;

use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
};
use coven::IdProvider;

pub(crate) fn find_or_push_artist(
    artists: &mut Vec<DbArtist>,
    matches: impl Fn(&DbArtist) -> bool,
    seed: impl FnOnce() -> (String, Option<String>, Option<String>, Option<String>),
    ids: &dyn IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    if let Some(existing) = artists.iter().find(|artist| matches(artist)) {
        return existing.id.clone();
    }

    let (name, sort_name, discogs_artist_id, musicbrainz_artist_id) = seed();
    let artist = DbArtist {
        id: ids.new_id(),
        name,
        sort_name,
        discogs_artist_id,
        musicbrainz_artist_id,
        created_at: now,
    };
    let id = artist.id.clone();
    artists.push(artist);
    id
}

pub(crate) fn album_artist_links(
    album_id: &str,
    artists: &[DbArtist],
    ids: &dyn IdProvider,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<DbAlbumArtist> {
    artists
        .iter()
        .enumerate()
        .skip(1)
        .map(|(position, artist)| {
            DbAlbumArtist::new(album_id, &artist.id, position as i32, ids.new_id(), now)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct ParsedWorkGraph {
    pub works: Vec<DbWork>,
    pub work_artists: Vec<DbWorkArtist>,
    pub work_parts: Vec<DbWorkPart>,
    pub track_works: Vec<DbTrackWork>,
}

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
    pub work_graph: ParsedWorkGraph,
    pub release_artist_roles: Vec<DbReleaseArtistRole>,
    pub track_artist_roles: Vec<DbTrackArtistRole>,
    /// One element per source the parser resolved for this release.
    /// Empty for Unknown imports (file-tag-only).
    pub identities: Vec<crate::import::types::ReleaseIdentity>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use folder_registry::{ImportFolderRegistry, WatchedFolder};
pub use folder_scanner::{
    scan_for_candidates_with_callback, CategorizedFiles, FileEntry, FolderCandidate,
    InvalidCandidate, InvalidReason, ScanItem,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use handle::{
    parsed_album_to_user_edit, shape_user_edit_from_search_detail, DiscogsSaveOutcome,
    GroupedSearchResults, ImportCandidateSnapshot, ImportCandidatesSnapshot, ImportEvent,
    ImportServiceHandle, ScanEvent, SearchQuery,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use progress::ImportProgressHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use service::ImportService;
pub use types::{
    CoverSelection, EditValidationError, IdentityChoice, ImportCommand, ImportPhase,
    ImportProgress, ImportStep, MetadataPointer, MetadataRef, MetadataSource, PrepareStep,
    PressingEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit, ReleaseIdentity, ReleaseUserEdit,
    StorageMode, TrackFile, TrackUserEdit,
};
