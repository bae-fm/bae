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
    /// Whether this device imported these files, matched by content hash.
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
    /// `count` of its audio files can't be played.
    DamagedAudio {
        count: u32,
    },
    NoValidAudio,
}

impl BridgeInvalidReason {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::DamagedAudio { .. } => "core.import.invalid.damaged_audio",
            Self::NoValidAudio => "core.import.invalid.no_valid_audio",
        }
    }
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeInvalidReason = bae_core::import::InvalidReason,
    from_core: pub(crate) fn,
    variants: {
        DamagedAudio { count },
        NoValidAudio,
    },
}

/// The `Core` string table key for an invalid-candidate reason.
#[uniffi::export]
pub fn bridge_invalid_reason_key(reason: BridgeInvalidReason) -> String {
    reason.loc_key().to_string()
}

/// Why one audio file can't be played; the UI localizes it through
/// `bridge_audio_damage_key`. Mirrors
/// `bae_core::import::folder_scanner::AudioDamage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeAudioDamage {
    /// It holds no bytes.
    Empty,
    /// It holds no stream bae plays.
    Unreadable,
}

impl BridgeAudioDamage {
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::Empty => "core.import.audio_damage.empty",
            Self::Unreadable => "core.import.audio_damage.unreadable",
        }
    }
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeAudioDamage = bae_core::import::folder_scanner::AudioDamage,
    from_core: pub(crate) fn,
    variants: { Empty, Unreadable },
}

/// The `Core` string table key for why an audio file can't be played.
#[uniffi::export]
pub fn bridge_audio_damage_key(damage: BridgeAudioDamage) -> String {
    damage.loc_key().to_string()
}

/// What a skipped folder's tags call the release. Mirrors
/// `bae_core::import::folder_scanner::SkippedTags`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSkippedTags {
    pub album_title: String,
    pub artist: Option<String>,
}

mirror_struct! {
    #[cfg(feature = "desktop")]
    BridgeSkippedTags = bae_core::import::folder_scanner::SkippedTags,
    from_core: pub(crate) fn,
    fields: { album_title, artist },
}

/// A folder that looks like a release but failed validation, listed under
/// Skipped with its reason, as what the scan read of it.
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
    /// What its tags call the release; absent when they name no album, and
    /// the row names its folder instead.
    pub tags: Option<BridgeSkippedTags>,
    /// The image it leads with: the folder's own, when one decodes.
    pub cover: Option<BridgeCoverImageSource>,
}

/// A folder that failed validation, as the pane looking into it reads it.
/// Mirrors `bae_core::import::InvalidCandidateDetail`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInvalidCandidateDetail {
    pub candidate: BridgeInvalidCandidate,
    /// The images that decode, as the gallery shows them.
    pub images: Vec<BridgeMappingImage>,
    /// Every other file, each with what it is; the damaged audio among them
    /// is why the folder failed.
    pub files: Vec<BridgeMappingFile>,
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
    /// What identification is doing for the candidate; `None` when nothing
    /// is, and the pane shows its stored verdict.
    pub identification: Option<BridgeIdentificationInFlight>,
    pub import: Option<BridgeImportInFlight>,
    /// The typed search submitted for this candidate, as its sources answer.
    pub search: Option<BridgeCandidateSearch>,
}

/// What identification is doing for a candidate right now.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeIdentificationInFlight {
    /// On the identification queue, its run not started.
    Queued,
    /// The latest state a run published: in flight, or the answer being
    /// written. Never `Idle`.
    Run { state: BridgeIdentifyState },
}

/// The step a running import is at. How far through it is moves many times
/// a second, so it is the candidate's value in the import percents'
/// `KeyedValueStream`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImportInFlight {
    pub step: BridgeImportStep,
}

/// A running import, the release this device imported from the folder's
/// files, or its stored failure.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCandidateImportStatus {
    Importing { standing: BridgeImportStanding },
    Complete { release: BridgeImportedRelease },
    Error { error: BridgeError },
}

/// The release this device imported from the folder's files, or its stored
/// import failure.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeTriageImportStatus {
    Complete { release: BridgeImportedRelease },
    Error { error: BridgeError },
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
/// the tables alone; what is running for it is the row's `live`.
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

