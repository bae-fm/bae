use super::*;

mod error;
mod identify;
mod library;
mod metadata_edit;
mod pressing;

pub use error::AutomationError;
pub use identify::*;
pub use library::*;
pub use metadata_edit::*;
pub use pressing::*;

#[derive(Debug, Clone, Serialize)]
pub struct AutomationConfig {
    pub library_id: String,
    pub library_name: String,
    pub library_path: String,
    pub mcp: AutomationMcpConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationMcpConfig {
    pub enabled: bool,
    pub port: u16,
}

impl From<McpConfig> for AutomationMcpConfig {
    fn from(value: McpConfig) -> Self {
        Self { enabled: value.enabled, port: value.port }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationWatchedFolder {
    /// The hex of the folder's path bytes (`coven::path_bytes`): what
    /// `watched_folder_remove` takes back, which names a folder whose name
    /// isn't UTF-8 as no text can.
    pub key: String,
    /// The folder's path, shown as text.
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode")]
pub enum ScanWait {
    NoWait,
    UntilFinished { timeout_ms: u64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationScanResult {
    pub watched_folders: Vec<AutomationWatchedFolder>,
    pub candidates: Vec<AutomationCandidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCandidate {
    Valid {
        #[serde(flatten)]
        common: AutomationCandidateCommon,
        track_count: u32,
        source_audio: Option<AutomationSourceAudioSummary>,
        content_hash: String,
        /// What identification and import have recorded for this candidate.
        runtime: AutomationCandidateRuntime,
        /// The pressing this candidate is linked to; `None` while nothing is
        /// picked, for a folder read as its own tags, and for a candidate
        /// linked to an album.
        picked_pressing: Option<AutomationPressingLink>,
        /// What identified the picked release, with the file each piece was
        /// read off.
        file_evidence: Vec<AutomationFileEvidence>,
        /// The metadata this candidate will commit with; `None` while nothing
        /// is picked.
        edit: Option<AutomationReleaseUserEdit>,
        /// The last import of this candidate that failed.
        failure: Option<AutomationImportFailure>,
    },
    Invalid {
        #[serde(flatten)]
        common: AutomationCandidateCommon,
        invalid_reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationSourceAudioLayout {
    File,
    Cue,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationSourceAudioDescriptor {
    pub layout: AutomationSourceAudioLayout,
    pub format: AutomationAudioFormat,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSourceAudioSummary {
    Uniform { descriptor: AutomationSourceAudioDescriptor },
    Mixed { differences: Vec<AutomationSourceAudioDifference> },
}

/// One fact a release's files disagree on, and its values, each once.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "fact")]
pub enum AutomationSourceAudioDifference {
    Layout { layouts: Vec<AutomationSourceAudioLayout> },
    Codec { codecs: Vec<String> },
    SampleRate { sample_rates_hz: Vec<i64> },
    BitDepth { bits_per_sample: Vec<i64> },
    Channels { channels: Vec<i64> },
}

impl AutomationCandidate {
    pub(super) fn common(&self) -> &AutomationCandidateCommon {
        match self {
            Self::Valid { common, .. } | Self::Invalid { common, .. } => common,
        }
    }

    pub(super) fn key(&self) -> &str {
        &self.common().key
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationCandidateCommon {
    /// The hex of the candidate's key's bytes, which every candidate command
    /// takes as `candidate_key`.
    pub key: String,
    /// The folders it is read from, shown as text.
    pub source_folders: Vec<String>,
    pub name: String,
    /// The watched folder it was scanned from, shown as text.
    pub watched_folder_path: String,
    pub skipped: bool,
    pub is_added: bool,
}

/// A candidate's identify state, signals and import status.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCandidateRuntime {
    pub identify_state: AutomationIdentifyState,
    pub signals: Option<AutomationSignals>,
    pub import_status: Option<AutomationImportStatus>,
}

/// A candidate's last failed import; `failed_at` is RFC 3339.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationImportFailure {
    pub error: String,
    pub failed_at: String,
}

/// Mirrors bae-catalogs' `LookupFailure`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationLookupFailure {
    Network,
    Provider { status: Option<u16> },
    Timeout,
}

/// Mirrors bae-catalogs' `InternalFailure`: how bae broke on its own
/// side.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationInternalFailure {
    pub detail: String,
}

/// Mirrors bae-library's `identify::signals::DiscIdSignal`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscIdSignal {
    /// Every disc ID computed, in reading order.
    Computed {
        disc_ids: Vec<String>,
    },
    Absent,
    /// A CUE over audio sampled at a rate no CD plays at.
    NotCdAudio,
    Failed {
        failure: AutomationInternalFailure,
    },
}

/// Mirrors bae-catalogs' `CdProof`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCdProof {
    RipLog,
    AccurateRipReport,
    RipperSheet,
}

/// Mirrors bae-catalogs' `StoreMarker`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStoreMarker {
    ITunesPurchase,
    Bandcamp,
}

/// Mirrors bae-catalogs' `DownloadProof`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDownloadProof {
    Store { marker: AutomationStoreMarker, file: String },
    DeliverySet,
}

/// Mirrors bae-catalogs' `AudioSource`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationAudioSource {
    CdRip { proof: AutomationCdProof, file: Option<String> },
    Download { proof: AutomationDownloadProof },
}

/// Mirrors bae-catalogs' `AudioOrigin`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationAudioOrigin {
    pub source: Option<AutomationAudioSource>,
    pub not_cd_rate: Option<u32>,
}

/// Mirrors bae-library's `identify::signals::BarcodeSignal`, each code once.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeSignal {
    Scanning { codes: Vec<String> },
    Settled { codes: Vec<String> },
    Failed { failure: AutomationInternalFailure, codes: Vec<String> },
    Absent,
}

/// Mirrors bae-library's `identify::signals::TextSignal`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTextSignal {
    Scanning { catalogs: Vec<String>, free_text: Vec<String> },
    Settled { catalogs: Vec<String>, free_text: Vec<String> },
    Failed { failure: AutomationInternalFailure, catalogs: Vec<String>, free_text: Vec<String> },
}

/// Mirrors bae-library's `identify::signals::Signals`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignals {
    pub origin: AutomationAudioOrigin,
    pub disc_id: AutomationDiscIdSignal,
    pub barcode: AutomationBarcodeSignal,
    pub text: AutomationTextSignal,
    pub isrcs: Vec<String>,
    pub track_titles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchQuery {
    General { artist: String, album: String, source: AutomationLookupCatalog },
    CatalogNumber { catalog_number: String, source: AutomationLookupCatalog },
    Barcode { barcode: String, source: AutomationLookupCatalog },
}

/// A catalog that answers searches: the one a search is asked of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationLookupCatalog {
    MusicBrainz,
    Discogs,
}

