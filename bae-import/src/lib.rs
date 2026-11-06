//! Import: watched folders and their scans, the candidates they hold and
//! the drafts a person shapes, identification against the catalogs, and the
//! import that writes a candidate's release into the library's store.

// Import is desktop-only: a phone browses, plays and casts, and neither imports
// nor edits a release's metadata, so no crate a phone builds depends on this
// one. The pipeline (scanning, transcoding, identification), the import tables
// a phone's library does not hold, and the release-edit model are each gated on
// `desktop` all the same; only `error`, `session` and `types` carry no gate.
#[cfg(desktop)]
pub mod apply_steps;
#[cfg(desktop)]
pub mod assemble;
#[cfg(desktop)]
pub mod audio_layout;
#[cfg(desktop)]
pub mod candidate_detail;
#[cfg(desktop)]
pub mod candidate_key;
#[cfg(desktop)]
pub(crate) mod candidate_runtime;
#[cfg(desktop)]
pub mod candidate_search;
#[cfg(desktop)]
mod catalog_sessions;
#[cfg(desktop)]
pub mod cover_art;
#[doc(hidden)]
#[cfg(desktop)]
pub mod cue_sheet;
#[cfg(desktop)]
pub mod discid;
mod error;
#[cfg(desktop)]
mod event_bus;
#[cfg(desktop)]
pub mod extraction;
#[cfg(desktop)]
mod file_evidence;
#[cfg(desktop)]
pub(crate) mod file_metadata_seed;
#[cfg(desktop)]
pub mod file_tag_mapper;
#[cfg(desktop)]
pub(crate) mod file_tag_reader;
#[cfg(desktop)]
pub(crate) mod file_tag_snapshot;
#[cfg(desktop)]
pub mod folder_scan_status;
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
pub mod identify_service;
#[cfg(desktop)]
pub(crate) mod import_cancel;
#[cfg(desktop)]
mod import_failure;
#[cfg(desktop)]
pub mod list;
#[cfg(desktop)]
pub mod loudness;
#[cfg(desktop)]
pub mod mapping;
#[cfg(desktop)]
pub mod pane;
#[cfg(desktop)]
pub mod preparations;
#[cfg(desktop)]
pub mod probe;
#[cfg(desktop)]
pub mod reading_draft;
#[cfg(desktop)]
pub mod release_candidate;
#[cfg(desktop)]
mod release_reads;
#[cfg(desktop)]
pub mod rip_reading;
#[cfg(desktop)]
mod scan_passes;
#[cfg(desktop)]
pub mod selection;
#[cfg(desktop)]
pub mod service;
pub mod session;
#[cfg(desktop)]
pub(crate) mod shared_album;
#[cfg(desktop)]
pub mod tables;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub mod test_fixtures;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub mod test_imports;
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
mod watching;

#[cfg(desktop)]
pub use tables::watched_path::{RelativeFolder, WatchedPathError};
#[cfg(desktop)]
pub use volume::check_period_minutes;
#[cfg(desktop)]
pub use volume::VolumeKind;

/// The import service's shared dependencies: the store it reads and writes,
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
    event_tx: ImportEventBus,
    /// The library's store, which import reads and writes directly.
    store: bae_store::Store,
    /// The library's records, which import reads where it links a candidate to
    /// the library.
    records: bae_records::Records,
    /// Import's own tables in that store.
    tables: tables::ImportTables,
    /// The catalogs as this library may ask them.
    catalogs: CatalogSessions,
    /// The provider images candidates' covers and new artists' pictures are
    /// fetched through.
    remote_images: bae_catalogs::remote_image::RemoteImageCache,
    preparations: preparations::CandidatePreparations,
    clock: coven::ClockRef,
    ids: coven::IdSourceRef,
    /// What reads a folder's audio files for their embedded tags. Held here
    /// rather than reached for, so a test can hand the scan a reader whose
    /// answers — and whose count of calls — it decides.
    file_tags: std::sync::Arc<dyn file_tag_reader::FileTagReader>,
    /// What lists a folder's entries for a scan. Held for the same reason as
    /// `file_tags`: a test can hand the scan a reader that holds one folder's
    /// listing closed, and observe what the scan has announced by then.
    directories: std::sync::Arc<dyn folder_scanner::DirectoryReader>,
    folder_state_commit: folder_state_commit::FolderStateCommit,
    /// The imports between their claim and their end, which a person can
    /// cancel: shared by the handle that claims and cancels them and the
    /// worker that runs them.
    import_cancels: import_cancel::ImportCancels,
    /// Where a panic in import or scan work is reported as the bug it is.
    diagnostics: bae_diagnostics::Diagnostics,
    /// Cancelled when the library closes, before its store does: the live
    /// reads import opens end with it.
    closing: tokio_util::sync::CancellationToken,
}

