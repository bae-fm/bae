// Gated with its two callers (`handle` and `service`), which the mobile builds
// leave out — the import editor is desktop-only.
desktop_only! {
    pub mod album_links;
    mod artist_assignments;
    pub(crate) mod assemble;
    pub(crate) mod release_metadata;
    pub(crate) mod candidate_runtime;
    pub mod candidate_search;
    pub(crate) mod candidates;
    pub mod grouping;
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
    mod file_identity;
    pub mod file_tag_mapper;
    pub(crate) mod file_tag_snapshot;
    pub(crate) mod file_metadata_seed;
    mod file_validation;
    pub mod folder_scanner;
    pub(crate) mod folder_state_commit;
    pub(crate) mod volume;
    pub mod watched_folder;
    pub use volume::check_period_minutes;
    pub use volume::VolumeKind;
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
    /// The carriers MusicBrainz and Discogs name, in each catalog's own
    /// closed list of format names.
    /// Which of a release's mediums a folder's audio is a rip of.
    pub(crate) mod medium_coverage;
    pub mod musicbrainz_mapper;
    pub mod pane;
    pub mod payloads;
    pub mod preparation;
    pub mod preparations;
    pub mod probe;
    pub(crate) mod pressing_evidence;
    pub mod release_candidate;
    pub mod release_group;
    pub mod search;
    pub(crate) mod service;
    pub mod source_release;
}
pub mod lookup_choices;
pub mod session;
desktop_only! {
    pub mod identification;
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

/// A parsed release (a catalog's document, or the files' own tags) in the shape
/// that flows into commit, which turns it into `albums` / `releases` / `tracks`
/// writes. The records are the pick's to state, not this mapping's, so they
/// reach commit alongside rather than inside it.
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
    event_tx: handle::ImportEventBus,
    library_manager: crate::library::LibraryManager,
    preparations: preparations::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdRef,
    /// What reads a folder's audio files for their embedded tags. Held here
    /// rather than reached for, so a test can hand the scan a reader whose
    /// answers — and whose count of calls — it decides.
    file_tags: std::sync::Arc<dyn file_tag_snapshot::FileTagReader>,
    /// What lists a folder's entries for a scan. Held for the same reason as
    /// `file_tags`: a test can hand the scan a reader that holds one folder's
    /// listing closed, and observe what the scan has announced by then.
    directories: std::sync::Arc<dyn folder_scanner::DirectoryReader>,
    folder_state_commit: folder_state_commit::FolderStateCommit,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ImportServices {
    pub(crate) fn new(
        event_tx: handle::ImportEventBus,
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
            file_tags: std::sync::Arc::new(file_tag_snapshot::LoftyFileTagReader),
            directories: std::sync::Arc::new(folder_scanner::OsDirectoryReader),
            folder_state_commit: folder_state_commit::FolderStateCommit::default(),
        }
    }
}

desktop_only! {
    pub use candidate_runtime::{CandidateRuntime, CandidateRuntimeChange};
    pub use candidate_search::{CandidateSearch, SearchStatus, SourceSearch};
    pub use candidates::{
        Admission, CandidateRuntimeSnapshot, CandidateStanding, FolderScanStatus,
        ImportCandidateSnapshot, ImportInFlight, ImportedRelease, WatchedFolderScanStatus,
    };
    pub use cover_art::{CoverChoice, CoverImageSource};
    pub use edits::{
        CandidateTrackEdit, ImportFailure,
        TrackEditState,
    };
}
pub(crate) use error::artist_source_ids_are_compatible;
pub use error::ArtistIdentityConflict;
pub use error::ImportError;
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) use folder_state_commit::{FolderStateCommit, FolderStateCommitGuard};
desktop_only! {
    pub use file_evidence::{file_evidence, EvidenceSignal, FileEvidence};
    pub use grouping::GroupingBlock;
    pub use folder_scanner::{
        FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
        InvalidReason, ReleaseFileScope, ReleasePart,
    };
    pub use handle::{
        parsed_album_to_user_edit, DiscogsSaveOutcome, GroupedSearchResults, ImportEvent,
        ImportEventBus, ImportServiceHandle, ScanEvent,
    };
    pub use list::{
        ActiveFolderScan, CandidateImportStatus, CandidatePanePlacement, ChosenFolder,
        FolderScanActivity,
        FolderScanProgress,
        ImportCandidateDetail,
        ImportCandidateDetailProjection, ImportCandidateListLocation, ImportListItem,
        ImportListOrder, ImportListProjection, ImportListRequest, ImportListSnapshot,
        ImportListSubscription, ImportListSubscriptionError, ImportListView, ImportListWindow,
        ImportQueueSummary, ReadyRowRef,
    };
    pub use mapping::{
        mapping_table, mapping_tracks, MappingBecomes,
        MappingContainer, MappingEntry, MappingFile, MappingFileRow, MappingImage, MappingRole,
        MappingSource, MappingTable, MappingTrackSection, MappingTrackSectionContent,
        PickedTracklist, SheetBound, SheetGroup, TrackMapping, TracklistSource,
    };
    pub use preparation::{CandidateAsRead, CandidatePreparation, MetadataAuthor};
    pub use preparations::CandidatePreparations;
    pub use search::{SearchQuery, SourceFailure, SourceLookup};
    pub use service::ImportService;
}
pub use lookup_choices::{ChoiceChange, LookupChoices, SearchWords};
pub use session::{CandidateSession, MetadataPresentation, SearchForm, SearchTab};
desktop_only! {
    pub use identification::IdentificationHandle;
    pub use track_slots::{
        lengths_disagree, SlotFile, SlotReconciliation, SlotSpan, SlotTable, SourceTrack, TrackSlot,
    };
    pub use triage::{
        CandidateAction, CandidateActionBasis, CandidateLiveState, IdentificationStatus,
        ImportedReleaseSummary, ImportedReleaseText, ImportedRow, MatchEvidence, MatchedPressing, MatchedRelease,
        MatchedSignal, TriageGroup, TriageImportStatus, TriageMetadataSummary, TriagePlacement,
        TriageRow, TriageRuntimeFacts, TriageSkipAction, TriageTab, TriageTabCounts,
    };
    pub(crate) use types::CandidateMappingPreparation;
    pub use types::ImportCommand;
}
pub use types::{
    artists_standing, asked_sources, is_the_only_asked_source, parse_catalog_url, ArtistAssignment,
    ArtistCredit, ArtistStanding, ArtistsStanding, AudioFile, CandidateDraft, CandidateEditField,
    CandidateTrack, Catalog, CatalogAvailability, CatalogPage, CreditResolution, DraftFieldEdit,
    EditValidationError, ExistingArtist, MetadataProvenance, MetadataRef, PressingFactEdit,
    RawPressingEdit, RawReleaseEdit, RawReleaseEditOf, RawTrackEdit, ReleaseEditSeed,
    ReleaseRecord, ReleaseUserEdit, ResolvedCredit, SourceAvailability, TrackArtistAssignments,
    TrackUserEdit,
};
desktop_only! {
    pub use types::{
        CandidateMetadataDraft, CandidatePreparedAssets, CoverSelection, ImportPhase,
        ImportProgress, ImportStep, PayloadSource, PrepareStep, PreparedArtistImage, ReleaseReseed,
        SourcePayload, StorageMode, TrackAudio, TrackFile,
    };
    pub use watched_folder::WatchedFolder;
}
