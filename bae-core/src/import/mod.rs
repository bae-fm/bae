// Every module but `error`, `lookup_choices`, `session` and `types` is the
// import pipeline (scanning, transcoding, identification), which only the
// desktop builds carry: mobile is a sync and playback client. Of `types`, only
// the shared domain types compile on mobile.
#[cfg(desktop)]
pub mod album_links;
#[cfg(desktop)]
pub(crate) mod assemble;
#[cfg(desktop)]
pub mod audio_layout;
#[cfg(desktop)]
pub(crate) mod candidate_runtime;
#[cfg(desktop)]
pub mod candidate_search;
#[cfg(desktop)]
pub(crate) mod candidates;
/// Reading a catalog release against a candidate's audio, once.
#[cfg(desktop)]
pub(crate) mod catalog_reading;
#[cfg(desktop)]
pub mod cover_art;
#[cfg(desktop)]
pub mod discid;
#[cfg(desktop)]
mod discid_hash;
#[cfg(desktop)]
pub mod discogs_mapper;
/// A Discogs tracklist split into the mediums its positions name.
#[cfg(desktop)]
pub(crate) mod discogs_mediums;
mod error;
#[cfg(desktop)]
mod file_evidence;
#[cfg(desktop)]
mod file_identity;
#[cfg(desktop)]
pub(crate) mod file_metadata_seed;
#[cfg(desktop)]
pub mod file_tag_mapper;
#[cfg(desktop)]
pub(crate) mod file_tag_snapshot;
#[cfg(desktop)]
mod file_validation;
pub(crate) mod folder_path;
#[cfg(desktop)]
pub mod folder_scanner;
#[cfg(desktop)]
pub(crate) mod folder_state_commit;
#[cfg(desktop)]
pub mod grouping;
#[cfg(desktop)]
mod handle;
#[cfg(desktop)]
pub mod identification;
#[cfg(desktop)]
pub(crate) mod import_cancel;
#[cfg(desktop)]
mod import_failure;
#[cfg(desktop)]
pub mod list;
#[cfg(desktop)]
pub(crate) mod local_artwork;
pub mod lookup_choices;
#[cfg(desktop)]
mod loudness;
#[cfg(desktop)]
pub mod mapping;
/// Which of a release's mediums a folder's audio is a rip of.
#[cfg(desktop)]
pub(crate) mod medium_coverage;
#[cfg(desktop)]
pub mod musicbrainz_mapper;
#[cfg(desktop)]
pub mod pane;
#[cfg(desktop)]
pub mod payloads;
#[cfg(desktop)]
pub mod preparation;
#[cfg(desktop)]
pub mod preparations;
#[cfg(desktop)]
pub(crate) mod pressing_evidence;
#[cfg(desktop)]
pub mod probe;
#[cfg(desktop)]
pub mod release_candidate;
#[cfg(desktop)]
pub mod release_group;
#[cfg(desktop)]
pub(crate) mod release_metadata;
/// What one reading of a source says a release is, as plain values.
#[cfg(desktop)]
pub(crate) mod release_values;
#[cfg(desktop)]
pub(crate) mod scrub_points;
#[cfg(desktop)]
pub mod search;
#[cfg(desktop)]
pub mod selection;
#[cfg(desktop)]
pub(crate) mod service;
pub mod session;
#[cfg(desktop)]
pub mod shared_album;
#[cfg(desktop)]
pub mod source_release;
#[cfg(desktop)]
pub(crate) mod track_numbers;
#[cfg(desktop)]
pub mod triage;
mod types;
#[cfg(desktop)]
pub(crate) mod volume;
#[cfg(desktop)]
pub mod watched_folder;

#[cfg(desktop)]
pub use volume::check_period_minutes;
#[cfg(desktop)]
pub use volume::VolumeKind;

#[cfg(desktop)]
use crate::db::{
    DbAlbum, DbAlbumArtist, DbArtist, DbRelease, DbReleaseArtistRole, DbTrack, DbTrackArtist,
    DbTrackArtistRole, DbTrackWork, DbWork, DbWorkArtist, DbWorkPart,
};

/// The four-digit year at the head of a metadata date string (`"1998"`,
/// `"1998-05-01"`), or `None` when the value is absent or has no leading year.
#[cfg(desktop)]
pub(crate) fn parse_year(date: Option<&str>) -> Option<i32> {
    date?.split('-').next()?.parse().ok()
}

/// The rows of the works one import writes.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub struct WorkGraphRows {
    pub works: Vec<DbWork>,
    pub work_artists: Vec<DbWorkArtist>,
    pub work_parts: Vec<DbWorkPart>,
    pub track_works: Vec<DbTrackWork>,
}

