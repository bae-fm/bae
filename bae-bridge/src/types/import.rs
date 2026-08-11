use super::*;

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

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportCandidateSnapshot {
    Folder {
        candidate: BridgeFolderCandidate,
        runtime_snapshot: BridgeCandidateRuntimeSnapshot,
        actionable: bool,
    },
    Invalid {
        candidate: BridgeInvalidCandidate,
    },
    Runtime {
        key: String,
        runtime_snapshot: BridgeCandidateRuntimeSnapshot,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderImportCandidateSnapshot {
    pub candidate: BridgeFolderCandidate,
    pub runtime: BridgeCandidateRuntimeSnapshot,
    pub actionable: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateRuntimeSnapshot {
    pub identify_state: BridgeIdentifyState,
    pub signals_toolbar: BridgeSignalsToolbar,
    pub signals: Option<BridgeSignals>,
    pub import_status: Option<BridgeCandidateImportStatus>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateImportStatus {
    Importing {
        progress_percent: u32,
        step: Option<BridgeImportStep>,
    },
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: BridgeError,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportCandidatesSnapshot {
    pub watched_folders: Vec<BridgeWatchedFolder>,
    pub folder_candidates: Vec<BridgeFolderImportCandidateSnapshot>,
    pub invalid_candidates: Vec<BridgeInvalidCandidate>,
    pub boundaries: Vec<BridgeFolderReleaseBoundary>,
    pub folder_scan_statuses: Vec<BridgeWatchedFolderScanStatus>,
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

/// The sidebar's four tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageTab {
    Ready,
    NeedsYou,
    Done,
    Skipped,
}

/// Where a row sits, and — under Needs you — why. One value rather than a tab
/// plus an optional group, so a surface cannot read half of it.
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
    /// folder is not in the library until the import says it is. The
    /// percentage rides on `BridgeTriageRow::import_status`.
    Importing,
    Done,
    Skipped,
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
        BridgeTriagePlacement::Ready => BridgeTriageTab::Ready,
        BridgeTriagePlacement::NeedsYou { .. } | BridgeTriagePlacement::Importing => {
            BridgeTriageTab::NeedsYou
        }
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

/// A candidate's decided identity with everything the pane seeds from it —
/// what the pick command and the selection query both return, so a fresh
/// launch renders exactly what the click rendered. Mirror of
/// `bae_core::import::DecidedIdentity`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDecidedIdentity {
    Release {
        source: BridgeMetadataSource,
        release_id: String,
        prefetch: BridgeReleasePrefetch,
    },
    Unknown {
        seed: BridgeReleaseUserEdit,
        mapping: BridgeMappingTable,
    },
}

#[cfg(feature = "desktop")]
impl BridgeDecidedIdentity {
    pub(crate) fn from_core(answer: bae_core::import::DecidedIdentity) -> Self {
        match answer {
            bae_core::import::DecidedIdentity::Release {
                source,
                release_id,
                prefetch,
            } => Self::Release {
                source: BridgeMetadataSource::from_core(source),
                release_id,
                prefetch: BridgeReleasePrefetch::from_core(prefetch),
            },
            bae_core::import::DecidedIdentity::Unknown { seed, mapping } => Self::Unknown {
                seed: BridgeReleaseUserEdit::from_core(seed),
                mapping: BridgeMappingTable::from_core(mapping),
            },
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
    pub matched: Option<BridgeMatchedRelease>,
    /// Whether this row takes a bulk-import checkbox.
    pub selectable: bool,
    pub import_status: Option<BridgeCandidateImportStatus>,
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

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTriageEntry {
    Candidate {
        stable_key: String,
        row: BridgeTriageRow,
    },
    Boundary {
        stable_key: String,
        boundary: BridgeFolderReleaseBoundary,
    },
    Invalid {
        stable_key: String,
        invalid_candidate: BridgeInvalidCandidate,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageSection {
    pub tab: BridgeTriageTab,
    pub watched_folder_path: String,
    pub group: Option<BridgeTriageGroup>,
    pub entries: Vec<BridgeTriageEntry>,
}

/// How many rows each tab holds. Computed in core in the same pass that places
/// them — a UI never counts an array length, which would be wrong the moment a
/// filter is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTriageTabCounts {
    pub ready: u32,
    pub needs_you: u32,
    pub done: u32,
    pub skipped: u32,
}

/// The whole sidebar.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageQueue {
    pub sections: Vec<BridgeTriageSection>,
    /// `skipped` counts the Skipped rows **plus** `invalid`.
    pub counts: BridgeTriageTabCounts,
    pub folder_scan_statuses: Vec<BridgeWatchedFolderScanStatus>,
}

/// A folder the user watches for imports — one candidate-list group.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWatchedFolder {
    /// Absolute path of the watched folder.
    pub path: String,
    /// Final path component — the group header label.
    pub name: String,
}

#[cfg(feature = "desktop")]
impl BridgeWatchedFolder {
    pub fn from_core(folder: bae_core::import::WatchedFolder) -> Self {
        let bae_core::import::WatchedFolder { path, name } = folder;
        Self { path, name }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFileInfo {
    pub name: String,
    pub size: u64,
    /// Directory prefix for display, e.g. "Artwork/". `None` when the file
    /// sits at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// Filename without directory, e.g. "front.jpg".
    pub file_name: String,
    /// Absolute filesystem path of the file on disk.
    pub local_path: String,
}

/// What a track sheet describes. Mirror of bae-core's `SheetBinding`; `file_id`
/// is a file's `name` (its release-relative path).
///
/// The scan proposes it from the sheet's `FILE` directive and the user can
/// overrule it — see `AppHandle::set_sheet_binding`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetBinding {
    /// Bound to the audio named by `file_id`.
    Describes { file_id: String },
    /// The sheet describes nothing: the directive names audio that is not in
    /// the folder, names several and only some are here, or the user cleared
    /// the binding. `requested` is what the directive asked for, so the pane
    /// can say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae can't carve tracks out of that codec.
    /// The audio imports as one track. The UI localizes `codec` through
    /// [`bridge_sheet_refused_codec_key`].
    RefusedCodec { file_id: String, codec: String },
}

/// Whether one of a candidate's audio files can back a sheet's binding. Mirror
/// of bae-core's `SheetBindingOffer`. Core decides this by probing, so no UI
/// reads a codec to work out what it may offer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetBindingOffer {
    /// The sheet can be bound to this audio.
    Offered,
    /// bae can't carve tracks out of that codec. The UI localizes `codec`
    /// through [`bridge_sheet_refused_codec_key`] — the same wording a sheet
    /// the scan already refused carries.
    RefusedCodec { codec: String },
    /// bae can't read the file at all. Localized through
    /// [`bridge_sheet_refused_unreadable_key`].
    RefusedUnreadable,
}

impl BridgeSheetBindingOffer {
    pub(crate) fn loc_key(&self) -> Option<&'static str> {
        match self {
            Self::Offered => None,
            Self::RefusedCodec { .. } => Some(SHEET_REFUSED_CODEC_KEY),
            Self::RefusedUnreadable => Some(SHEET_REFUSED_UNREADABLE_KEY),
        }
    }
}

/// Localization key for why a file cannot back a sheet's binding — resolved by
/// the UI against the `Core` string table, interpolating `codec` where the
/// variant carries one. `None` for a file that *is* offerable: it needs no
/// reason, which is what makes an offer and a refusal distinguishable without a
/// UI reading the variant.
#[uniffi::export]
pub fn bridge_sheet_binding_offer_key(offer: BridgeSheetBindingOffer) -> Option<String> {
    offer.loc_key().map(str::to_string)
}

/// One of a candidate's audio files, as a choice for a sheet's binding. The set
/// crosses already filtered to what the sheet can use, each refusal carrying
/// its reason: offering a file the commit would reject is the failure the
/// editable binding exists to remove.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSheetBindingOption {
    /// The audio file's `name` (its release-relative path) — the id
    /// `AppHandle::set_sheet_binding` takes, and the one to match against
    /// `BridgeFileInfo.name` for anything else the row shows.
    pub file_id: String,
    pub offer: BridgeSheetBindingOffer,
}

/// Localization key for a refused sheet binding — resolved by the UI against the
/// `Core` string table, with the codec interpolated. One key, so the reason a
/// binding was refused reads the same on every surface.
#[uniffi::export]
pub fn bridge_sheet_refused_codec_key() -> String {
    SHEET_REFUSED_CODEC_KEY.to_string()
}

/// Localization key for audio bae cannot read, refused as a binding for that
/// reason rather than for its codec.
#[uniffi::export]
pub fn bridge_sheet_refused_unreadable_key() -> String {
    SHEET_REFUSED_UNREADABLE_KEY.to_string()
}

pub(crate) const SHEET_REFUSED_CODEC_KEY: &str = "core.import.sheet.refused_codec";
pub(crate) const SHEET_REFUSED_UNREADABLE_KEY: &str = "core.import.sheet.refused_unreadable";

/// The job the scan proposed for one file. Mirror of bae-core's `FileRole`. No
/// UI decides a file's role, and no UI infers a pairing from a filename.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeFileRole {
    Audio,
    /// A parsed track sheet, with what its `FILE` directive resolved to.
    TrackSheet {
        binding: BridgeSheetBinding,
        /// Playable tracks the sheet carves.
        track_count: u32,
    },
    /// The image that leads the release.
    Cover {
        choice: BridgeCoverChoice,
    },
    Artwork {
        choice: BridgeCoverChoice,
    },
    Document,
    /// In the folder and carried with the release, unrecognized — a scene
    /// sidecar, a stray video, a file with no extension.
    Other,
}

/// The catalog key naming the role in force for a file — the roles table's
/// Role column. Core's concept, so core's wording: two UIs naming these
/// differently is two answers about what the release holds.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_role_key(role: &BridgeFileRole) -> String {
    match role {
        BridgeFileRole::Audio => "core.import.role.audio",
        BridgeFileRole::TrackSheet { .. } => "core.import.role.track_sheet",
        BridgeFileRole::Cover { .. } => "core.import.role.cover",
        BridgeFileRole::Artwork { .. } => "core.import.role.artwork",
        BridgeFileRole::Document => "core.import.role.document",
        BridgeFileRole::Other => "core.import.role.other",
    }
    .to_string()
}

/// A role a person can put a file in, as opposed to the whole
/// [`BridgeFileRole`] the scan proposes. Mirror of bae-core's
/// `FileRoleChoice`. Only audio is a decision: an image is an image, and a
/// track sheet's job is decided by what it is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileRoleChoice {
    /// One of the release's tracks.
    Audio,
    /// Carried with the release — the folder is the release — but not one of
    /// its tracks. What a slot's Exclude action writes.
    NotATrack,
}

/// The catalog key naming one file-role choice.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_role_choice_key(choice: BridgeFileRoleChoice) -> String {
    match choice {
        BridgeFileRoleChoice::Audio => "core.import.role.audio",
        BridgeFileRoleChoice::NotATrack => "core.import.role.not_a_track",
    }
    .to_string()
}

