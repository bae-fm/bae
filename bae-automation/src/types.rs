use super::*;

mod error;
mod field_provenance;
mod identify;
mod library;
mod metadata_edit;

pub use error::AutomationError;
pub use field_provenance::*;
pub use identify::*;
pub use library::*;
pub use metadata_edit::*;

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
        Self {
            enabled: value.enabled,
            port: value.port,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationWatchedFolder {
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
        /// What the identify and import pipelines have recorded against this
        /// candidate. Every scanned folder carries one — idle until something
        /// runs — because the import service keeps it alongside the candidate.
        runtime: AutomationCandidateRuntime,
        /// The release this candidate is picked as, described by the documents
        /// the pick archived. `None` while nothing is picked, and for a folder
        /// read as its own tags.
        picked_release: Option<AutomationReleaseDetail>,
        /// What identified the picked release, pinned to the candidate file
        /// each piece of evidence was read off — the same chips the pane puts
        /// on that image's tile or that file's row.
        file_evidence: Vec<AutomationFileEvidence>,
        /// The metadata this candidate will commit with: the pick's own values
        /// with whatever has been typed over them. `None` while nothing is
        /// picked.
        edit: Option<AutomationReleaseUserEdit>,
        /// One entry per album-level field: where its value came from, and
        /// what every catalog claiming the pick says about it.
        field_provenance: Vec<AutomationFieldProvenance>,
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
    Uniform {
        descriptor: AutomationSourceAudioDescriptor,
    },
    Mixed {
        descriptors: Vec<AutomationSourceAudioDescriptor>,
    },
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
    pub key: String,
    pub source_folders: Vec<String>,
    pub name: String,
    pub watched_folder_path: String,
    pub skipped: bool,
    pub is_added: bool,
}

/// What the import tab shows for one candidate beyond its folder: the run in
/// flight or the answer stored for it, that state's signals toolbar, the
/// signals extraction settled on, and where its import stands.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCandidateRuntime {
    pub identify_state: AutomationIdentifyState,
    pub toolbar: Vec<AutomationToolbarSignal>,
    pub signals: Option<AutomationSignals>,
    pub import_status: Option<AutomationImportStatus>,
}

/// An import that failed, as the candidate still records it after a relaunch.
/// `failed_at` is RFC 3339.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationImportFailure {
    pub error: String,
    pub failed_at: String,
}

/// Mirrors bae-core's `signals::LookupFailure`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationLookupFailure {
    Network,
    Provider { status: Option<u16> },
    Timeout,
    ArtworkAnalysis,
    Diagnostic { detail: String },
}

/// Mirrors bae-core's `signals::SignalOrigin`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationSignalOrigin {
    DiscToc,
    CueSheet,
    Artwork,
    FolderName,
    Filename,
    TextFile,
}

/// Mirrors bae-core's `signals::ImageRegion`: where on its image a value was
/// read, as fractions of the image's size with the origin at the top-left.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct AutomationImageRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Mirrors bae-core's `signals::SourcedValue` — one sighting of a value.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSourcedValue {
    pub value: String,
    pub origin: AutomationSignalOrigin,
    /// The candidate-relative path of the file the value was read off, where
    /// the origin is a file.
    pub origin_path: Option<String>,
    /// Where on that image it was read, where the detector said.
    pub region: Option<AutomationImageRegion>,
}

/// Mirrors bae-core's `signals::DiscIdSignal`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscIdSignal {
    Computed {
        disc_id: String,
        track_count: u32,
    },
    Absent {
        track_count: u32,
    },
    Failed {
        failure: AutomationLookupFailure,
        track_count: u32,
    },
}

/// Mirrors bae-core's `signals::BarcodeSignal`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeSignal {
    Scanning {
        codes: Vec<AutomationSourcedValue>,
    },
    Settled {
        codes: Vec<AutomationSourcedValue>,
    },
    Failed {
        failure: AutomationLookupFailure,
        codes: Vec<AutomationSourcedValue>,
    },
    Absent,
}

/// Mirrors bae-core's `signals::TextSignal`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTextSignal {
    Scanning {
        catalogs: Vec<AutomationSourcedValue>,
        free_text: Vec<String>,
    },
    Settled {
        catalogs: Vec<AutomationSourcedValue>,
        free_text: Vec<String>,
    },
    Failed {
        failure: AutomationLookupFailure,
        catalogs: Vec<AutomationSourcedValue>,
        free_text: Vec<String>,
    },
}

/// Mirrors bae-core's `signals::Signals`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignals {
    pub disc_id: AutomationDiscIdSignal,
    pub barcode: AutomationBarcodeSignal,
    pub text: AutomationTextSignal,
}

/// Mirrors bae-core's `identify::SignalKind`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationSignalKind {
    DiscId,
    Barcode,
    Catalog,
}

/// Mirrors bae-core's `identify::SignalState`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSignalState {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Skipped,
    Failed { failure: AutomationLookupFailure },
}

/// Mirrors bae-core's `identify::SignalOption` — one of the values a signal
/// could take, for the signals that offer a choice.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSignalOption {
    pub value: String,
    pub origin: AutomationSignalOrigin,
    pub chosen: bool,
}

