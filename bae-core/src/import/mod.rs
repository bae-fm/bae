// Gated with its two callers (`handle` and `service`), which the mobile builds
// leave out — the import editor is desktop-only.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod artist_assignments;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod assemble;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod candidate_runtime;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod candidate_search;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod candidates;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod combination;
pub mod cover_art;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discid;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod discid_hash;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod discogs_mapper;
mod error;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod file_evidence;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod file_tag_mapper;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod file_tag_snapshot;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod file_validation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod folder_registry;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod folder_scanner;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod volume;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use volume::check_period_minutes;
// The import pipeline (scanning, transcoding, identify orchestration) is
// desktop-only; mobile is a sync/playback client. Only the shared domain types
// below (re-exported from `types`) compile on mobile.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod edits;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod handle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod list;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod local_artwork;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
mod loudness;
// Projects the folder's audio units against a picked tracklist — the desktop
// import pane's one structure, and desktop-only like the slots it reads.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod direct_entry_mapper;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod mapping;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod musicbrainz_mapper;
// The payload store's projections build the picker detail and the commit's
// `ParsedAlbum` from archived documents — both desktop-only import shapes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod pane;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod payloads;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod probe;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod release_candidate;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod release_group;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod search;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) mod service;
pub mod session;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod sweep;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod track_slots;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub mod triage;
mod types;

#[cfg(not(any(target_os = "ios", target_os = "android")))]
use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
};

/// The four-digit year at the head of a metadata date string (`"1998"`,
/// `"1998-05-01"`), or `None` when the value is absent or has no leading year.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn parse_year(date: Option<&str>) -> Option<i32> {
    date?.split('-').next()?.parse().ok()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
    /// Empty for File Tags and direct-entry imports, which claim no external identity.
    pub identities: Vec<crate::import::types::ReleaseIdentity>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use candidate_runtime::{CandidateRuntime, CandidateRuntimeChange};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use candidate_search::{CandidateSearch, SourceSearch};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use candidates::{
    CandidateIdentifyRuntime, CandidateRuntimeSnapshot, FolderScanStatus, ImportCandidateSnapshot,
    ImportInFlight, ImportedRelease, WatchedFolderScanStatus,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use cover_art::{CoverChoice, CoverImageSource};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use edits::preserve_track_decisions;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use edits::{
    apply_track_edits, CandidateEditField, CandidateEditOverlay, CandidateTrackEdit, ImportFailure,
    TrackEditState,
};
pub(crate) use error::artist_source_ids_are_compatible;
pub use error::ArtistIdentityConflict;
pub use error::ImportError;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use file_evidence::{file_evidence, EvidenceSignal, FileEvidence};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use folder_registry::{ImportFolderRegistry, WatchedFolder};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
    InvalidReason, ReleaseFileScope, ResolvedFolderReleaseBoundary,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use handle::{
    parsed_album_to_user_edit, DiscogsSaveOutcome, GroupedSearchResults, ImportEvent,
    ImportServiceHandle, ScanEvent,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use list::{
    ActiveFolderScan, FirstUnidentifiedRowRef, FolderScanActivity, ImportCandidateDetail,
    ImportCandidateDetailProjection, ImportCandidateListLocation, ImportListItem, ImportListOrder,
    ImportListProjection, ImportListRequest, ImportListSnapshot, ImportListSubscription,
    ImportListSubscriptionError, ImportListView, ImportListWindow, ImportQueueSummary, ReadyRowRef,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use mapping::{
    mapping_table, mapping_tracks, mapping_with_track, mapping_without_track, MappingBecomes,
    MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
    MappingSource, MappingTable, MappingTrackSection, MappingTrackSectionContent, PickedTracklist,
    SheetBound, SheetGroup, TrackMapping, TracklistSource,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use search::{SearchQuery, SourceFailure, SourceLookup};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use service::ImportService;
pub use session::{CandidateSession, MetadataPresentation, SearchForm, SearchTab};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use sweep::QueueSweepHandle;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use track_slots::{
    lengths_disagree, SlotFile, SlotReconciliation, SlotSpan, SlotTable, SourceTrack, TrackSlot,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use triage::{
    CandidateAnswer, IdentificationStatus, MatchEvidence, MatchedPressing, MatchedRelease,
    MatchedSignal, TriageGroup, TriageImportStatus, TriageMetadataSummary, TriagePlacement,
    TriageRow, TriageRuntimeFacts, TriageSkipAction, TriageTab, TriageTabCounts,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use types::CandidateMappingPreparation;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use types::ImportCommand;
pub use types::{
    ArtistAssignment, AudioFile, CandidateDraft, CandidateTrack, EditValidationError,
    ExistingArtist, MetadataProvenance, MetadataSource, NewArtistSeed, PressingEdit,
    RawPressingEdit, RawReleaseEdit, RawReleaseEditOf, RawTrackEdit, ReleaseEditSeed,
    ReleaseIdentity, ReleaseUserEdit, TrackArtistAssignments, TrackFileAuthor, TrackUserEdit,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use types::{
    CandidateMetadataDraft, CandidatePreparedAssets, CoverSelection, ImportPhase, ImportProgress,
    ImportStep, MetadataRef, PayloadSource, PrepareStep, PreparedArtistImage, ReleaseReseed,
    SourcePayload, StorageMode, TrackFile,
};