/// A service that publishes descriptions of releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCatalog {
    MusicBrainz,
    Discogs,
    AllMusic,
    AppleMusic,
    Bandcamp,
    Deezer,
    Genius,
    MusikSammler,
    RateYourMusic,
    Spotify,
    Wikidata,
}

impl From<AutomationCatalog> for Catalog {
    fn from(value: AutomationCatalog) -> Self {
        match value {
            AutomationCatalog::MusicBrainz => Catalog::MusicBrainz,
            AutomationCatalog::Discogs => Catalog::Discogs,
            AutomationCatalog::AllMusic => Catalog::AllMusic,
            AutomationCatalog::AppleMusic => Catalog::AppleMusic,
            AutomationCatalog::Bandcamp => Catalog::Bandcamp,
            AutomationCatalog::Deezer => Catalog::Deezer,
            AutomationCatalog::Genius => Catalog::Genius,
            AutomationCatalog::MusikSammler => Catalog::MusikSammler,
            AutomationCatalog::RateYourMusic => Catalog::RateYourMusic,
            AutomationCatalog::Spotify => Catalog::Spotify,
            AutomationCatalog::Wikidata => Catalog::Wikidata,
        }
    }
}

impl From<Catalog> for AutomationCatalog {
    fn from(value: Catalog) -> Self {
        match value {
            Catalog::MusicBrainz => AutomationCatalog::MusicBrainz,
            Catalog::Discogs => AutomationCatalog::Discogs,
            Catalog::AllMusic => AutomationCatalog::AllMusic,
            Catalog::AppleMusic => AutomationCatalog::AppleMusic,
            Catalog::Bandcamp => AutomationCatalog::Bandcamp,
            Catalog::Deezer => AutomationCatalog::Deezer,
            Catalog::Genius => AutomationCatalog::Genius,
            Catalog::MusikSammler => AutomationCatalog::MusikSammler,
            Catalog::RateYourMusic => AutomationCatalog::RateYourMusic,
            Catalog::Spotify => AutomationCatalog::Spotify,
            Catalog::Wikidata => AutomationCatalog::Wikidata,
        }
    }
}

