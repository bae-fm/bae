use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFolderCandidate {
    /// The folders this release is read from, in play order; empty for one
    /// folder.
    pub parts: Vec<BridgeReleasePart>,
    pub folder_path: String,
    pub source_folder_name: String,
    /// The watched folder this candidate was scanned from
    /// (`BridgeWatchedFolder.path`).
    pub watched_folder_path: String,
    pub files: BridgeCandidateFiles,
    pub track_count: u32,
    /// Whether the person skipped this candidate.
    pub skipped: bool,
    /// Whether these files were already imported, matched by content hash.
    pub is_added: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFolderReleaseDecisionKey {
    pub watched_folder_path: String,
    pub relative_folder_path: String,
}

/// Why a folder failed validation; the UI localizes it through
/// `bridge_invalid_reason_key`.
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

/// The `Core` string table key for an invalid-candidate reason.
#[uniffi::export]
pub fn bridge_invalid_reason_key(reason: BridgeInvalidReason) -> String {
    reason.loc_key().to_string()
}

/// A folder that looks like a release but failed validation, listed under
/// Skipped with its reason.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInvalidCandidate {
    /// Its folder's path, or its grouping's.
    pub candidate_key: String,
    pub folder_path: String,
    pub source_folder_name: String,
    /// The watched folder this was scanned from (`BridgeWatchedFolder.path`).
    pub watched_folder_path: String,
    pub display_path: String,
    /// Whether this row is folders a grouping reads as one, which it offers
    /// to separate.
    pub separable: bool,
    pub reason: BridgeInvalidReason,
}

/// A change to what one key has in flight, or every key in flight.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateRuntimeChange {
    Updated {
        key: String,
        runtime: BridgeCandidateRuntimeSnapshot,
    },
    /// Nothing is running for the key any more.
    Removed { key: String },
    /// Every key in flight now; a key not listed is removed.
    Reset {
        runtimes: Vec<BridgeKeyedCandidateRuntime>,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeKeyedCandidateRuntime {
    pub key: String,
    pub runtime: BridgeCandidateRuntimeSnapshot,
}

/// What is happening for one candidate right now; what finished work left
/// behind is on its row.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateRuntimeSnapshot {
    pub identify_state: BridgeIdentifyState,
    pub import: Option<BridgeImportInFlight>,
    /// The typed search submitted for this candidate, as its sources answer.
    pub search: Option<BridgeCandidateSearch>,
}

/// How far a running import has got.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportInFlight {
    pub progress_percent: Option<u32>,
    pub step: BridgeImportStep,
}

/// Where a candidate's import stands: running, or how the last one ended.
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

/// How a candidate's last import ended.
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
    /// Whether this folder is on a network volume, which is also checked on a
    /// schedule because a watch misses changes made on the server.
    pub on_network_volume: bool,
}

/// The catalog key for a network folder's hover text; its argument is
/// [`bridge_network_folder_check_minutes`].
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_network_folder_watch_key() -> String {
    "core.import.folder.network_watch".to_string()
}

/// How often a watched folder on a network volume is checked, in minutes.
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

// ── Sidebar triage: mirrors of `bae_core::import::triage` ─────────────────

/// The sidebar's three tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageTab {
    Pending,
    Done,
    Skipped,
}

/// Which tab a row belongs to, and whether its last import failed, read from
/// the tables alone; what is running for it is its `BridgeCandidateLiveState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriagePlacement {
    Pending,
    /// The last import attempt failed; why is the row's import status.
    Failed,
    Done,
    Skipped,
}

/// Every command a candidate can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeCandidateAction {
    /// Import the candidate from its draft, wherever Pending places it.
    Import,
    Identify,
    CancelIdentification,
    CancelImport,
    RetryIdentification,
    ResetToFileMetadata,
    ClearMetadata,
    /// Read the selection's folders as one release.
    Combine,
    /// Read this release as the folders it is made of.
    Separate,
    Skip,
    Restore,
    /// Show the candidate's folders in the platform's file browser.
    RevealFolder,
}

/// What the tables say a row's commands are decided from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateActionBasis {
    pub actionable: bool,
    pub placement: BridgeTriagePlacement,
    /// Whether the draft shapes into a release an import can commit.
    pub draft_valid: bool,
    /// What the lookup stored for the candidate's current files came to, or
    /// none when none is stored.
    pub lookup: Option<BridgeStoredLookup>,
    pub separable: bool,
}