/// Where one Found row stands: its state, and why a row that needs the
/// person does, and how bae broke for a row in error. Mirrors
/// `bae_core::import::PendingStanding`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgePendingStanding {
    NotLookedUp,
    Identifying,
    NeedsYou {
        reason: BridgeNeedsYouReason,
    },
    Identified,
    Unmatched,
    LookupError,
    /// bae broke on its own side; the row states how.
    Error {
        failure: BridgeInternalFailure,
    },
    Importing,
    ImportError,
}

/// Why the lookup left a folder's answer to the person, with the numbers a
/// UI formats for its locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeNeedsYouReason {
    /// Several pressings could be the folder's: `count` of them.
    Matches { count: u32 },
    /// The one release found lists no tracks.
    NoTracklist,
    /// The folder's own files rule out every release found, of which there
    /// are `releases`.
    MediumMismatch {
        folder: BridgeMediumMismatch,
        releases: u32,
    },
    /// No catalog has the folder.
    NotFound,
    /// The folder has nothing to look it up by.
    NothingToLookUp,
}

/// What the folder's own files prove against the releases found. Mirrors
/// `bae_core::identify::MediumConflict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeMediumMismatch {
    /// The folder is a CD rip, and no release could be a CD.
    CdRip,
    /// The folder's audio is at a rate no CD plays at, and every release is
    /// a CD.
    NotCdAudio,
}

/// The one badge a Found row waiting on the person wears: what it says, and
/// how it reads. Mirrors `bae_core::import::PendingBadge` and its tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Record)]
pub struct BridgeRowBadge {
    pub says: BridgePendingBadge,
    pub tone: BridgeBadgeTone,
}

/// What the badge of a Found row waiting on the person says. Mirrors
/// `bae_core::import::PendingBadge`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgePendingBadge {
    /// The lookup left the answer to the person, for this reason.
    NeedsYou { reason: BridgeNeedsYouReason },
    /// A catalog could not answer the lookup, or hand over a release it found
    /// in full.
    LookupError,
    /// bae broke on its own side.
    Error,
    /// The last import failed, or the release cannot be worked on as it
    /// stands.
    ImportError,
}

/// How a badge reads at a glance. Mirrors `bae_core::import::BadgeTone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeBadgeTone {
    /// An answer is the person's to give.
    Attention,
    /// Something failed.
    Failure,
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
    /// The one selected row, when exactly one is.
    pub single: Option<BridgeSingleSelection>,
    pub offers: Vec<BridgeSelectionOffer>,
}

/// The one selected row of the import list. Mirrors
/// `bae_core::import::selection::SingleSelection`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeSingleSelection {
    Candidate {
        key: String,
    },
    /// A folder that failed validation.
    InvalidCandidate {
        key: String,
    },
}

mirror_enum! {
    #[cfg(feature = "desktop")]
    BridgeSingleSelection = bae_core::import::selection::SingleSelection,
    from_core: pub(crate) fn,
    variants: { Candidate { key }, InvalidCandidate { key } },
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
    let parts: Vec<bae_core::import::triage::SelectionPart> = members
        .into_iter()
        .map(|member| bae_core::import::triage::SelectionPart {
            actions: member
                .actions
                .into_iter()
                .map(BridgeCandidateAction::into_core)
                .collect(),
            count: 1,
        })
        .collect();
    bae_core::import::triage::selection_offers(&parts)
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
/// offers and the state it is in with it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCandidateLiveState {
    /// The lookup at work the row shows: only a row Identifying shows one.
    pub identifying: Option<BridgeIdentifyingStep>,
    /// Whether the release the run picked unattended is being read in full
    /// before its answer is stored, which the pane shows on that release.
    pub reading_pick: bool,
    /// Where the import that owns the candidate stands; `None` when none does.
    pub import: Option<BridgeImportStanding>,
    pub actions: Vec<BridgeCandidateAction>,
    /// Where the row stands among Found's states; `None` off Found.
    pub standing: Option<BridgePendingStanding>,
    /// The one badge the row wears in that state; `None` unless it waits on
    /// the person.
    pub badge: Option<BridgeRowBadge>,
}

