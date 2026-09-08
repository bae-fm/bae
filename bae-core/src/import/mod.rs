// Gated with its two callers (`handle` and `service`), which the mobile builds
// leave out — the import editor is desktop-only.
desktop_only! {
    mod artist_assignments;
    mod assemble;
    pub(crate) mod candidate_runtime;
    pub mod candidate_search;
    pub(crate) mod candidates;
    pub mod combination;
}
pub mod cover_art;
desktop_only! {
    pub mod discid;
    mod discid_hash;
    pub mod discogs_mapper;
}
mod error;
desktop_only! {
    mod file_evidence;
    pub mod file_tag_mapper;
    pub(crate) mod file_tag_snapshot;
    mod file_validation;
    pub mod folder_scanner;
    pub(crate) mod volume;
    pub mod watched_folder;
    pub use volume::check_period_minutes;
    // The import pipeline (scanning, transcoding, identify orchestration) is
    // desktop-only; mobile is a sync/playback client. Only the shared domain
    // types below (re-exported from `types`) compile on mobile.
    mod edits;
    mod handle;
    pub mod list;
    pub(crate) mod local_artwork;
    mod loudness;
    // Projects the folder's audio units against a picked tracklist — the desktop
    // import pane's one structure, and desktop-only like the slots it reads.
    pub(crate) mod direct_entry_mapper;
    pub mod mapping;
    pub mod musicbrainz_mapper;
    // The payload store's projections build the picker detail and the commit's
    // `ParsedAlbum` from archived documents — both desktop-only import shapes.
    pub mod pane;
    pub mod payloads;
    pub mod preparation;
    pub mod preparations;
    pub mod probe;
    pub mod release_candidate;
    pub mod release_group;
    pub mod search;
    pub(crate) mod service;
}
pub mod session;
desktop_only! {
    pub mod sweep;
    pub mod track_slots;
    pub mod triage;
}
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

/// The import service's shared dependencies: the library it reads and writes,
/// the one writer of candidates' stored state, coven's clock and id sources,
/// the lock that serializes folder-state commits, and the channel every import
/// event goes out on.
///
/// [`service::ImportService::start`] builds one and hands it to the service
/// handle and to every folder scan, which is why it is declared here rather
/// than in either of those modules — its fields stay private to `import`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Clone)]
pub(crate) struct ImportServices {
    event_tx: tokio::sync::broadcast::Sender<handle::ImportEvent>,
    library_manager: crate::library::LibraryManager,
    preparations: preparations::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    folder_state_commit: std::sync::Arc<tokio::sync::Mutex<()>>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ImportServices {
    pub(crate) fn new(
        event_tx: tokio::sync::broadcast::Sender<handle::ImportEvent>,
        library_manager: crate::library::LibraryManager,
        preparations: preparations::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdRef,
    ) -> Self {
        Self {
            event_tx,
            library_manager,
            preparations,
            clock,
            ids,
            folder_state_commit: std::sync::Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

desktop_only! {
    pub use candidate_runtime::{CandidateRuntime, CandidateRuntimeChange};
    pub use candidate_search::{CandidateSearch, SearchStatus, SourceSearch};
    pub use candidates::{
        CandidateIdentifyRuntime, CandidateRuntimeSnapshot, CandidateStanding, FolderScanStatus,
        ImportCandidateSnapshot, ImportInFlight, ImportedRelease, WatchedFolderScanStatus,
    };
    pub use cover_art::{CoverChoice, CoverImageSource};
    pub use edits::{
        apply_track_edits, CandidateEditField, CandidateEditOverlay, CandidateTrackEdit,
        ImportFailure, TrackEditState,
    };
}
pub(crate) use error::artist_source_ids_are_compatible;
pub use error::ArtistIdentityConflict;
pub use error::ImportError;
desktop_only! {
    pub use file_evidence::{file_evidence, EvidenceSignal, FileEvidence};
    pub use folder_scanner::{
        FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
        InvalidReason, ReleaseFileScope, ResolvedFolderReleaseBoundary,
    };
    pub use handle::{
        parsed_album_to_user_edit, DiscogsSaveOutcome, GroupedSearchResults, ImportEvent,
        ImportServiceHandle, ScanEvent,
    };
    pub use list::{
        ActiveFolderScan, FirstUnidentifiedRowRef, FolderScanActivity, ImportCandidateDetail,
        ImportCandidateDetailProjection, ImportCandidateListLocation, ImportListItem,
        ImportListOrder, ImportListProjection, ImportListRequest, ImportListSnapshot,
        ImportListSubscription, ImportListSubscriptionError, ImportListView, ImportListWindow,
        ImportQueueSummary, ReadyRowRef,
    };
    pub use mapping::{
        mapping_table, mapping_tracks, mapping_with_track, mapping_without_track, MappingBecomes,
        MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
        MappingSource, MappingTable, MappingTrackSection, MappingTrackSectionContent,
        PickedTracklist, SheetBound, SheetGroup, TrackMapping, TracklistSource,
    };
    pub use preparation::{CandidateAsRead, CandidatePreparation, MetadataAuthor};
    pub use preparations::CandidatePreparations;
    pub use search::{SearchQuery, SourceFailure, SourceLookup};
    pub use service::ImportService;
}
pub use session::{CandidateSession, MetadataPresentation, SearchForm, SearchTab};
desktop_only! {
    pub use sweep::QueueSweepHandle;
    pub use track_slots::{
        lengths_disagree, SlotFile, SlotReconciliation, SlotSpan, SlotTable, SourceTrack, TrackSlot,
    };
    pub use triage::{
        CandidateAnswer, IdentificationStatus, MatchEvidence, MatchedPressing, MatchedRelease,
        MatchedSignal, TriageGroup, TriageImportStatus, TriageMetadataSummary, TriagePlacement,
        TriageRow, TriageRuntimeFacts, TriageSkipAction, TriageTab, TriageTabCounts,
    };
    pub(crate) use types::CandidateMappingPreparation;
    pub use types::ImportCommand;
}
pub use types::{
    asked_sources, is_the_only_asked_source, ArtistAssignment, AudioFile, CandidateDraft,
    CandidateTrack, EditValidationError, ExistingArtist, MetadataProvenance, MetadataSource,
    MetadataSourceAvailability, NewArtistSeed, PressingEdit, RawPressingEdit, RawReleaseEdit,
    RawReleaseEditOf, RawTrackEdit, ReleaseEditSeed, ReleaseIdentity, ReleaseUserEdit,
    SourceAvailability, TrackArtistAssignments, TrackFileAuthor, TrackUserEdit,
};
desktop_only! {
    pub use types::{
        CandidateMetadataDraft, CandidatePreparedAssets, CoverSelection, ImportPhase,
        ImportProgress, ImportStep, MetadataRef, PayloadSource, PrepareStep, PreparedArtistImage,
        ReleaseReseed, SourcePayload, StorageMode, TrackFile,
    };
    pub use watched_folder::WatchedFolder;
}