/// Mirrors bae-core's `identify::ToolbarSignal`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationToolbarSignal {
    pub kind: AutomationSignalKind,
    pub value: Option<String>,
    pub origin: AutomationSignalOrigin,
    pub state: AutomationSignalState,
    pub excluded: bool,
    pub options: Vec<AutomationSignalOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchQuery {
    General {
        artist: String,
        album: String,
        source: AutomationCatalog,
    },
    CatalogNumber {
        catalog_number: String,
        source: AutomationCatalog,
    },
    Barcode {
        barcode: String,
        source: AutomationCatalog,
    },
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

/// One catalog's description of a release: which catalog, its key for the
/// release, and the page it publishes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleaseRecord {
    pub catalog: AutomationCatalog,
    pub key: String,
    /// The catalog's page for this release, built by core.
    pub url: String,
    /// True for the one record the draft's facts were read from.
    pub reads_draft: bool,
}

/// Which name a mark is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationMarkKind {
    DiscId,
    Barcode,
    CatalogNumber,
}

/// One name read off the object itself: the kind, the value, and every surface
/// it was read from, each named once.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseMark {
    pub kind: AutomationMarkKind,
    pub value: String,
    pub origins: Vec<AutomationSignalOrigin>,
}

/// Where a release's verification came from. Mirrors
/// `bae_core::import::VerificationSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationVerificationSource {
    Log,
}

/// One track's agreement count from each rip database, and the CRC of the
/// audio those counts are about. Mirrors
/// `bae_core::import::TrackVerification`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AutomationTrackVerification {
    pub number: u32,
    pub accuraterip_confidence: Option<u32>,
    pub ctdb_confidence: Option<u32>,
    pub crc: Option<u32>,
}