/// How far the lookup of a row Identifying has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentifyingStep {
    /// Waiting for its run to start.
    Queued,
    /// Its run is going, or writing an answer that picks no release.
    Running,
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

/// A check of the found release against the folder that did not pass, with raw
/// operands the UI formats into the `bridge_folder_check_key` message.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFolderCheck {
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

/// The candidate's stored draft as its row shows it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTriageMetadataSummary {
    pub album_title: String,
    /// The draft's album credits, printed as one line.
    pub album_artists: String,
}

/// Where a candidate's draft was read from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: crate::types::BridgeMetadataRef,
    },
    FileMetadata,
}

/// What a candidate is linked to in the catalogs, independent of its draft.
/// Mirrors `bae_core::import::ReleaseLink`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseLink {
    /// The pressing picked for the folder.
    Pressing { link: BridgePressingLink },
    /// The album the folder's copy is a pressing of, its pressing unknown:
    /// the album in each catalog that files it, in catalog order.
    Album {
        albums: Vec<crate::types::BridgeMetadataRef>,
    },
}

#[cfg(feature = "desktop")]
impl BridgeReleaseLink {
    pub(crate) fn from_core(link: bae_core::import::ReleaseLink) -> Self {
        match link {
            bae_core::import::ReleaseLink::Pressing(pressing) => Self::Pressing {
                link: BridgePressingLink::from_core(pressing),
            },
            bae_core::import::ReleaseLink::Album(album) => Self::Album {
                albums: album
                    .albums()
                    .iter()
                    .cloned()
                    .map(crate::types::BridgeMetadataRef::from_core)
                    .collect(),
            },
        }
    }
}

/// The pressing a pick links a candidate to. Mirrors
/// `bae_core::import::PressingLink`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgePressingLink {
    /// The catalog's release the pick names first.
    pub record: crate::types::BridgeMetadataRef,
    /// The same pressing in the other catalogs, which the pick also claims.
    pub partners: Vec<crate::types::BridgeMetadataRef>,
}

#[cfg(feature = "desktop")]
impl BridgePressingLink {
    pub(crate) fn from_core(link: bae_core::import::PressingLink) -> Self {
        Self {
            record: crate::types::BridgeMetadataRef::from_core(link.record),
            partners: link
                .partners
                .into_iter()
                .map(crate::types::BridgeMetadataRef::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::PressingLink {
        bae_core::import::PressingLink {
            record: self.record.into_core(),
            partners: self
                .partners
                .into_iter()
                .map(crate::types::BridgeMetadataRef::into_core)
                .collect(),
        }
    }
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
    variants: {
        ExternalRelease {
            record: (crate::types::BridgeMetadataRef),
        },
        FileMetadata,
    },
}

/// What a row's text column says about its release.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTriageReading {
    /// No draft, from tags or anywhere: the row leads with its folder.
    Unidentified,
    /// A draft of a candidate linked to no release.
    Prefilled,
    /// A draft of a candidate linked to a catalog pressing, with every
    /// catalog that describes that pressing.
    Identified {
        records: Vec<crate::types::BridgeReleaseRecord>,
    },
    /// A draft of a candidate linked to an album, its pressing unknown, with
    /// every catalog's record of the album.
    IdentifiedAlbum {
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
        IdentifiedAlbum { records: (each crate::types::BridgeReleaseRecord) },
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
    /// What is running for the candidate right now, and the commands the row
    /// offers and the state it is in with it. The list delivers the row again
    /// when it changes.
    pub live: BridgeCandidateLiveState,
    pub metadata_summary: Option<BridgeTriageMetadataSummary>,
    /// The cover the row draws, even when its draft is otherwise blank.
    pub cover: Option<BridgeCoverImageSource>,
    pub import_status: Option<BridgeTriageImportStatus>,
    pub metadata_provenance: Option<BridgeMetadataProvenance>,
    pub reading: BridgeTriageReading,
    /// Whether the person has selected the row.
    pub selected: bool,
}

/// The release this device imported from a candidate's files.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportedRelease {
    pub release_id: String,
    pub album_id: String,
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
