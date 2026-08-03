pub mod artist_image;
// Gated with its two callers (`handle` and `service`), which the mobile builds
// leave out — the import editor is desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod artist_names;
mod assemble;
// Reads the identify state and the picked release's detail, both desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod claim;
pub mod cover_art;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discid;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod discid_hash;
pub mod discogs_mapper;
mod error;
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod loudness;
// Projects the folder's audio units against a picked tracklist — the desktop
// import pane's one structure, and desktop-only like the slots it reads.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod mapping;
pub mod musicbrainz_mapper;
// The payload store's projections build the picker detail and the commit's
// `ParsedAlbum` from archived documents — both desktop-only import shapes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod payloads;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod release_group;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod search;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod service;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod sweep;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod track_slots;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod triage;
mod types;

use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
};

/// The four-digit year at the head of a metadata date string (`"1998"`,
/// `"1998-05-01"`), or `None` when the value is absent or has no leading year.
pub(crate) fn parse_year(date: Option<&str>) -> Option<i32> {
    date?.split('-').next()?.parse().ok()
}

#[derive(Debug, Clone)]
pub struct ParsedWorkGraph {
    pub works: Vec<DbWork>,
    pub work_artists: Vec<DbWorkArtist>,
    pub work_parts: Vec<DbWorkPart>,
    pub track_works: Vec<DbTrackWork>,
}

/// A parsed release (MusicBrainz, Discogs, or file tags) in the shape that
/// flows into commit: commit turns `identities` into `release_identities` rows
/// and the rest into `albums` / `releases` / `tracks` writes.
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
pub use claim::{claim_line, ClaimEvidence, ClaimLine, ClaimRelease};
pub use error::ImportError;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use folder_registry::{ImportFolderRegistry, WatchedFolder};
pub use folder_scanner::{
    FolderCandidate, FolderReleaseBoundary, FolderReleaseDecision, FolderReleaseDecisionKey,
    FolderReleaseTreeRow, FolderReleaseTreeRowKind, InvalidCandidate, InvalidReason,
    ReleaseFileScope, ResolvedFolderReleaseBoundary,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use handle::{
    parsed_album_to_user_edit, shape_user_edit_for_choice, CandidateImportStatusSnapshot,
    CandidateRuntimeSnapshot, DiscogsSaveOutcome, FolderImportCandidateSnapshot, FolderScanStatus,
    GroupedSearchResults, ImportCandidateSnapshot, ImportCandidatesSnapshot, ImportEvent,
    ImportServiceHandle, ScanEvent, SearchQuery, WatchedFolderScanStatus,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use mapping::{
    mapping_table, mapping_tracks, mapping_with_track, mapping_without_file, mapping_without_track,
    MappingBecomes, MappingContainer, MappingEntry, MappingFile, MappingRole, MappingRow,
    MappingSource, MappingTable, MappingUnit, PickedTracklist, SheetBound, SheetGroup,
    TracklistSource,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use service::ImportService;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use sweep::QueueSweepHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use track_slots::{
    lengths_disagree, SlotFile, SlotReconciliation, SlotSpan, SlotTable, SourceTrack, TrackSlot,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use triage::{
    Answered, CandidateAnswer, IdentifyPhase, MatchEvidence, MatchedPressing, MatchedRelease,
    MatchedSignal, NeedsYouGroup, NeedsYouReason, TriageEntry, TriageGroup, TriagePlacement,
    TriageQueue, TriageRow, TriageSection, TriageTab, TriageTabCounts,
};
pub use types::{
    AudioFile, CoverSelection, DecidedIdentity, EditValidationError, IdentityChoice, IdentityPick,
    ImportCommand, ImportPhase, ImportProgress, ImportStep, MetadataPointer, MetadataRef,
    MetadataSource, PayloadSource, PrepareStep, PressingEdit, RawPressingEdit, RawReleaseEdit,
    RawTrackEdit, ReleaseIdentity, ReleaseUserEdit, SourcePayload, StorageMode, TrackFile,
    TrackUserEdit,
};
