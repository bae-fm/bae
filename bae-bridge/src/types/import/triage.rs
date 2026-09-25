use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderCandidate {
    pub composition_action: Option<BridgeCombinationAction>,
    pub combination: Option<BridgeCombination>,
    pub source_file_edits_allowed: bool,
    pub folder_path: String,
    pub source_folder_name: String,
    /// Absolute path of the watched folder this candidate was scanned from —
    /// the grouping key for the candidate-list section it renders under. Match
    /// it against `BridgeWatchedFolder.path` for the section's display name.
    pub watched_folder_path: String,
    /// Categorized files for this candidate. Delivered with the candidate so
    /// the receiver sees a fully populated value in a single event.
    pub files: BridgeCandidateFiles,
    /// Folder candidates always have files on disk and CUEs parsed during the
    /// scan, so track count is always known.
    pub track_count: u32,
    /// Whether the user manually marked this candidate as skipped — the import
    /// view tabs it under "Skipped".
    pub skipped: bool,
    /// Whether this candidate's file structure was already imported (matched by
    /// content hash). When true, the import view tabs it under "Added".
    pub is_added: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFolderReleaseDecisionKey {
    pub watched_folder_path: String,
    pub relative_folder_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFolderReleaseDecision {
    CombineAsOneRelease,
    KeepAsSeparateReleases,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeResolvedFolderReleaseBoundary {
    pub key: BridgeFolderReleaseDecisionKey,
    pub decision: BridgeFolderReleaseDecision,
    pub name: String,
    pub display_path: String,
}

/// Mirror of bae-core's `InvalidReason`. The UI localizes each variant via its
/// catalog key (`bridge_invalid_reason_key`), interpolating the path where set.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInvalidReason {
    CorruptAudioFile { path: String },
    CorruptImage { path: String },
    NoValidAudio,
}

impl BridgeInvalidReason {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::CorruptAudioFile { .. } => "core.import.invalid.corrupt_audio",
            Self::CorruptImage { .. } => "core.import.invalid.corrupt_image",
            Self::NoValidAudio => "core.import.invalid.no_valid_audio",
        }
    }
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeInvalidReason = bae_core::import::InvalidReason,
    from_core: pub(crate) fn,
    variants: {
        CorruptAudioFile { path },
        CorruptImage { path },
        NoValidAudio,
    },
}

/// Localization key for an invalid-candidate reason — resolved by the UI against
/// the `Core` string table; the UI interpolates the path arg where present.
#[uniffi::export]
pub fn bridge_invalid_reason_key(reason: BridgeInvalidReason) -> String {
    reason.loc_key().to_string()
}

/// A leaf folder that looks like a release but failed validation — the import
/// view surfaces it under the Skipped tab with a warning and the reason. Mirror
/// of `bae_core::import::InvalidCandidate`; carries no files or identify state
/// because an invalid folder can't be imported.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInvalidCandidate {
    pub folder_path: String,
    pub source_folder_name: String,
    /// Absolute path of the watched folder this was scanned from — the grouping
    /// key for the candidate-list section. Match it against
    /// `BridgeWatchedFolder.path` for the section's display name.
    pub watched_folder_path: String,
    pub display_path: String,
    pub resolved_boundaries: Vec<BridgeResolvedFolderReleaseBoundary>,
    /// Why the folder failed validation — the UI localizes this typed reason.
    pub reason: BridgeInvalidReason,
}

/// What one key has in flight after a change, its removal, or — after a
/// dropped delivery — every key in flight right now.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateRuntimeChange {
    Updated {
        key: String,
        runtime: BridgeCandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more.
    Removed { key: String },
    /// The subscription dropped changes; this is every key in flight right
    /// now. A consumer holding a key this does not list treats it as removed.
    Reset {
        runtimes: Vec<BridgeKeyedCandidateRuntime>,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeKeyedCandidateRuntime {
    pub key: String,
    pub runtime: BridgeCandidateRuntimeSnapshot,
}

/// What is happening for one candidate right now. Everything a finished run or
/// import leaves behind — the stored verdict, the extracted signals, the
/// release an import wrote, the error one failed with — is on the candidate's
/// row instead.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateRuntimeSnapshot {
    /// The live driver's state. `Idle` when no driver is running for the key
    /// and nothing terminal is being held.
    pub identify_state: BridgeIdentifyState,
    /// The badge row projected from `identify_state`, so both come from one
    /// value. Empty when `identify_state` is `Idle`.
    pub signals_toolbar: BridgeSignalsToolbar,
    /// The running import, or absent when none is.
    pub import: Option<BridgeImportInFlight>,
    /// The typed search submitted for this candidate, as its sources land.
    /// Absent before one is submitted and after it is cleared.
    pub search: Option<BridgeCandidateSearch>,
}

/// How far a running import has got.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportInFlight {
    pub progress_percent: Option<u32>,
    pub step: Option<BridgeImportStep>,
}