/// What the rip databases said about a release's audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AutomationVerification {
    pub source: AutomationVerificationSource,
    /// The weakest track's best database — `None` when a track no database
    /// confirmed leaves the release unverified.
    pub matched_copies: Option<u32>,
    pub tracks: Vec<AutomationTrackVerification>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationReleaseReseed {
    ExternalRelease {
        source: AutomationCatalog,
        release_id: String,
        /// The other sources' releases the picked pressing paired with.
        partners: Vec<AutomationMetadataRef>,
    },
    FileTags,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationSearchResults {
    pub groups: Vec<AutomationReleaseGroup>,
    pub statuses: Vec<AutomationLibraryStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseGroup {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    /// The label the album's pressings name, where they name one.
    pub label: Option<String>,
    pub cover_art: Option<AutomationRemoteCover>,
    /// Every source carrying this group, in the order its rows name them.
    pub sources: Vec<AutomationReleaseGroupSource>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<AutomationPressing>,
}

/// One source carrying a group, and its editorial page for it.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseGroupSource {
    pub source: AutomationCatalog,
    pub group_url: Option<String>,
}

/// One physical pressing, on every source that lists it.
///
/// A row is picked whole. `releases` is what the row shows, its extra entries
/// naming the other sources carrying the same pressing rather than offering
/// separate picks; `pick` is what picking the row means and can be handed
/// straight back as a candidate's metadata provenance.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationPressing {
    pub releases: Vec<AutomationMetadataResult>,
    pub pick: AutomationMetadataProvenance,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationMetadataResult {
    pub source: AutomationCatalog,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// The barcode this source prints for the pressing, where it prints one.
    pub barcode: Option<String>,
    pub cover_art: Option<AutomationRemoteCover>,
    pub source_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationLibraryStatus {
    pub release_id: String,
    pub release_in_library: bool,
    pub album_in_library: bool,
    pub album_title: Option<String>,
    pub album_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationRemoteCover {
    pub url: String,
    pub thumbnail_url: String,
    pub label: String,
    pub source: AutomationCatalog,
}

/// One signal that identified the picked release, and the candidate file it
/// was read off. It explains the pick and decides nothing: a pick claims the
/// pressing whatever turned it up.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationFileEvidence {
    pub signal: AutomationEvidenceSignal,
    /// The value itself — the barcode digits, the disc ID.
    pub value: String,
    /// The file's identity within the release: its candidate-relative path.
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
pub struct AutomationReleaseDetail {
    pub release_id: String,
    pub source: AutomationCatalog,
    pub source_group_id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
    pub track_count: u32,
    pub tracks: Vec<AutomationReleaseTrack>,
    pub cover_art: Vec<AutomationRemoteCover>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleaseTrack {
    pub title: String,
    pub artist: Option<String>,
    pub duration_ms: Option<u64>,
    pub position: String,
    pub side: u32,
}

/// Start an import of a candidate. Nothing about the release rides in: the
/// pick, the metadata edits, the track rows and the cover are all stored under
/// the candidate, so the commit reads the very values it would show.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationStartImport {
    pub candidate_key: String,
    pub storage_mode: AutomationStorageMode,
    pub pin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationCoverSelection {
    Remote {
        url: String,
        source: AutomationCatalog,
    },
    Local {
        path: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStorageMode {
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationImportStarted {
    pub import_id: String,
}

/// Mirrors bae-core's `import::PrepareStep`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationPrepareStep {
    Queued,
    ValidatingSourceFiles,
}

/// Mirrors bae-core's `import::ImportPhase`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationImportPhase {
    ReadingFiles,
    MeasuringLoudness,
    Finalizing,
}

/// Where a candidate's import stands, mirroring bae-core's
/// `import::TriageImportStatus` with the running attempt's progress joined in.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationImportStatus {
    Importing {
        progress_percent: Option<u32>,
        step: Option<AutomationImportStep>,
    },
    Complete {
        release_id: String,
        album_id: String,
    },
    Error {
        error: String,
    },
}

/// Mirrors bae-core's `import::ImportStep`: the preparation step before the
/// running phases, or the running phase itself.
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
pub struct FolderInput {
    pub folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateKeyInput {
    pub candidate_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateSkipSetInput {
    pub candidate_key: String,
    pub skipped: bool,
}

/// Where a candidate's draft will be committed from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationMetadataProvenance {
    ExternalRelease {
        /// The catalog's release the draft is read from.
        record: AutomationMetadataRef,
        /// The other catalogs' releases the picked pressing paired with. The
        /// pick claims these too.
        partners: Vec<AutomationMetadataRef>,
    },
    FileTags,
}

/// One catalog's key for one release.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationMetadataRef {
    pub catalog: AutomationCatalog,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateMetadataProvenanceInput {
    pub candidate_key: String,
    pub provenance: AutomationMetadataProvenance,
}

/// One field of a candidate's metadata form. Years are text because the form
/// is text; the commit parses them.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCandidateEditField {
    AlbumTitle,
    AlbumYear,
    PressingYear,
    Format,
    Label,
    CatalogNumber,
    Country,
    Barcode,
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

/// Which storage transition to run, with whatever that transition needs. The
/// names are the Storage Manager's, not the core enum's: a caller asks to move a
/// release to the cloud, not to "make remote".
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationStorageAction {
    /// Local → Cloud. `pin` keeps the uploaded blobs offline on this device,
    /// the same choice the desktop's move-to-cloud sheet asks for.
    MoveToCloud { pin: bool },
    /// Keep a cloud release offline on this device.
    Pin,
    /// Stop keeping a cloud release offline. Its bytes stay in the cloud.
    Unpin,
    /// Cloud → Local: move the files back out into `destination_dir`, which the
    /// desktop asks for with a folder panel and a caller must supply here.
    MakeLocal { destination_dir: String },
    /// Cancel whichever transition is in flight — upload, pin, or make-local.
    /// Core dispatches on what is actually running, and does nothing when
    /// nothing is.
    Cancel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseStorageActionInput {
    pub release_id: String,
    pub action: AutomationStorageAction,
}

/// What a storage action left behind. Each transition reports the durable thing
/// it produced rather than a bare acknowledgement: a move to the cloud yields the
/// outbox revision its uploads were queued at, which is what a caller waits on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationStorageActionOutcome {
    /// The uploads are queued and draining; `outbox_revision` is the durable
    /// queue revision they were committed at. Poll the release to see it land.
    CloudUploadQueued {
        release_id: String,
        outbox_revision: u64,
    },
    /// The pin joined the download queue, which serializes and reports it. The
    /// bytes are not offline yet when this returns.
    PinQueued {
        release_id: String,
    },
    Unpinned {
        release_id: String,
    },
    /// The files are at their new path and the release no longer names its cloud
    /// copies — this one completes before it returns.
    MadeLocal {
        release_id: String,
    },
    /// Whatever was in flight was told to stop. A release with nothing running
    /// reports this too: core treats the cancel as a no-op rather than an error.
    Cancelled {
        release_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseExportInput {
    pub release_id: String,
    pub target_dir: String,
}

/// Acknowledges that an export was enqueued. The copy runs on the background
/// export queue; poll `output_status` for progress.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseExport {
    pub release_id: String,
}

/// A queued export's state, mirroring bae-core's `OutputState`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationOutputState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// What a queued release output produces. Mirrors bae-core's `OutputKind`; a
/// save carries its preset's display name (resolved at enqueue).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationOutputKind {
    Export,
    Save { preset_name: String },
}

/// One queued release output in the `output_status` snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationOutputOp {
    pub release_id: String,
    pub target_dir: String,
    pub title: String,
    pub file_count: i64,
    pub total_size: i64,
    pub created_at: i64,
    pub state: AutomationOutputState,
    /// Whether this row is a verbatim export or a preset save.
    pub kind: AutomationOutputKind,
}

/// Per-state counts for the export queue.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationOutputProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory export queue snapshot returned by `output_status`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationOutputSnapshot {
    pub outputs: Vec<AutomationOutputOp>,
    pub total: AutomationOutputProgress,
    pub paused: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseReidentifyInput {
    pub release_id: String,
    pub choice: AutomationReleaseReseed,
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