/// A known pressing or album identity, with its catalog page built by core.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationReleaseRecord {
    Pressing { release: AutomationMetadataRef, album_key: Option<String>, url: String },
    Album { album: AutomationMetadataRef, url: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationSearchResults {
    pub groups: Vec<AutomationReleaseGroup>,
    pub statuses: Vec<AutomationLibraryStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseGroup {
    pub id: String,
    /// The album's title as each of its records states it, each distinct
    /// title once.
    pub titles: Vec<String>,
    /// The album's artist as each of its records states it, each distinct
    /// credit once.
    pub artists: Vec<String>,
    /// Every label the album's pressings name, each once.
    pub labels: Vec<String>,
    pub cover_art: Option<AutomationRemoteCover>,
    /// Every source carrying this group, in the order its rows name them.
    pub sources: Vec<AutomationReleaseGroupSource>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    /// The card's rows, album by album: one section with no heading, or one
    /// per album where the card holds two albums of one catalog.
    pub sections: Vec<AutomationPressingSection>,
}

/// One album's rows on a card.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationPressingSection {
    /// The album the rows are pressings of, where the card splits its rows by
    /// album.
    pub album: Option<AutomationAlbumHeading>,
    /// The rows offered.
    pub pressings: Vec<AutomationPressing>,
    /// The rows of this album a run's agreement set aside.
    pub narrowed_out: Vec<AutomationPressing>,
}

/// An album heading a card's section: its own title, and its page on its
/// catalog.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationAlbumHeading {
    pub title: String,
    pub source: AutomationReleaseGroupSource,
}

/// One source carrying a group, and its page for it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseGroupSource {
    pub source: AutomationCatalog,
    pub group_url: Option<String>,
    /// Whether what this album is on the other catalog could not be read.
    pub album_links_unread: bool,
}

/// One physical pressing, on every source that lists it, picked whole: `pick`
/// can be handed straight back as a candidate's metadata provenance.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationPressing {
    pub releases: Vec<AutomationMetadataResult>,
    pub pick: AutomationPressingLink,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationMetadataResult {
    pub source: AutomationLookupCatalog,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub labels: Vec<bae_types::pressing::ReleaseLabel>,
    /// What the record says the pressing is.
    pub facts: bae_types::pressing::PressingFacts,
    /// Every barcode this source prints for the pressing, in its order;
    /// empty where it prints none.
    pub barcodes: Vec<String>,
    pub cover_art: Option<AutomationRemoteCover>,
    pub source_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationLibraryStatus {
    pub release_id: String,
    /// Every library album holding the release or another release of its
    /// album, those holding the release first.
    pub albums: Vec<AutomationLibraryAlbumMatch>,
}

/// One library album a release's records name.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationLibraryAlbumMatch {
    pub album_id: String,
    pub album_title: String,
    /// It holds a copy of the release itself.
    pub holds_release: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationRemoteCover {
    pub image: AutomationRemoteImageSet,
    pub label: AutomationCoverLabel,
    pub source: AutomationCatalog,
    pub standing: AutomationCoverStanding,
}

/// What a catalog offers a cover as.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCoverLabel {
    /// The catalog's lead image of one entity, or (`release_group`) the front
    /// image of the release group the album belongs to.
    Lead { release_group: bool },
    /// One image of the catalog's own gallery of an entity, numbered from 1.
    Gallery {
        reference: Option<String>,
        position: u32,
        types: Vec<AutomationCoverImageType>,
        comment: String,
    },
}