/// Where a candidate's import stands for the pane that shows it: running now,
/// or the outcome the last one left in the tables.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateImportStatus {
    Importing,
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: BridgeError,
    },
}

/// What the last import of a candidate left in the tables. An import running
/// now is the candidate's `BridgeCandidateLiveState`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTriageImportStatus {
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: BridgeError,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWatchedFolderScanStatus {
    pub watched_folder_path: String,
    pub watched_folder_name: String,
    pub status: BridgeFolderScanStatus,
    /// Whether this folder lives on a volume served over the network. Such a
    /// folder is checked on a schedule as well as watched, because a watch on
    /// a network mount reports only what this machine does to it — and the
    /// list says so, so a change made on the server that has not appeared yet
    /// is explained rather than mysterious.
    pub on_network_volume: bool,
}

/// The catalog key for what a network folder's indicator says on hover. Its
/// one argument is how often the folder is checked, which
/// [`bridge_network_folder_check_minutes`] answers — the two travel together so
/// the line cannot state an interval nothing uses.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_network_folder_watch_key() -> String {
    "core.import.folder.network_watch".to_string()
}

/// How often a watched folder on a network volume is checked, in whole
/// minutes.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_network_folder_check_minutes() -> u32 {
    bae_core::import::check_period_minutes()
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeFolderScanStatus {
    Scanning { found_count: u64 },
    Complete,
    Failed { error: String },
}

// ── Sidebar triage ─────────────────────────────────────────────────────────
//
// Mirrors `bae_core::import::triage` field for field and decides nothing. Every
// rule the sidebar renders — which tab, which group, which checkbox, which
// counts — is core's; a UI iterates these and formats them for its locale.

/// The sidebar's three tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageTab {
    Pending,
    Done,
    Skipped,
}

/// Where a row sits, including why a Pending row still needs input. One value
/// rather than a tab plus an optional group, so a surface cannot read half of
/// it.
///
/// Read from the tables alone. A run or an import is true of a candidate
/// wherever its row sits, so both are its `BridgeCandidateLiveState` instead.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriagePlacement {
    Pending,
    Ready,
    NeedsYou {
        reason: BridgeNeedsYou,
    },
    /// The last attempt failed and nothing has been attempted since. Pending,
    /// not Done: the folder is not in the library and the work is waiting on
    /// another attempt, which is the ordinary import the pane offers. What
    /// went wrong is the row's `BridgeTriageImportStatus::Error`.
    Failed,
    Done,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCandidateAction {
    ImportReady,
    Identify,
    RetryIdentification,
    ResetToFileMetadata,
    ClearMetadata,
    Skip,
    Restore,
}

/// What the tables say a row's commands are decided from: whether it can be
/// acted on, where it is placed, and whether its stored lookup failed.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateActionBasis {
    pub actionable: bool,
    pub placement: BridgeTriagePlacement,
    pub lookup_failed: bool,
}

/// What is running for one candidate right now, and the commands its row
/// offers with it — the part of a row that changes without a write, read per
/// row beside the list.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateLiveState {
    /// What identification is doing for the candidate. Absent when no run is
    /// queued, running or settling and the last one's write did not fail.
    pub identification: Option<BridgeIdentificationStatus>,
    /// Whether an import owns the candidate. How far it has got is
    /// `BridgeCandidateRuntimeSnapshot::import`.
    pub importing: bool,
    pub actions: Vec<BridgeCandidateAction>,
}