/// The rows one import writes, built from the candidate's draft
/// (`assemble::assemble_release`) and turned by the commit into `albums` /
/// `releases` / `tracks` writes. The records are the link's to state, not the
/// draft's, so they reach the commit alongside rather than inside it.
#[cfg(desktop)]
#[derive(Debug, Clone)]
pub struct ReleaseRows {
    pub album: DbAlbum,
    pub release: DbRelease,
    /// The release's mediums, in order; each track sits on one.
    pub mediums: Vec<crate::db::DbReleaseMedium>,
    pub tracks: Vec<TrackRow>,
    pub artists: Vec<DbArtist>,
    pub album_artists: Vec<DbAlbumArtist>,
    pub track_artists: Vec<DbTrackArtist>,
    pub work_graph: WorkGraphRows,
    pub release_artist_roles: Vec<DbReleaseArtistRole>,
    pub track_artist_roles: Vec<DbTrackArtistRole>,
}

/// A track row as the import builds it from the draft, before its audio is
/// bound: every column but the length, which only the audio it plays can
/// measure. Binding it makes the [`DbTrack`] (see
/// `audio_layout::resolve_track_files`), so no track row exists without its
/// length.
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq)]
pub struct TrackRow {
    pub id: String,
    pub release_id: String,
    pub title: String,
    pub medium_id: String,
    pub side: std::num::NonZeroU8,
    pub track_number: std::num::NonZeroU32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(desktop)]
impl TrackRow {
    /// The row, its audio measuring `duration_ms`.
    pub(crate) fn measured(self, duration_ms: u64) -> DbTrack {
        DbTrack {
            id: self.id,
            release_id: self.release_id,
            title: self.title,
            medium_id: self.medium_id,
            side: self.side,
            track_number: self.track_number,
            duration_ms,
            created_at: self.created_at,
        }
    }
}

/// The import service's shared dependencies: the library it reads and writes,
/// the one writer of candidates' stored state, coven's clock and id sources,
/// the lock that serializes folder-state commits, and the channel every import
/// event goes out on.
///
/// [`service::ImportService::start`] builds one and hands it to the service
/// handle and to every folder scan, which is why it is declared here rather
/// than in either of those modules — its fields stay private to `import`.
#[cfg(desktop)]
#[derive(Clone)]
pub(crate) struct ImportServices {
    event_tx: handle::ImportEventBus,
    library_manager: crate::library::LibraryManager,
    preparations: preparations::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    /// What reads a folder's audio files for their embedded tags. Held here
    /// rather than reached for, so a test can hand the scan a reader whose
    /// answers — and whose count of calls — it decides.
    file_tags: std::sync::Arc<dyn file_tag_snapshot::FileTagReader>,
    /// What lists a folder's entries for a scan. Held for the same reason as
    /// `file_tags`: a test can hand the scan a reader that holds one folder's
    /// listing closed, and observe what the scan has announced by then.
    directories: std::sync::Arc<dyn folder_scanner::DirectoryReader>,
    folder_state_commit: folder_state_commit::FolderStateCommit,
    /// The imports between their claim and their end, which a person can
    /// cancel: shared by the handle that claims and cancels them and the
    /// worker that runs them.
    import_cancels: import_cancel::ImportCancels,
}

#[cfg(desktop)]
impl ImportServices {
    pub(crate) fn new(
        event_tx: handle::ImportEventBus,
        library_manager: crate::library::LibraryManager,
        preparations: preparations::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
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
            import_cancels: import_cancel::ImportCancels::default(),
        }
    }
}