/// What an image shows, by the Cover Art Archive's vocabulary.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCoverImageType {
    Front,
    Back,
    Booklet,
    Medium,
    Obi,
    Spine,
    Track,
    Liner,
    Sticker,
    Poster,
    Watermark,
    Raw,
    Matrix,
    Top,
    Bottom,
    Tray,
    Other,
}

/// Whether a catalog stated the cover is there, or it is an address nothing
/// said anything about.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCoverStanding {
    Stated,
    Unstated,
}

/// A catalog image: the original, and the downscaled copies the catalog
/// serves of it, each no larger than `max_edge` pixels on its longer side.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationRemoteImageSet {
    pub url: String,
    pub downscaled: Vec<AutomationDownscaledCopy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationDownscaledCopy {
    pub url: String,
    pub max_edge: u32,
}

/// One signal that identified the picked release, and the file it was read
/// off. It explains the pick and decides nothing.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationFileEvidence {
    pub signal: AutomationEvidenceSignal,
    pub value: String,
    /// The candidate-relative path of the file.
    pub file_id: String,
}

/// A signal that can name the file it was read off.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEvidenceSignal {
    Barcode,
    DiscId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCoverSelection {
    Remote { image: AutomationRemoteImageSet, source: AutomationCatalog },
    Local { path: String },
}

/// Mirrors bae-library's `import::ImportStart`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationImportStarted {
    /// The import is queued under this id.
    Queued { import_id: String },
    /// The release the candidate's run picked is still being read; the
    /// candidate imports once that read stores it.
    WhenPickStored,
}

/// Mirrors bae-library's `import::PrepareStep`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPrepareStep {
    Queued,
    ValidatingSourceFiles,
    FetchingArtistPictures,
}

/// Mirrors bae-library's `import::ImportPhase`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationImportPhase {
    ReadingFiles,
    MeasuringLoudness,
    Finalizing,
}

/// Mirrors bae-library's `import::CandidateImportStatus`, with the running
/// attempt's progress.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationImportStatus {
    Importing {
        progress_percent: Option<u32>,
        step: Option<AutomationImportStep>,
    },
    /// The release this device imported from the candidate's files.
    Complete {
        release: AutomationImportedRelease,
    },
    Error {
        error: String,
    },
}

/// The release this device imported from a candidate's files.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationImportedRelease {
    pub release_id: String,
    pub album_id: String,
}

/// Mirrors bae-library's `import::ImportStep`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationImportStep {
    Preparing { step: AutomationPrepareStep },
    Running { phase: AutomationImportPhase },
}