/// What a candidate's stored lookup came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStoredLookup {
    /// It found a release, found none, or left the choice to the person.
    Answered,
    /// A source it asked could not answer.
    Failed,
}

/// One selected candidate and the actions its live state offers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSelectionMember {
    pub candidate_key: String,
    pub actions: Vec<BridgeCandidateAction>,
}

/// One action a selection offers, how many members it applies to, and whether
/// it can run now.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSelectionOffer {
    pub action: BridgeCandidateAction,
    pub count: u64,
    pub enabled: bool,
}

/// What the import list's selection holds and can be told to do. Mirrors
/// `bae_core::import::selection::SelectionSummary`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSelectionSummary {
    pub count: u64,
    /// The one selected candidate, when exactly one is.
    pub single: Option<String>,
    pub offers: Vec<BridgeSelectionOffer>,
}

/// How a person changed the selection by pointing at rows. Mirrors
/// `bae_core::import::selection::SelectionChange`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSelectionChange {
    /// Select exactly these rows, and nothing else.
    Replace { keys: Vec<String> },
    /// Add these rows and take those out, leaving the rest as it is.
    Toggle {
        add: Vec<String>,
        remove: Vec<String>,
    },
    /// Add every row the list shows from `from` to `to`, both included.
    Extend { from: String, to: String },
}

/// How many of the candidates a bulk action runs on it has finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSelectionActionProgress {
    pub completed: u64,
    pub total: u64,
}

/// One selected candidate a bulk action could not run on, and why.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSelectionActionFailure {
    pub candidate_key: String,
    pub name: String,
    pub error: BridgeError,
}

/// What a selection of candidates can be told to do, in display order.
#[uniffi::export]
pub fn bridge_candidate_selection_offers(
    members: Vec<BridgeSelectionMember>,
) -> Vec<BridgeSelectionOffer> {
    let members: Vec<bae_core::import::triage::SelectionMember> = members
        .into_iter()
        .map(|member| bae_core::import::triage::SelectionMember {
            candidate_key: member.candidate_key,
            actions: member
                .actions
                .into_iter()
                .map(BridgeCandidateAction::into_core)
                .collect(),
        })
        .collect();
    bae_core::import::triage::selection_offers(&members)
        .into_iter()
        .map(BridgeSelectionOffer::from_core)
        .collect()
}

impl BridgeSelectionOffer {
    pub(crate) fn from_core(offer: bae_core::import::triage::SelectionOffer) -> Self {
        Self {
            action: BridgeCandidateAction::from_core(offer.action),
            count: offer.count,
            enabled: offer.enabled,
        }
    }
}

/// What is running for one candidate right now, and the commands its row
/// offers.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateLiveState {
    pub identification: Option<BridgeIdentificationStatus>,
    /// Where the import that owns the candidate stands; `None` when none does.
    pub import: Option<BridgeImportStanding>,
    pub actions: Vec<BridgeCandidateAction>,
}

/// Where the import that owns a candidate stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeImportStanding {
    /// Waiting for the worker to take it up.
    Queued,
    /// Taken up and not yet writing its release.
    Running,
    /// Writing its release; it can no longer be cancelled.
    Writing,
}

/// What identification is doing for a candidate right now.
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

/// A check of the found release against the folder that did not pass, with raw
/// operands the UI formats into the `bridge_folder_check_key` message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFolderCheck {
    TrackCountDisagrees {
        local: u32,
        source: u32,
    },
    SourceTracksUnknown,
    /// The folder's own files rule out every release found.
    MediumDisagrees {
        folder: BridgeMediumConflict,
    },
}

/// What the folder's own files prove against the releases found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMediumConflict {
    /// The folder is a CD rip, and no release could be a CD.
    CdRip,
    /// The folder's audio is at a rate no CD plays at, and every release is
    /// a CD. The rate is the audio's, read off its files; `None` when they
    /// are not at hand.
    NotCdAudio { sample_rate_hz: Option<u32> },
}

impl BridgeFolderCheck {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::TrackCountDisagrees { .. } => "core.import.triage.track_count_disagrees",
            Self::SourceTracksUnknown => "core.import.triage.source_tracks_unknown",
            Self::MediumDisagrees {
                folder: BridgeMediumConflict::CdRip,
            } => "core.import.triage.medium_disagrees.cd_rip",
            Self::MediumDisagrees {
                folder: BridgeMediumConflict::NotCdAudio { .. },
            } => "core.import.triage.medium_disagrees.not_cd_audio",
        }
    }
}