/// What a file's role makes of it in the release being imported — the roles
/// table's "Becomes" column, as a consequence rather than as prose. Mirror of
/// bae-core's `FileBecomes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileBecomes {
    /// Track slots `first`..=`last`, counting the release's slots from one.
    /// `first == last` is the single-slot case a loose audio file produces.
    Slots { first: u32, last: u32 },
    /// Nothing in the tracklist. Still carried with the release.
    NoSlots,
}

/// The catalog key naming what a file becomes. The single-slot case has its own
/// key because "slot 12" and "slots 1–11" are different sentences in most
/// languages, not one sentence with a range in it.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_becomes_key(becomes: BridgeFileBecomes) -> String {
    match becomes {
        BridgeFileBecomes::Slots { first, last } if first == last => "core.import.becomes.slot",
        BridgeFileBecomes::Slots { .. } => "core.import.becomes.slots",
        BridgeFileBecomes::NoSlots => "core.import.becomes.not_a_track",
    }
    .to_string()
}

/// The job a collapsed directory's files share. Mirror of bae-core's
/// `FileRowKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileRowKind {
    Document,
    Other,
}

/// The catalog key naming a collapsed directory's contents. Takes a `count`
/// argument in every language.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_file_row_kind_key(kind: BridgeFileRowKind) -> String {
    match kind {
        BridgeFileRowKind::Document => "core.import.files.documents",
        BridgeFileRowKind::Other => "core.import.files.other",
    }
    .to_string()
}