#[derive(Debug, Clone, Serialize)]
pub struct EmptyResponse {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PathInput {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct WatchedFolderKeyInput {
    /// A watched folder's `key`, as the watched folder list gives it.
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateKeyInput {
    /// A candidate's `key`, as the candidate list gives it.
    pub candidate_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateSkipSetInput {
    pub candidate_key: String,
    pub skipped: bool,
}

/// The pressing a pick links a candidate to.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationPressingLink {
    /// The catalog's release the draft is read from.
    pub record: AutomationLookupRef,
    /// The other catalogs' releases the picked pressing paired with, which
    /// the pick claims too.
    pub partners: Vec<AutomationLookupRef>,
}

/// A release on a catalog that answers lookups: one bae can fetch and pick.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationLookupRef {
    pub catalog: AutomationLookupCatalog,
    pub key: String,
}

/// One catalog's key for an entity, whose kind is stated by its containing field.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationMetadataRef {
    pub catalog: AutomationCatalog,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateReleaseInput {
    pub candidate_key: String,
    pub link: AutomationPressingLink,
}

/// One text field of a candidate's metadata form. Years are text because the
/// form is text; the commit parses them. The labels are set with
/// `import_candidate_labels_set`, the barcodes with
/// `import_candidate_barcodes_set`, and what the pressing is is chosen with
/// `import_candidate_pressing_fact_set`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
}

/// One label row of a candidate's metadata form, as typed; empty is unset.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationLabelRow {
    pub name: String,
    pub catalog_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateLabelsInput {
    pub candidate_key: String,
    /// Every label row, in order.
    pub labels: Vec<AutomationLabelRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateAlbumInput {
    pub candidate_key: String,
    /// The library album the candidate joins, over the one its records
    /// name; absent to leave it to its records.
    pub album_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateBarcodesInput {
    pub candidate_key: String,
    /// Every barcode row, in order, as typed; a blank row is unset.
    pub barcodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateEditFieldInput {
    pub candidate_key: String,
    pub field: AutomationCandidateEditField,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateCoverInput {
    pub candidate_key: String,
    pub cover: AutomationCoverSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseIdInput {
    pub release_id: String,
}

/// Which storage transition to run, named as the bridge and a release's
/// reported transitions name them.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationStorageAction {
    /// Upload a local release's audio from this device's originals.
    Upload,
    /// Keep a cloud release offline on this device.
    Pin,
    /// Stop keeping a cloud release offline. Its bytes stay in the cloud.
    Unpin,
    /// Queue an uploaded release to be made local: its audio is copied into a
    /// folder of its own named for the release below `destination_dir`, and
    /// plays from there once the copy is done.
    DetachAudio { destination_dir: String },
    /// Cancel whichever transition is in flight; nothing running is no error.
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseStorageActionInput {
    pub release_id: String,
    pub action: AutomationStorageAction,
}

/// What a storage action left behind.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationStorageActionOutcome {
    /// The uploads are queued; poll the release to see them land.
    UploadQueued {
        release_id: String,
    },
    /// The pin is queued; the files are not offline yet.
    PinQueued {
        release_id: String,
    },
    Unpinned {
        release_id: String,
    },
    /// The make-local is queued; the audio is not local yet.
    MakeLocalQueued {
        release_id: String,
    },
    /// Whatever was in flight was told to stop, if anything was.
    Cancelled {
        release_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseExportInput {
    pub release_id: String,
    pub target_dir: String,
}

/// An export was queued; poll `output_status` for progress.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseExport {
    pub release_id: String,
}

/// A queued export's state, mirroring bae-library's `OutputState`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationOutputState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// Mirrors bae-library's `OutputKind`; a save carries its preset's name.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationOutputKind {
    Export,
    Save { preset_name: String },
}

/// A queued release as the library holds it now.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationQueuedRelease {
    pub title: String,
    pub file_count: u64,
    pub total_size: u64,
}

/// One queued release output in the `output_status` snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationOutputOp {
    pub release_id: String,
    pub target_dir: String,
    /// `None` once the library no longer has the release.
    pub release: Option<AutomationQueuedRelease>,
    pub created_at: i64,
    pub state: AutomationOutputState,
    pub kind: AutomationOutputKind,
}

/// Per-state counts for the export queue.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationOutputProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The export queue snapshot returned by `output_status`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationOutputSnapshot {
    pub outputs: Vec<AutomationOutputOp>,
    pub total: AutomationOutputProgress,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseReidentifyInput {
    pub release_id: String,
    /// The catalog release it is, with the other catalogs' releases the pick
    /// pairs with it.
    pub pressing: AutomationPressingLink,
    /// The library album it files under, over the one its records name,
    /// which keeps its own title and artists; absent to leave it to its
    /// records.
    pub album_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseAlbumInput {
    pub release_id: String,
    /// The library album the release moves into.
    pub album_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseMetadataUpdateInput {
    pub release_id: String,
    pub edit: AutomationReleaseUserEdit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LibrarySearchInput {
    pub query: String,
}