/// What identification is doing for a candidate right now, whatever its
/// placement says.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentificationStatus {
    /// Admitted to the queue but not started.
    Queued,
    /// Signals or provider lookups are in flight.
    Running,
    /// A terminal result is being committed.
    Finalizing,
    /// The terminal result could not be committed.
    FinalizationFailed { error: BridgeError },
}

/// Mirror of bae-core's `identify::NeedsYou`: one variant per question the user
/// is being asked, carrying the operands the row's line is built from. Every
/// number crosses raw — the UI formats it for its own locale and interpolates
/// it into the variant's `core.*` message (`bridge_needs_you_key`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNeedsYou {
    SeveralMatches { count: u32 },
    NoMatch,
    NothingToLookUp,
    LookupFailed,
    TrackCountDisagrees { local: u32, source: u32 },
    SourceTracksUnknown,
}

impl BridgeNeedsYou {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::SeveralMatches { .. } => "core.import.triage.several_matches",
            Self::NoMatch => "core.import.triage.no_match",
            Self::NothingToLookUp => "core.import.triage.nothing_to_look_up",
            Self::LookupFailed => "core.import.triage.lookup_failed",
            Self::TrackCountDisagrees { .. } => "core.import.triage.track_count_disagrees",
            Self::SourceTracksUnknown => "core.import.triage.source_tracks_unknown",
        }
    }
}

/// Localization key for the line a Needs-you row states its disagreement with —
/// resolved by the UI against the `Core` string table, which interpolates the
/// variant's own operands.
#[uniffi::export]
pub fn bridge_needs_you_key(needs_you: &BridgeNeedsYou) -> String {
    needs_you.loc_key().to_string()
}

/// Which signal produced a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMatchedSignal {
    DiscId,
    Barcode,
    /// The run searched the catalogs for the candidate's own album title,
    /// which is what it falls back on when no identifier named anything.
    TitleSearch,
}

/// Which provider answered and what matched — the row's trailing evidence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchEvidence {
    pub source: BridgeCatalog,
    /// `None` when nothing in the provenance names a signal; the row then shows
    /// the provider alone.
    pub signal: Option<BridgeMatchedSignal>,
}

/// The pressing-level facts about a match, present as a whole exactly when the
/// pressing is settled — absent while several are in play, because that is the
/// question the row is asking. The inner fields stay optional: a settled
/// pressing may state a year and no format.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchedPressing {
    pub year: Option<i32>,
    pub format: Option<String>,
    /// What the source says the release holds, when it has said.
    pub track_count: Option<u32>,
}

/// The release a row leads with. Absent as a whole when nothing matched, in
/// which case the row's title is `folder_name` and it has no metadata line —
/// there is no half-populated match to render. Present on Done and Skipped rows
/// too: a candidate already imported or set aside still shows what it matched.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchedRelease {
    /// The lead match's release id — what a bulk import commits a Ready row
    /// against, with no mapping pane to pick one in.
    pub release_id: String,
    /// The lead match's title, which with several matches stands in for the
    /// album — titles vary between the editions of one release group.
    pub title: String,
    /// The lead match's artist, with the same caveat as `title`.
    pub artist: Option<String>,
    pub pressing: Option<BridgeMatchedPressing>,
    /// Thumbnail-sized cover URL for the row's 40px art — the lead match's own
    /// sleeve, since cover art is fetched per release id.
    pub cover_thumbnail_url: Option<String>,
    pub evidence: BridgeMatchEvidence,
}

/// The candidate's applied editable metadata, projected for its sidebar row.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageMetadataSummary {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeArtistAssignment>,
}

/// Where a candidate's draft was read from. Mirror of
/// `bae_core::import::MetadataProvenance`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: crate::types::BridgeMetadataRef,
        /// The other catalogs' releases the picked pressing paired with. Each
        /// of these is the same pressing as another catalog has it, and the
        /// pick claims them all.
        partners: Vec<crate::types::BridgeMetadataRef>,
    },
    FileMetadata,
}