/// A directory whose files all do the same job, which the roles table shows as
/// one row instead of one row each. Mirror of bae-core's `CollapsedDirectory`.
///
/// Core decides which directories these are; a UI renders the group row in
/// place of the files whose `dir_prefix` equals this one, and lists nothing
/// else for them.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCollapsedDirectory {
    pub dir_prefix: String,
    pub kind: BridgeFileRowKind,
    pub count: u32,
    pub total_size: u64,
}

/// One file of a candidate, with the role in force for it and what that role
/// makes of it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateFile {
    pub file: BridgeFileInfo,
    pub role: BridgeFileRole,
    /// Which of the release's track slots this file backs. The one fact the
    /// role does not already say, and what makes the effect of a binding or an
    /// exclusion legible without reading the slot table below.
    pub becomes: BridgeFileBecomes,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make, which is every file the scan did
    /// not read as audio.
    pub alternatives: Vec<BridgeFileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when `alternatives` is empty.
    pub role_choice: Option<BridgeFileRoleChoice>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateFiles {
    /// Every file in the folder, each exactly once, in release-relative path
    /// order.
    pub files: Vec<BridgeCandidateFile>,
    /// e.g. "CUE+FLAC", "FLAC", "MP3" — computed by core from the probed codec.
    pub format_label: String,
    /// The directories the roles table shows as one row. Every file whose
    /// `dir_prefix` matches one of these is stood for by its group row.
    pub collapsed_directories: Vec<BridgeCollapsedDirectory>,
}

