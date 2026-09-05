use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderCandidate {
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

#[cfg(feature = "desktop")]
impl BridgeInvalidReason {
    pub(crate) fn from_core(r: bae_core::import::InvalidReason) -> Self {
        use bae_core::import::InvalidReason as R;
        match r {
            R::CorruptAudioFile { path } => BridgeInvalidReason::CorruptAudioFile { path },
            R::CorruptImage { path } => BridgeInvalidReason::CorruptImage { path },
            R::NoValidAudio => BridgeInvalidReason::NoValidAudio,
        }
    }
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

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTriageImportStatus {
    Importing,
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

/// How often a watched folder is re-read, in whole minutes.
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
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriagePlacement {
    Pending,
    Identification {
        status: BridgeIdentificationStatus,
    },
    Ready,
    NeedsYou {
        reason: BridgeNeedsYou,
    },
    /// An import claimed this candidate and has not finished. Not Done: the
    /// folder is not in the library until the import says it is. How far it
    /// has got is `BridgeCandidateRuntimeSnapshot::import`.
    Importing,
    /// The last attempt failed and nothing has been attempted since. Pending,
    /// not Done: the folder is not in the library and the work is waiting on
    /// another attempt, which is the ordinary import the pane offers. What
    /// went wrong is the row's `BridgeTriageImportStatus::Error`.
    Failed,
    Done,
    Skipped,
}

/// The absolute skip-state command available for a sidebar row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageSkipAction {
    Skip,
    Unskip,
}

/// What identification is doing for a candidate with no stored verdict.
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
    AlreadyInLibrary,
    SeveralMatches {
        count: u32,
    },
    NoMatch,
    NothingToLookUp,
    LookupFailed,
    TrackCountDisagrees {
        local: u32,
        source: u32,
    },
    /// All three numbers cross even though the line names two: the tolerance is
    /// what makes the other two a disagreement rather than a rounding, and a
    /// surface that wants to show it should not have to re-derive it.
    DurationsDisagree {
        probed_ms: u64,
        source_ms: u64,
        tolerance_ms: u64,
    },
    SourceLengthsUnknown,
    LocalDurationUnknown,
}

impl BridgeNeedsYou {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::AlreadyInLibrary => "core.import.triage.already_in_library",
            Self::SeveralMatches { .. } => "core.import.triage.several_matches",
            Self::NoMatch => "core.import.triage.no_match",
            Self::NothingToLookUp => "core.import.triage.nothing_to_look_up",
            Self::LookupFailed => "core.import.triage.lookup_failed",
            Self::TrackCountDisagrees { .. } => "core.import.triage.track_count_disagrees",
            Self::DurationsDisagree { .. } => "core.import.triage.durations_disagree",
            Self::SourceLengthsUnknown => "core.import.triage.source_lengths_unknown",
            Self::LocalDurationUnknown => "core.import.triage.local_duration_unknown",
        }
    }
}

/// Localization key for the line a Needs-you row states its disagreement with —
/// resolved by the UI against the `Core` string table, which interpolates the
/// variant's own operands (durations formatted by the platform first).
#[uniffi::export]
pub fn bridge_needs_you_key(needs_you: &BridgeNeedsYou) -> String {
    needs_you.loc_key().to_string()
}

/// Which tab a placement puts the row in — the filter a tab bar applies.
#[uniffi::export]
pub fn bridge_triage_tab(placement: &BridgeTriagePlacement) -> BridgeTriageTab {
    match placement {
        BridgeTriagePlacement::Pending
        | BridgeTriagePlacement::Identification { .. }
        | BridgeTriagePlacement::Ready
        | BridgeTriagePlacement::NeedsYou { .. }
        | BridgeTriagePlacement::Importing
        | BridgeTriagePlacement::Failed => BridgeTriageTab::Pending,
        BridgeTriagePlacement::Done => BridgeTriageTab::Done,
        BridgeTriagePlacement::Skipped => BridgeTriageTab::Skipped,
    }
}

/// Which signal produced a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMatchedSignal {
    DiscId,
    Barcode,
}

/// Which provider answered and what matched — the row's trailing evidence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchEvidence {
    pub source: BridgeMetadataSource,
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

/// The metadata source selected for a candidate. Mirror of
/// `bae_core::import::MetadataProvenance`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataProvenance {
    ExternalRelease {
        source: BridgeMetadataSource,
        release_id: String,
        /// The other sources' releases the picked pressing paired with. The
        /// draft is read from `release_id`; each of these is the same pressing
        /// as another source has it, and the pick claims them all.
        partners: Vec<crate::types::BridgeMetadataRef>,
    },
    FileTags,
}

#[cfg(feature = "desktop")]
impl BridgeMetadataProvenance {
    pub(crate) fn from_core(pick: bae_core::import::MetadataProvenance) -> Self {
        match pick {
            bae_core::import::MetadataProvenance::ExternalRelease {
                source,
                release_id,
                partners,
            } => Self::ExternalRelease {
                source: BridgeMetadataSource::from_core(source),
                release_id,
                partners: partners
                    .into_iter()
                    .map(crate::types::BridgeMetadataRef::from_core)
                    .collect(),
            },
            bae_core::import::MetadataProvenance::FileTags => Self::FileTags,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::MetadataProvenance {
        match self {
            Self::ExternalRelease {
                source,
                release_id,
                partners,
            } => bae_core::import::MetadataProvenance::ExternalRelease {
                source: source.into_core(),
                release_id,
                partners: partners
                    .into_iter()
                    .map(crate::types::BridgeMetadataRef::into_core)
                    .collect(),
            },
            Self::FileTags => bae_core::import::MetadataProvenance::FileTags,
        }
    }
}

/// One candidate's sidebar row.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    /// The folder on disk: the mono subtitle, and the title when `matched` is
    /// absent.
    pub folder_name: String,
    /// Match against `BridgeWatchedFolder.path` for the section header.
    pub watched_folder_path: String,
    pub display_path: String,
    pub resolved_boundaries: Vec<BridgeResolvedFolderReleaseBoundary>,
    pub combine_ancestor_key: Option<BridgeFolderReleaseDecisionKey>,
    pub actionable: bool,
    pub placement: BridgeTriagePlacement,
    pub skip_action: Option<BridgeTriageSkipAction>,
    pub matched: Option<BridgeMatchedRelease>,
    pub metadata_summary: Option<BridgeTriageMetadataSummary>,
    /// The cover selected for this candidate, even when its metadata draft is
    /// otherwise blank.
    pub cover_thumbnail: Option<BridgeCoverImageSource>,
    /// Whether this row takes a bulk-import checkbox.
    pub selectable: bool,
    /// Where the candidate's import stands, without its progress: the row
    /// says *that* an import is running; how far along rides on the
    /// candidate's runtime.
    pub import_status: Option<BridgeTriageImportStatus>,
    /// The metadata provenance already recorded for this candidate.
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
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