#[cfg(desktop)]
impl ImportServices {
    pub(crate) fn new(
        event_tx: ImportEventBus,
        store: bae_store::Store,
        records: bae_records::Records,
        catalogs: CatalogSessions,
        remote_images: bae_catalogs::remote_image::RemoteImageCache,
        preparations: preparations::CandidatePreparations,
        clock: coven::ClockRef,
        ids: coven::IdSourceRef,
        file_tags: std::sync::Arc<dyn file_tag_reader::FileTagReader>,
        diagnostics: bae_diagnostics::Diagnostics,
        closing: tokio_util::sync::CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            tables: tables::ImportTables::new(store.clone()),
            store,
            records,
            catalogs,
            remote_images,
            preparations,
            clock,
            ids,
            file_tags,
            directories: std::sync::Arc::new(folder_scanner::OsDirectoryReader),
            folder_state_commit: folder_state_commit::FolderStateCommit::default(),
            import_cancels: import_cancel::ImportCancels::default(),
            diagnostics,
            closing,
        }
    }
}

#[cfg(desktop)]
pub use apply_steps::{CatalogApplyStep, CatalogApplySteps};
#[cfg(desktop)]
pub use candidate_detail::{
    CandidateDraftPane, CandidateImportStatus, CandidatePanePlacement, CandidateStage,
    DraftProjection, ImportCandidateDetail, ImportCandidateDetailProjection, ListedRelease,
    ProjectedStage,
};
#[cfg(desktop)]
pub use candidate_key::CandidateKey;
#[cfg(desktop)]
pub use candidate_runtime::CandidateRuntime;
#[cfg(desktop)]
pub(crate) use candidate_runtime::CandidateWork;
#[cfg(desktop)]
pub use candidate_runtime::ImportPercentsWatch;
#[cfg(desktop)]
pub use candidate_runtime::{
    Admission, CandidateFiles, CandidateRuntimeSnapshot, IdentificationInFlight,
    IdentificationRuntime, ImportInFlight, SaveFailure, SavingAnswer,
};
#[cfg(any(test, feature = "test-utils"))]
pub use candidate_runtime::{
    CandidateRuntimeChange, CandidateRuntimeChanges, IdentificationCounts,
};
#[cfg(desktop)]
pub use candidate_search::{CandidateSearch, SearchStatus, SourceSearch};
#[cfg(desktop)]
pub use catalog_sessions::{CatalogSessionError, CatalogSessions, DiscogsKeyError};
#[cfg(desktop)]
pub use cover_art::CoverSelection;
#[cfg(desktop)]
pub use cover_art::{CoverChoice, CoverImageSource, CoverRefusal};
pub use error::ImportError;
#[cfg(desktop)]
pub use event_bus::{ImportEvent, ImportEventBus, ImportEvents, ScanEvent};
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub use event_bus::{ImportProgressFeed, ScanEvents};
#[cfg(desktop)]
pub use file_evidence::{file_evidence, EvidenceSignal, FileEvidence};
#[cfg(desktop)]
pub use folder_scan_status::{
    ActiveFolderScan, FolderScanActivity, FolderScanProgress, FolderScanStatus,
    WatchedFolderScanStatus,
};
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
    CandidateStanding, CoverTarget, GroupedSearchResults, ImportCandidateSnapshot,
    ImportServiceHandle, ImportStart, RemoteCoverGallery,
};
#[cfg(desktop)]
pub use identification::IdentificationHandle;
#[cfg(desktop)]
pub use identify_service::{IdentifyRunId, IdentifyService};
#[cfg(desktop)]
pub use import_failure::{ImportFailure, ImportFailureReason};
#[cfg(desktop)]
pub use list::{
    ImportCandidateListLocation, ImportCounts, ImportListId, ImportListItem, ImportListNarrowing,
    ImportListOrder, ImportListProjection, ImportListRequest, ImportListSnapshot, ImportListView,
    ImportListWindow, ImportQueueSummary, LiveImportList, LiveImportListError, NarrowedCount,
    PendingFilter, PendingFilterEntry,
};
#[cfg(desktop)]
pub use mapping::{
    mapping_tracks, FolderFiles, InvalidCandidateDetail, MappingContainer, MappingEntry,
    MappingFile, MappingFileRow, MappingImage, MappingRole, MappingSource, MappingTable,
    MappingTrackRun, MappingTrackSide, SheetBound, SheetGroup, TrackMapping,
};
#[cfg(desktop)]
pub use preparations::CandidatePreparations;
#[cfg(desktop)]
pub use preparations::{CandidateAsRead, CandidatePreparation, MetadataAuthor};
#[cfg(desktop)]
pub use release_reads::ReleaseReads;
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub use scan_passes::store_scan_item;
#[cfg(desktop)]
pub use service::ImportService;
pub use session::{
    CandidateSession, FindOnlineSection, MetadataPresentation, PaneCommand, PaneFailure, PaneMove,
    PaneOutcome, SearchForm, SearchTab,
};
#[cfg(desktop)]
pub use tables::SCHEMA_PARTS;