/// Phase-0 preparation step, mirroring bae-core's `PrepareStep`. The UI
/// localizes each variant via its catalog key (`bridge_prepare_step_key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePrepareStep {
    ParsingMetadata,
    WritingCoverArt,
    DiscoveringFiles,
    ValidatingTracks,
    SavingToDatabase,
}

impl BridgePrepareStep {
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::ParsingMetadata => "core.import.prepare.parsing_metadata",
            Self::WritingCoverArt => "core.import.prepare.writing_cover_art",
            Self::DiscoveringFiles => "core.import.prepare.discovering_files",
            Self::ValidatingTracks => "core.import.prepare.validating_tracks",
            Self::SavingToDatabase => "core.import.prepare.saving_to_database",
        }
    }
}

/// Running phase, mirroring bae-core's `ImportPhase`. Localized via
/// `bridge_import_phase_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportPhase {
    ReferencingFiles,
    MeasuringLoudness,
    Finalizing,
}

impl BridgeImportPhase {
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::ReferencingFiles => "core.import.phase.referencing_files",
            Self::MeasuringLoudness => "core.import.phase.measuring_loudness",
            Self::Finalizing => "core.import.phase.finalizing",
        }
    }
}

/// Which step of an import is in progress, mirroring bae-core's `ImportStep`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportStep {
    Preparing { step: BridgePrepareStep },
    Running { phase: BridgeImportPhase },
}

