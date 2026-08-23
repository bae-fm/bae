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

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderReleaseTreeRow {
    pub name: String,
    pub display_path: String,
    pub depth: u32,
    pub kind: BridgeFolderReleaseTreeRowKind,
    pub decision_key: BridgeFolderReleaseDecisionKey,
    pub ancestor_decision_keys: Vec<BridgeFolderReleaseDecisionKey>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeFolderReleaseTreeRowKind {
    Folder,
    Candidate {
        track_count: u32,
        format_label: String,
    },
    Invalid {
        reason: BridgeInvalidReason,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderReleaseBoundary {
    pub key: BridgeFolderReleaseDecisionKey,
    pub name: String,
    pub display_path: String,
    pub shared_file_count: u32,
    pub tree_rows: Vec<BridgeFolderReleaseTreeRow>,
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
}

/// How far a running import has got.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportInFlight {
    pub progress_percent: u32,
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
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeFolderScanStatus {
    Scanning,
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
    Ready,
    NeedsYou {
        /// The header this row stacks under.
        group: BridgeNeedsYouGroup,
        /// The question itself, with its operands, so the row's line can be
        /// precise where its group header cannot.
        reason: BridgeNeedsYouReason,
    },
    /// An import claimed this candidate and has not finished. Not Done: the
    /// folder is not in the library until the import says it is. How far it
    /// has got is `BridgeCandidateRuntimeSnapshot::import`.
    Importing,
    Done,
    Skipped,
}

/// The absolute skip-state command available for a sidebar row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageSkipAction {
    Skip,
    Unskip,
}

/// The Needs-you group headers. Each UI localizes the variant from its own
/// catalog; the stacking order comes from
/// [`bridge_needs_you_groups_in_order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNeedsYouGroup {
    PickAPressing,
    SignalsDisagree,
    CountsOrLengthsDisagree,
    AlreadyInLibrary,
    NoMatch,
    StillIdentifying,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeNeedsYouReason {
    Disagreement {
        disagreement: BridgeNeedsYou,
    },
    /// No verdict yet — the row is dimmed and leaves this group on its own.
    /// `phase` says which of three unlike states it is in, so the row can say
    /// so rather than showing all three identically.
    StillIdentifying {
        phase: BridgeIdentifyPhase,
    },
}

/// How far identification has got for a candidate with no stored verdict.
/// Mirror of `bae_core::import::IdentifyPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentifyPhase {
    /// Nothing has run yet: the sweep has not reached this candidate.
    Queued,
    /// A run is in flight.
    Running,
    /// A run settled without an answer worth keeping — a lookup that never
    /// responded. It is retried on a later pass; nobody is waiting on it.
    NoAnswer,
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
    SignalsConflict,
    NoMatch,
    NothingToLookUp,
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
            Self::SignalsConflict => "core.import.triage.signals_conflict",
            Self::NoMatch => "core.import.triage.no_match",
            Self::NothingToLookUp => "core.import.triage.nothing_to_look_up",
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

/// The Needs-you groups in the order the sidebar stacks them. Ordering is a
/// domain decision, so it is stated once rather than in each UI. Mirrors
/// `bae_core::import::NeedsYouGroup::IN_ORDER`, which `triage_group_order`
/// pins it against.
#[uniffi::export]
pub fn bridge_needs_you_groups_in_order() -> Vec<BridgeNeedsYouGroup> {
    vec![
        BridgeNeedsYouGroup::PickAPressing,
        BridgeNeedsYouGroup::SignalsDisagree,
        BridgeNeedsYouGroup::CountsOrLengthsDisagree,
        BridgeNeedsYouGroup::AlreadyInLibrary,
        BridgeNeedsYouGroup::NoMatch,
        BridgeNeedsYouGroup::StillIdentifying,
    ]
}

/// Which tab a placement puts the row in — the filter a tab bar applies.
#[uniffi::export]
pub fn bridge_triage_tab(placement: &BridgeTriagePlacement) -> BridgeTriageTab {
    match placement {
        BridgeTriagePlacement::Ready
        | BridgeTriagePlacement::NeedsYou { .. }
        | BridgeTriagePlacement::Importing => BridgeTriageTab::Pending,
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

/// How far a claim on a picked release reaches. Mirror of
/// `bae_core::import::ClaimLevel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeClaimLevel {
    /// This pressing is the one in the room.
    Exact,
    /// The album, with which pressing left open.
    Approximate,
}

#[cfg(feature = "desktop")]
impl BridgeClaimLevel {
    pub(crate) fn from_core(level: bae_core::import::ClaimLevel) -> Self {
        match level {
            bae_core::import::ClaimLevel::Exact => Self::Exact,
            bae_core::import::ClaimLevel::Approximate => Self::Approximate,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::ClaimLevel {
        match self {
            Self::Exact => bae_core::import::ClaimLevel::Exact,
            Self::Approximate => bae_core::import::ClaimLevel::Approximate,
        }
    }
}

/// The identity decided for a candidate, as the row carries it back and the
/// pick command sends it down. Mirror of `bae_core::import::IdentityPick`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentityPick {
    Release {
        source: BridgeMetadataSource,
        release_id: String,
        /// How far the claim on this release reaches. Picking a release sends
        /// `Exact`; the header's claim control sends the same pick back at the
        /// level the user set.
        claim: BridgeClaimLevel,
    },
    Unknown,
}

#[cfg(feature = "desktop")]
impl BridgeIdentityPick {
    pub(crate) fn from_core(pick: bae_core::import::IdentityPick) -> Self {
        match pick {
            bae_core::import::IdentityPick::Release {
                source,
                release_id,
                claim,
            } => Self::Release {
                source: BridgeMetadataSource::from_core(source),
                release_id,
                claim: BridgeClaimLevel::from_core(claim),
            },
            bae_core::import::IdentityPick::Unknown => Self::Unknown,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::IdentityPick {
        match self {
            Self::Release {
                source,
                release_id,
                claim,
            } => bae_core::import::IdentityPick::Release {
                source: source.into_core(),
                release_id,
                claim: claim.into_core(),
            },
            Self::Unknown => bae_core::import::IdentityPick::Unknown,
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
    /// Whether this row takes a bulk-import checkbox.
    pub selectable: bool,
    /// Where the candidate's import stands, without its progress: the row
    /// says *that* an import is running; how far along rides on the
    /// candidate's runtime.
    pub import_status: Option<BridgeTriageImportStatus>,
    /// The identity already decided for this candidate — the settled single
    /// match, the pressing the user picked, or their decision to read the
    /// folder's own tags. Selection re-applies it, so the pane opens answered.
    pub picked: Option<BridgeIdentityPick>,
    /// The same decision in the shape commit takes, for a bulk import — which
    /// has no pane to read a claim line off. `None` alongside `picked`.
    pub claim: Option<BridgeIdentityChoice>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageGroup {
    pub key: BridgeFolderReleaseDecisionKey,
    pub name: String,
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