/// Who wrote the candidate's draft. Mirror of
/// `bae_core::import::MetadataAuthor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataAuthor {
    Nobody,
    Prefill,
    Identification,
    Person,
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeMetadataAuthor = bae_core::import::MetadataAuthor,
    from_core: pub(crate) fn,
    variants: { Nobody, Prefill, Identification, Person },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeMetadataProvenance = bae_core::import::MetadataProvenance,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: {
        ExternalRelease {
            record: (crate::types::BridgeMetadataRef),
            partners: (each crate::types::BridgeMetadataRef),
        },
        FileMetadata,
    },
}

/// What a row's text column says about its release. One value rather than a
/// flag beside a record list: "identified naming no catalog" and "prefilled
/// from a catalog" are both unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft read off the files' tags, or typed in.
    Prefilled,
    /// A draft read from a catalog's release, with every catalog that
    /// describes it, in the order surfaces list catalogs.
    Identified {
        records: Vec<crate::types::BridgeReleaseRecord>,
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeTriageReading = bae_core::import::triage::TriageReading,
    from_core: pub(crate) fn,
    variants: {
        Unidentified,
        Prefilled,
        Identified { records: (each crate::types::BridgeReleaseRecord) },
    },
}

/// One candidate's sidebar row.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder on disk: what a row leads with while its draft is blank,
    /// whatever identification matched.
    pub folder_name: String,
    /// Match against `BridgeWatchedFolder.path` for the section header.
    pub watched_folder_path: String,
    pub display_path: String,
    pub resolved_boundaries: Vec<BridgeResolvedFolderReleaseBoundary>,
    pub combine_ancestor_key: Option<BridgeFolderReleaseDecisionKey>,
    pub actionable: bool,
    pub placement: BridgeTriagePlacement,
    /// The Ready check this row did not pass — its release's tracklist
    /// disagrees with the folder, or there is none — stated beside Import.
    pub ready_check: Option<BridgeNeedsYou>,
    /// What the row's commands are decided from in the tables. Handed back
    /// with the row's live-state subscription, which answers with the commands
    /// themselves.
    pub action_basis: BridgeCandidateActionBasis,
    pub matched: Option<BridgeMatchedRelease>,
    pub metadata_summary: Option<BridgeTriageMetadataSummary>,
    /// The cover selected for this candidate, even when its metadata draft is
    /// otherwise blank.
    pub cover_thumbnail: Option<BridgeCoverImageSource>,
    /// Whether a bulk import can take this row when nothing is running for
    /// it. What is running is checked when the import runs.
    pub selectable: bool,
    /// What the last import of this candidate left in the tables.
    pub import_status: Option<BridgeTriageImportStatus>,
    /// Where this candidate's draft was read from, already recorded.
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
    /// How the row's text column reads: its folder, a draft, or a draft read
    /// from a catalog's release.
    pub reading: BridgeTriageReading,
}

/// A Done row: the candidate that became a library release, presented as that
/// release as the library has it now. Its own shape rather than a
/// `BridgeTriageRow` placed Done, so a Done row cannot show the candidate's
/// draft or pick — re-identifying, editing or re-covering the release in the
/// library is what the row shows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportedRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    pub display_path: String,
    /// Handed back with the row's live-state subscription: an import that
    /// just wrote the release can still own the candidate for a moment.
    pub action_basis: BridgeCandidateActionBasis,
    pub release: BridgeImportedReleaseSummary,
}

/// The library release a Done row became.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportedReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    /// The album's title. Empty for a release reseeded from tags that named
    /// none.
    pub title: String,
    /// The album's credited artists, joined, or absent when it credits none.
    pub artist: Option<String>,
    pub year: Option<i32>,
    /// The release's own cover.
    pub cover: Option<crate::types::BridgeImageRef>,
    /// Every catalog's description of the release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<crate::types::BridgeReleaseRecord>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageGroup {
    pub key: BridgeFolderReleaseDecisionKey,
    pub name: String,
    /// Whether the rows under this header are one folder read as several
    /// releases, and so whether the header offers to read them as one. `false`
    /// where the header is only a path component the rows share.
    pub combinable: bool,
}

/// How many rows each tab holds. Computed in core in the same pass that places
/// them — a UI never counts an array length, which would be wrong the moment a
/// filter is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}