#[cfg(feature = "desktop")]
impl BridgeImportStep {
    pub(crate) fn from_core(s: bae_core::import::ImportStep) -> Self {
        use bae_core::import::{ImportPhase, ImportStep, PrepareStep};
        match s {
            ImportStep::Preparing(p) => BridgeImportStep::Preparing {
                step: match p {
                    PrepareStep::ParsingMetadata => BridgePrepareStep::ParsingMetadata,
                    PrepareStep::WritingCoverArt => BridgePrepareStep::WritingCoverArt,
                    PrepareStep::DiscoveringFiles => BridgePrepareStep::DiscoveringFiles,
                    PrepareStep::ValidatingTracks => BridgePrepareStep::ValidatingTracks,
                    PrepareStep::SavingToDatabase => BridgePrepareStep::SavingToDatabase,
                },
            },
            ImportStep::Running(phase) => BridgeImportStep::Running {
                phase: match phase {
                    ImportPhase::ReferencingFiles => BridgeImportPhase::ReferencingFiles,
                    ImportPhase::MeasuringLoudness => BridgeImportPhase::MeasuringLoudness,
                    ImportPhase::Finalizing => BridgeImportPhase::Finalizing,
                },
            },
        }
    }
}

/// Localization key for a prepare step — resolved by the UI against the `Core`
/// string table. One source for every platform.
#[uniffi::export]
pub fn bridge_prepare_step_key(step: BridgePrepareStep) -> String {
    step.loc_key().to_string()
}

/// Localization key for an import phase.
#[uniffi::export]
pub fn bridge_import_phase_key(phase: BridgeImportPhase) -> String {
    phase.loc_key().to_string()
}

/// One pressing under a release-group card. The card carries the album's title,
/// artist, and cover, so this keeps only the pressing-distinguishing fields the
/// row renders plus the id/source the import commit needs. Grouping happens in
/// core, so the group id isn't surfaced.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMetadataResult {
    pub source: BridgeMetadataSource,
    pub release_id: String,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibraryStatus {
    pub release_id: String,
    pub release_in_library: bool,
    pub album_in_library: bool,
    pub album_title: Option<String>,
    pub album_id: Option<String>,
}

/// Search query — one of the three search modes.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSearchQuery {
    General {
        artist: String,
        album: String,
        source: BridgeMetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        source: BridgeMetadataSource,
    },
    Barcode {
        barcode: String,
        source: BridgeMetadataSource,
    },
}

#[cfg(feature = "desktop")]
impl BridgeLibraryStatus {
    pub(crate) fn from_core(s: bae_core::db::LibraryStatus) -> Self {
        let bae_core::db::LibraryStatus {
            release_id,
            release_in_library,
            album_in_library,
            album_title,
            album_id,
        } = s;
        Self {
            release_id,
            release_in_library,
            album_in_library,
            album_title,
            album_id,
        }
    }
}

/// A signal the user has toggled off in the toolbar — excluded from
/// triangulation. The disc ID and barcode are singletons; a catalog candidate
/// is named by its value. Mirrors `bae_core::identify::ExcludedSignal`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeExcludedSignal {
    Disc,
    Barcode,
    Catalog { value: String },
}

impl BridgeExcludedSignal {
    #[cfg(feature = "desktop")]
    pub fn into_core(self) -> bae_core::identify::ExcludedSignal {
        use bae_core::identify::ExcludedSignal;
        match self {
            Self::Disc => ExcludedSignal::Disc,
            Self::Barcode => ExcludedSignal::Barcode,
            Self::Catalog { value } => ExcludedSignal::Catalog(value),
        }
    }
}

/// Where a signal value was harvested from — what a badge shows on hover
/// ("from Cover OCR", "from the folder name", …). Mirrors
/// `bae_core::signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalOrigin {
    DiscToc,
    CueSheet,
    Artwork,
    FolderName,
    Filename,
    TextFile,
}

#[cfg(feature = "desktop")]
impl BridgeSignalOrigin {
    pub(super) fn from_core(o: bae_core::signals::SignalOrigin) -> Self {
        use bae_core::signals::SignalOrigin;
        match o {
            SignalOrigin::DiscToc => BridgeSignalOrigin::DiscToc,
            SignalOrigin::CueSheet => BridgeSignalOrigin::CueSheet,
            SignalOrigin::Artwork => BridgeSignalOrigin::Artwork,
            SignalOrigin::FolderName => BridgeSignalOrigin::FolderName,
            SignalOrigin::Filename => BridgeSignalOrigin::Filename,
            SignalOrigin::TextFile => BridgeSignalOrigin::TextFile,
        }
    }
}