/// The `Core` string table key for a failed folder check's line.
#[uniffi::export]
pub fn bridge_folder_check_key(folder_check: &BridgeFolderCheck) -> String {
    folder_check.loc_key().to_string()
}

/// Which signal produced a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMatchedSignal {
    DiscId,
    Barcode,
    /// The search by the audio's ISRCs.
    Isrc,
    /// A catalog search for the candidate's album title.
    TitleSearch,
}

/// Which provider answered and what matched.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchEvidence {
    pub source: BridgeCatalog,
    /// `None` for a release the person picked themselves.
    pub signal: Option<BridgeMatchedSignal>,
}

/// The facts that differ between editions of one album, present only once the
/// pressing is settled.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchedPressing {
    pub year: Option<i32>,
    /// Each carrier the source lists, with its count.
    pub media: Vec<BridgeMediaCount>,
    /// The source's track count, when it listed one.
    pub track_count: Option<u32>,
}

/// The release identification matched for a row, kept on Skipped rows too.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMatchedRelease {
    /// The lead match's release id.
    pub release_id: String,
    /// The lead match's title, standing in for the album when several
    /// pressings matched.
    pub title: String,
    pub artist: Option<String>,
    pub pressing: Option<BridgeMatchedPressing>,
    /// The lead pressing's cover, in every size its catalog serves.
    pub cover: Option<BridgeRemoteImageSet>,
    pub evidence: BridgeMatchEvidence,
}

/// The candidate's stored draft as its row shows it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageMetadataSummary {
    pub album_title: String,
    pub album_artist_assignments: Vec<BridgeArtistAssignment>,
}

/// Where a candidate's draft was read from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: crate::types::BridgeMetadataRef,
        /// The same pressing in the other catalogs, which the pick also
        /// claims.
        partners: Vec<crate::types::BridgeMetadataRef>,
    },
    FileMetadata,
}

/// Who wrote the candidate's draft.
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

/// What a row's text column says about its release.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft read off the files' tags, or typed in.
    Prefilled,
    /// A draft read from a catalog's release, with every catalog that
    /// describes it.
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
    /// The folder's name, which is the row's title while it has no draft.
    pub folder_name: String,
    /// The watched folder this was scanned from (`BridgeWatchedFolder.path`).
    pub watched_folder_path: String,
    pub display_path: String,
    pub actionable: bool,
    pub placement: BridgeTriagePlacement,
    /// What the row's commands are decided from, handed back with its
    /// live-state subscription.
    pub action_basis: BridgeCandidateActionBasis,
    pub matched: Option<BridgeMatchedRelease>,
    pub metadata_summary: Option<BridgeTriageMetadataSummary>,
    /// The cover the row draws, even when its draft is otherwise blank.
    pub cover: Option<BridgeCoverImageSource>,
    pub import_status: Option<BridgeTriageImportStatus>,
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
    pub reading: BridgeTriageReading,
    /// Whether the person has selected the row.
    pub selected: bool,
}

/// A Done row: the library release the candidate became, as the library has it
/// now.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportedRow {
    /// The candidate's folder path — the key every other import call takes.
    pub candidate_key: String,
    pub display_path: String,
    /// Handed back with the row's live-state subscription; an import that just
    /// wrote the release can still own the candidate for a moment.
    pub action_basis: BridgeCandidateActionBasis,
    pub release: BridgeImportedReleaseSummary,
    /// Whether the person has selected the row.
    pub selected: bool,
}

/// The library release a Done row became.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportedReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    /// Empty when the tags named no title.
    pub title: String,
    /// The album's credited artists, joined.
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub cover: Option<crate::types::BridgeImageRef>,
    /// Every catalog's description of the release, in catalog order.
    pub records: Vec<crate::types::BridgeReleaseRecord>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageGroup {
    pub key: BridgeFolderReleaseDecisionKey,
    pub name: String,
    /// Whether the rows under this header are one folder read as several
    /// releases, which the header offers to read as one.
    pub combinable: bool,
}

/// How many rows each tab holds, unfiltered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeTriageTabCounts {
    pub pending: u32,
    pub done: u32,
    pub skipped: u32,
}