#[cfg(desktop)]
pub(crate) use candidate_runtime::CandidateWork;
#[cfg(desktop)]
pub use candidate_runtime::{CandidateRuntime, CandidateRuntimeChange, RuntimeSnapshotsWatch};
#[cfg(desktop)]
pub use candidate_search::{CandidateSearch, SearchStatus, SourceSearch};
#[cfg(desktop)]
pub use candidates::{
    Admission, CandidateFiles, CandidateRuntimeSnapshot, CandidateStanding, FolderScanStatus,
    IdentificationInFlight, IdentificationRuntime, ImportCandidateSnapshot, ImportInFlight,
    ImportedRelease, SaveFailure, SavingAnswer, WatchedFolderScanStatus,
};
#[cfg(desktop)]
pub use cover_art::{CoverChoice, CoverImageSource};
pub(crate) use error::artist_source_ids_are_compatible;
pub use error::ImportError;
pub use error::{ArtistIdentityConflict, ArtistIdentityConflictChoice};
#[cfg(desktop)]
pub use file_evidence::{file_evidence, EvidenceSignal, FileEvidence};
#[cfg(desktop)]
pub use folder_scanner::{
    FolderCandidate, FolderReleaseDecision, FolderReleaseDecisionKey, InvalidCandidate,
    InvalidReason, ReleaseFileScope, ReleasePart,
};
#[cfg(desktop)]
pub(crate) use folder_state_commit::{FolderStateCommit, FolderStateCommitGuard};
#[cfg(desktop)]
pub use grouping::GroupingBlock;
#[cfg(desktop)]
pub(crate) use handle::{external_metadata_with_cover, VerdictWrite};
#[cfg(desktop)]
pub use handle::{
    DiscogsSaveOutcome, GroupedSearchResults, ImportEvent, ImportEventBus, ImportServiceHandle,
    ImportStart, ScanEvent,
};
#[cfg(desktop)]
pub use identification::IdentificationHandle;
#[cfg(desktop)]
pub use import_failure::{ImportFailure, ImportFailureReason};
#[cfg(desktop)]
pub use list::{
    ActiveFolderScan, CandidateImportStatus, CandidatePanePlacement, FolderScanActivity,
    FolderScanProgress, ImportCandidateDetail, ImportCandidateDetailProjection,
    ImportCandidateListLocation, ImportListId, ImportListItem, ImportListNarrowing,
    ImportListOrder, ImportListProjection, ImportListRequest, ImportListSnapshot,
    ImportListSubscription, ImportListSubscriptionError, ImportListView, ImportListWindow,
    ImportQueueSummary, ListedRelease, NarrowedCount, PendingFilter, PendingFilterEntry,
};
pub use lookup_choices::{ChoiceChange, LookupChoiceEdit, LookupChoices, SearchWords};
#[cfg(desktop)]
pub use mapping::{
    mapping_tracks, InvalidCandidateDetail, MappingContainer, MappingEntry, MappingFile,
    MappingFileRow, MappingImage, MappingRole, MappingSource, MappingTable, MappingTrackRun,
    MappingTrackSide, SheetBound, SheetGroup, TrackMapping,
};
#[cfg(desktop)]
pub use preparation::{CandidateAsRead, CandidatePreparation, MetadataAuthor};
#[cfg(desktop)]
pub use preparations::CandidatePreparations;
#[cfg(desktop)]
pub use search::{SearchQuery, SourceFailure, SourceLookup};
#[cfg(desktop)]
pub use service::ImportService;
pub use session::{
    CandidateSession, FindOnlineSection, MetadataPresentation, PaneCommand, PaneFailure, PaneMove,
    PaneOutcome, SearchForm, SearchTab,
};
#[cfg(desktop)]
pub use triage::{
    BadgeTone, CandidateAction, CandidateActionBasis, CandidateLiveState, IdentificationStatus,
    IdentifyingStep, ImportStanding, NeedsYouReason, PendingBadge, PendingStanding, PendingState,
    StoredLookup, TriageGroup, TriageImportStatus, TriageMetadataSummary, TriagePlacement,
    TriageRow, TriageRuntimeFacts, TriageSkipAction, TriageTab, TriageTabCounts,
};
#[cfg(desktop)]
pub use types::ImportCommand;
pub use types::{
    artists_standing, asked_sources, credited_line, is_the_only_asked_source, AlbumLink,
    ArtistAssignment, ArtistCredit, ArtistStanding, ArtistsStanding, AudioFile, CandidateDraft,
    CandidateEditField, CandidateTrack, CatalogAvailability, CreditResolution, Credited,
    CreditedRole, DraftAlbum, DraftFieldEdit, DraftTrack, EditValidationError, ExistingArtist,
    GraphWork, LibraryDraft, LibraryTrack, MetadataProvenance, MetadataRef, PressingFactEdit,
    PressingLink, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit, ReleaseDraft,
    ReleaseEditSeed, ReleaseLink, ReleaseRecord, ReleaseUserEdit, ResolvedCredit,
    SourceAvailability, SourceCredit, TrackArtistAssignments, TrackBinding, TrackUserEdit,
    WorkGraph, WorkPart,
};
#[cfg(desktop)]
pub use types::{
    CandidateMetadataDraft, CandidatePreparedAssets, CoverSelection, CueAnalyzedAudioFile,
    CueFlacAnalysis, DiscogsArtistImage, ImportDestination, ImportPhase, ImportProgress,
    ImportStep, PayloadSource, PrepareStep, SourcePayload, TrackAudio, TrackFile,
};
#[cfg(desktop)]
pub use watched_folder::WatchedFolder;
