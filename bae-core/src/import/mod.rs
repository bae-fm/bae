// Gated with its two callers (`handle` and `service`), which the mobile builds
// leave out — the import editor is desktop-only.
desktop_only! {
    pub mod album_links;
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
    /// A Discogs tracklist split into the mediums its positions name.
    pub(crate) mod discogs_mediums;
}
mod error;
desktop_only! {
    mod file_evidence;
    mod file_identity;
    pub mod file_tag_mapper;
    pub(crate) mod file_tag_snapshot;
    pub(crate) mod file_metadata_seed;
    pub(crate) mod track_numbers;
    mod file_validation;
    pub mod folder_scanner;
    pub(crate) mod folder_state_commit;
    pub(crate) mod import_cancel;
    pub(crate) mod volume;
    pub mod watched_folder;
    pub use volume::check_period_minutes;
    pub use volume::VolumeKind;
    // The import pipeline (scanning, transcoding, identify orchestration) is
    // desktop-only; mobile is a sync/playback client. Only the shared domain
    // types below (re-exported from `types`) compile on mobile.
    mod import_failure;
    mod handle;
    pub mod list;
    pub(crate) mod local_artwork;
    mod loudness;
    // The folder's audio units beside the draft's tracks — the desktop import
    // pane's one structure, and desktop-only like the scan it reads.
    pub mod mapping;
    /// Reading a catalog release against a candidate's audio, once.
    pub(crate) mod catalog_reading;
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
    /// What one reading of a source says a release is, as plain values.
    pub(crate) mod release_values;
    pub mod release_group;
    pub mod search;
    pub mod selection;
    pub mod shared_album;
    pub(crate) mod service;
    pub mod source_release;
}
pub mod lookup_choices;
pub mod session;
desktop_only! {
    pub mod identification;
    pub mod audio_layout;
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

/// The rows of the works one import writes.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

desktop_only! {
    pub use candidate_runtime::{CandidateRuntime, CandidateRuntimeChange, RuntimeSnapshotsWatch};
    pub(crate) use candidate_runtime::CandidateWork;
    pub use candidate_search::{CandidateSearch, SearchStatus, SourceSearch};
    pub use candidates::{
        Admission, CandidateRuntimeSnapshot, CandidateStanding, FolderScanStatus,
        IdentificationInFlight, ImportCandidateSnapshot, ImportInFlight, ImportedRelease,
        SaveFailure, SavingAnswer, WatchedFolderScanStatus,
    };
    pub use cover_art::{CoverChoice, CoverImageSource};
    pub use import_failure::{ImportFailure, ImportFailureReason};
}
pub(crate) use error::artist_source_ids_are_compatible;
pub use error::ImportError;
pub use error::{ArtistIdentityConflict, ArtistIdentityConflictChoice};
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
        DiscogsSaveOutcome, GroupedSearchResults, ImportEvent,
        ImportEventBus, ImportServiceHandle, ImportStart, ScanEvent,
    };
    pub(crate) use handle::{external_metadata_with_cover, VerdictWrite};
    pub use list::{
        ActiveFolderScan, CandidateImportStatus, CandidatePanePlacement,
        FolderScanActivity,
        FolderScanProgress,
        ImportCandidateDetail,
        ImportCandidateDetailProjection, ImportCandidateListLocation, ImportListItem,
        ImportListOrder, ImportListProjection, ImportListRequest, ImportListSnapshot,
        ImportListSubscription, ImportListSubscriptionError, ImportListView, ImportListWindow,
        ImportListNarrowing, ImportQueueSummary, NarrowedCount, PendingFilter, PendingFilterEntry,
        UploadStatus, UploadStatusKnown,
    };
    pub use mapping::{
        mapping_tracks, InvalidCandidateDetail, MappingContainer, MappingEntry, MappingFile,
        MappingFileRow, MappingImage, MappingRole, MappingSource, MappingTable,
        MappingTrackRun, MappingTrackSide, SheetBound, SheetGroup, TrackMapping,
    };
    pub use preparation::{CandidateAsRead, CandidatePreparation, MetadataAuthor};
    pub use preparations::CandidatePreparations;
    pub use search::{SearchQuery, SourceFailure, SourceLookup};
    pub use service::ImportService;
}
pub use lookup_choices::{ChoiceChange, LookupChoiceEdit, LookupChoices, SearchWords};
pub use session::{
    CandidateSession, FindOnlineSection, MetadataPresentation, PaneCommand, PaneFailure, PaneMove,
    PaneOutcome, SearchForm, SearchTab,
};
desktop_only! {
    pub use identification::IdentificationHandle;
    pub use triage::{
        CandidateAction, CandidateActionBasis, CandidateLiveState, IdentificationStatus,
        ImportedReleaseSummary, ImportedReleaseText, ImportedRow, MatchEvidence, MatchedPressing, MatchedRelease,
        MatchedSignal, TriageGroup, TriageImportStatus, TriageMetadataSummary, TriagePlacement,
        TriageRow, TriageRuntimeFacts, TriageSkipAction, TriageTab, TriageTabCounts,
        BadgeTone, ImportStanding, LiveStanding, NeedsYouReason, PendingBadge, PendingStanding,
        PendingState, StoredLookup,
    };
    pub use types::ImportCommand;
}
pub use types::{
    artists_standing, asked_sources, credited_line, is_the_only_asked_source, parse_catalog_url,
    AlbumLink, ArtistAssignment, ArtistCredit, ArtistStanding, ArtistsStanding, AudioFile,
    CandidateDraft, CandidateEditField, CandidateTrack, Catalog, CatalogAvailability, CatalogPage,
    CreditResolution, Credited, CreditedRole, DraftAlbum, DraftFieldEdit, DraftTrack,
    EditValidationError, ExistingArtist, GraphWork, LibraryDraft, LibraryTrack, MetadataProvenance,
    MetadataRef, PressingFactEdit, PressingLink, RawLabelEdit, RawPressingEdit, RawReleaseEdit,
    RawTrackEdit, ReleaseDraft, ReleaseEditSeed, ReleaseLink, ReleaseRecord, ReleaseUserEdit,
    ResolvedCredit, SourceAvailability, SourceCredit, TrackArtistAssignments, TrackBinding,
    TrackUserEdit, WorkGraph, WorkPart,
};
desktop_only! {
    pub use types::{
        CandidateMetadataDraft, CandidatePreparedAssets, CoverSelection, DiscogsArtistImage,
        ImportPhase, ImportDestination, ImportProgress, ImportStep, PayloadSource, PrepareStep,
        CueAnalyzedAudioFile, CueFlacAnalysis, SourcePayload, TrackAudio,
        TrackFile,
    };
    pub use watched_folder::WatchedFolder;
}