/// A signal value paired with its origin — a catalog candidate or a barcode
/// code. Mirrors `bae_core::signals::SourcedValue`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSourcedValue {
    pub value: String,
    pub origin: BridgeSignalOrigin,
}

#[cfg(feature = "desktop")]
impl BridgeSourcedValue {
    pub(super) fn from_core(s: bae_core::signals::SourcedValue) -> Self {
        let bae_core::signals::SourcedValue { value, origin } = s;
        Self {
            value,
            origin: BridgeSignalOrigin::from_core(origin),
        }
    }
}

/// Which kind of signal a toolbar badge represents. Mirrors
/// `bae_core::identify::SignalKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalKind {
    DiscId,
    Barcode,
    Catalog,
}

/// A toolbar signal's role in triangulation — identity signals find releases,
/// filter signals narrow them. Mirrors `bae_core::identify::SignalRole`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalRole {
    Identity,
    Filter,
}

/// Why a metadata lookup failed. Mirrors `bae_core::signals::LookupFailure`.
/// The locale never crosses the bridge: the UI resolves a localized line per
/// variant (`bridge_lookup_failure_key`) and renders `Provider`'s status as
/// the message argument. `Diagnostic` carries opaque, log-only detail — never
/// translated, never shown as primary copy.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLookupFailure {
    /// Transport/connection failure — no HTTP response.
    Network,
    /// An HTTP error response from the metadata provider, with its status
    /// code when one was observed.
    Provider { status: Option<u16> },
    /// The request timed out before a response arrived.
    Timeout,
    /// Artwork analysis failed before barcode/text extraction finished.
    ArtworkAnalysis,
    /// A local error (DB load, "not found", a compute task panic). `detail`
    /// is the opaque error chain — log-only, never translated.
    Diagnostic { detail: String },
}

#[cfg(feature = "desktop")]
impl BridgeLookupFailure {
    pub(super) fn from_core(f: bae_core::signals::LookupFailure) -> Self {
        use bae_core::signals::LookupFailure;
        match f {
            LookupFailure::Network => BridgeLookupFailure::Network,
            LookupFailure::Provider { status } => BridgeLookupFailure::Provider { status },
            LookupFailure::Timeout => BridgeLookupFailure::Timeout,
            LookupFailure::ArtworkAnalysis => BridgeLookupFailure::ArtworkAnalysis,
            LookupFailure::Diagnostic { detail } => BridgeLookupFailure::Diagnostic { detail },
        }
    }
}

/// Localization key for a lookup failure's user-facing line, or `None` for
/// `Diagnostic` (no translated copy — the UI shows a generic line plus the opaque
/// `detail`). `Provider` resolves to the status-bearing line when a code was
/// observed and a no-status fallback when not, so the UI never has to decide
/// which message a missing status takes. One source of these keys for every
/// platform.
#[uniffi::export]
pub fn bridge_lookup_failure_key(failure: BridgeLookupFailure) -> Option<String> {
    match failure {
        BridgeLookupFailure::Network => Some("core.lookup.failure.network".to_string()),
        BridgeLookupFailure::Provider { status: Some(_) } => {
            Some("core.lookup.failure.provider".to_string())
        }
        BridgeLookupFailure::Provider { status: None } => {
            Some("core.lookup.failure.provider_unknown".to_string())
        }
        BridgeLookupFailure::Timeout => Some("core.lookup.failure.timeout".to_string()),
        BridgeLookupFailure::ArtworkAnalysis => {
            Some("core.lookup.failure.artwork_analysis".to_string())
        }
        BridgeLookupFailure::Diagnostic { .. } => None,
    }
}

/// The live lookup/match state of one toolbar badge. Mirrors
/// `bae_core::identify::SignalState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalState {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Skipped,
    Failed { failure: BridgeLookupFailure },
    Confirms { count: u32 },
}