/// The schema this crate's tests open their stores on: records' and release
/// storage's parts, preferences' and import's.
#[cfg(all(desktop, any(test, feature = "test-utils")))]
pub const TEST_SCHEMA: bae_store::Schema = bae_store::Schema::new(&[
    bae_records::SCHEMA_PARTS,
    bae_preferences::SCHEMA_PARTS,
    SCHEMA_PARTS,
]);

#[cfg(desktop)]
pub use triage::{
    BadgeTone, CandidateAction, CandidateActionBasis, CandidateLiveState, IdentificationStatus,
    IdentifyingStep, ImportStanding, ImportedRelease, NeedsYouReason, PendingBadge,
    PendingStanding, PendingState, StoredLookup, TriageGroup, TriageImportStatus,
    TriageMetadataSummary, TriagePlacement, TriageRow, TriageRuntimeFacts, TriageSkipAction,
    TriageTab, TriageTabCounts,
};
#[cfg(desktop)]
pub use types::ImportCommand;
#[cfg(desktop)]
pub use types::ReleaseLink;
pub use types::{
    artists_standing, credited_line, ArtistAssignment, ArtistStanding, ArtistsStanding, AudioFile,
    CandidateDraft, CandidateEditField, CandidateTrack, Credited, DraftAlbum, DraftFieldEdit,
    DraftTrack, EditValidationError, LibraryDraft, LibraryTrack, MetadataProvenance,
    PressingFactEdit, RawLabelEdit, RawPressingEdit, RawReleaseEdit, RawTrackEdit, ReleaseDraft,
    ReleaseEditDisplayContext, ReleaseEditDraft, ReleaseEditLoadError, ReleaseEditSeed,
    ReleaseEditTrackContext, ReleaseEditTrackRun, ReleaseEditTrackSide, ReleaseEditTrackSource,
    ReleaseUserEdit, TrackArtistAssignments, TrackBinding, TrackUserEdit,
};
#[cfg(desktop)]
pub use types::{
    CandidateMetadataDraft, CueAnalyzedAudioFile, CueFlacAnalysis, ImportDestination, ImportPhase,
    ImportProgress, ImportStep, PrepareStep, TrackAudio, TrackFile,
};
#[cfg(desktop)]
pub use watched_folder::WatchedFolder;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