/// One badge in the signals toolbar — a pre-shaped row the UI renders without
/// deriving anything. Mirrors `bae_core::identify::ToolbarSignal`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeToolbarSignal {
    pub kind: BridgeSignalKind,
    pub role: BridgeSignalRole,
    pub value: Option<String>,
    pub origin: BridgeSignalOrigin,
    pub state: BridgeSignalState,
    pub excluded: bool,
}

/// The candidate's full signals toolbar — the ordered badge list. Mirrors a
/// `Vec<bae_core::identify::ToolbarSignal>`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignalsToolbar {
    pub signals: Vec<BridgeToolbarSignal>,
}

#[cfg(feature = "desktop")]
impl BridgeSignalState {
    pub(super) fn from_core(s: bae_core::identify::SignalState) -> Self {
        use bae_core::identify::SignalState;
        match s {
            SignalState::LookingUp => BridgeSignalState::LookingUp,
            SignalState::Found { count } => BridgeSignalState::Found { count },
            SignalState::NoMatch => BridgeSignalState::NoMatch,
            SignalState::Skipped => BridgeSignalState::Skipped,
            SignalState::Failed { failure } => BridgeSignalState::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            SignalState::Confirms { count } => BridgeSignalState::Confirms { count },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeToolbarSignal {
    pub(super) fn from_core(s: bae_core::identify::ToolbarSignal) -> Self {
        use bae_core::identify::{SignalKind, SignalRole, ToolbarSignal};
        let ToolbarSignal {
            kind,
            role,
            value,
            origin,
            state,
            excluded,
        } = s;
        BridgeToolbarSignal {
            kind: match kind {
                SignalKind::DiscId => BridgeSignalKind::DiscId,
                SignalKind::Barcode => BridgeSignalKind::Barcode,
                SignalKind::Catalog => BridgeSignalKind::Catalog,
            },
            role: match role {
                SignalRole::Identity => BridgeSignalRole::Identity,
                SignalRole::Filter => BridgeSignalRole::Filter,
            },
            value,
            origin: BridgeSignalOrigin::from_core(origin),
            state: BridgeSignalState::from_core(state),
            excluded,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSignalsToolbar {
    pub(crate) fn from_core(toolbar: Vec<bae_core::identify::ToolbarSignal>) -> Self {
        BridgeSignalsToolbar {
            signals: toolbar
                .into_iter()
                .map(BridgeToolbarSignal::from_core)
                .collect(),
        }
    }
}

/// Per-signal disc-ID progress inside `Triangulating`. Settled variants
/// (`Done`, `Skipped`, `Failed`) tell the UI this pipe is finished.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscidProgress {
    Computing,
    LookingUp,
    Done {
        n_results: u32,
    },
    /// No disc-ID artifacts (LOG/CUE) available for this candidate.
    Skipped,
    Failed {
        failure: BridgeLookupFailure,
    },
}

/// Per-signal barcode progress inside `Triangulating`. `LookingUp` carries
/// position + total so the UI can render "Looking up barcode 2 of 3."
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeProgress {
    Scanning,
    LookingUp {
        current: String,
        position: u32,
        total: u32,
    },
    Done {
        n_results: u32,
    },
    Failed {
        failure: BridgeLookupFailure,
    },
    /// No artwork to scan.
    Skipped,
}

/// An album's release group with the pressings the search/identify surfaced
/// for it, plus the display labels the group card renders. Mirrors
/// `bae_core::import::release_group::ReleaseGroup` — the grouping and label
/// formatting happen in core; the UI just iterates and renders.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseGroup {
    /// Stable card identity (shared group id, or the lone pressing's release
    /// id for an ungrouped result).
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// Representative cover for the card.
    pub cover_art: Option<BridgeRemoteCover>,
    /// Human-readable source name ("MusicBrainz" / "Discogs").
    pub source_label: String,
    /// Editorial URL for the group on its source (release-group on
    /// MusicBrainz, master on Discogs). `None` for an ungrouped result.
    pub group_url: Option<String>,
    /// Earliest and latest pressing year for the UI's "1992 – 2012" span; both
    /// `None` when no pressing carries a year. Pressing count is `pressings.len()`.
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<BridgeMetadataResult>,
}

/// The disc-ID signal. Mirrors `bae_core::signals::DiscIdSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscIdSignal {
    Computed {
        disc_id: String,
        track_count: u32,
    },
    Absent {
        track_count: u32,
    },
    Failed {
        failure: BridgeLookupFailure,
        track_count: u32,
    },
}

/// The barcode signal — the UPC/EAN code payloads with their origins. Mirrors
/// `bae_core::signals::BarcodeSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeSignal {
    Scanning {
        codes: Vec<BridgeSourcedValue>,
    },
    Settled {
        codes: Vec<BridgeSourcedValue>,
    },
    Failed {
        failure: BridgeLookupFailure,
        codes: Vec<BridgeSourcedValue>,
    },
    Absent,
}

/// The classified-text signal. Catalogs carry their origin (for the Refine
/// badges); free text doesn't (autocomplete only). Mirrors
/// `bae_core::signals::TextSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTextSignal {
    Scanning {
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
    Failed {
        failure: BridgeLookupFailure,
        catalogs: Vec<BridgeSourcedValue>,
        free_text: Vec<String>,
    },
}

/// The signals extracted from one candidate's files. Mirrors
/// `bae_core::signals::Signals`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSignals {
    pub disc_id: BridgeDiscIdSignal,
    pub barcode: BridgeBarcodeSignal,
    pub text: BridgeTextSignal,
}

/// Which signals produced or confirmed one result. Mirrors
/// `bae_core::identify::ResultProvenance` — drives the per-row signal badges.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeResultProvenance {
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub matches_catalog: bool,
}

/// Current identify-pipeline state for one candidate. One variant per state;
/// the UI reducer switches on the variant to render the right banner and
/// update the candidate.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIdentifyState {
    Idle,
    /// Both signals running in parallel. Per-signal progress lets the UI
    /// show side-by-side pipes ("Computing disc-id ✓ · Looking up barcode
    /// 2 of 3..."). The pipeline transitions to a terminal state once
    /// both pipes settle.
    Triangulating {
        discid: BridgeDiscidProgress,
        barcode: BridgeBarcodeProgress,
    },
    Found {
        /// The single release group every match shares, with its pressings —
        /// the UI renders it as one card with the pressings beneath.
        group: BridgeReleaseGroup,
        /// Library status per matched release, keyed by release id, so the
        /// UI looks up a row's status directly without re-indexing a flat
        /// list.
        library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        track_count: u32,
        /// Per-pressing provenance keyed by release id — the per-row signal
        /// badges, and which signal produced each match.
        provenance: std::collections::HashMap<String, BridgeResultProvenance>,
    },
    /// Signals disagreed: empty intersection or multi-group result. The UI
    /// presents the per-signal sections so the user can pick a section,
    /// ignore a signal, or fall back to manual search.
    Conflict {
        discid_results: Vec<BridgeMetadataResult>,
        /// Disc-id library statuses keyed by release id (see `Found`).
        discid_library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        barcode_results: Vec<BridgeMetadataResult>,
        /// Barcode library statuses keyed by release id (see `Found`).
        barcode_library_statuses: std::collections::HashMap<String, BridgeLibraryStatus>,
        /// The barcode value that produced `barcode_results`. `None` when
        /// the barcode side is empty. The conflict surface uses this in
        /// the section header so the user can correlate against the
        /// artwork.
        matched_barcode: Option<String>,
        track_count: u32,
    },
    NotFoundAnywhere,
    /// Nothing to look up — no disc-ID artifact and no barcode source. The UI
    /// offers manual search. Distinct from `NotFoundAnywhere` (signals ran,
    /// matched nothing).
    ManualOnly {
        track_count: u32,
    },
}

// ── Unified UI event system ─────────────────────────────────────────────
