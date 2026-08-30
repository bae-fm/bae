use super::*;

mod error;
mod metadata_edit;

pub use error::AutomationError;
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

    pub(super) fn path(&self) -> &str {
        &self.common().path
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationCandidateCommon {
    pub key: String,
    pub path: String,
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

/// Mirrors bae-core's `signals::SourcedValue`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationSourcedValue {
    pub value: String,
    pub origin: AutomationSignalOrigin,
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

/// Projects bae-core's `identify::DiscidProgress` — mid-flight result payloads
/// reduce to a count; the full match set surfaces only in a terminal state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationDiscidProgress {
    Computing,
    LookingUp,
    Done { n_results: u32 },
    Skipped,
    Failed { failure: AutomationLookupFailure },
}

/// Projects bae-core's `identify::BarcodeProgress` — mid-flight result payloads
/// reduce to a count; the full match set surfaces only in a terminal state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationBarcodeProgress {
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
        failure: AutomationLookupFailure,
    },
    Skipped,
}

/// Mirrors bae-core's `identify::ResultProvenance`, paired with the release id
/// it aligns to (the core type is index-aligned with the match list).
#[derive(Debug, Clone, Serialize)]
pub struct AutomationResultProvenance {
    pub release_id: String,
    pub by_disc_id: bool,
    pub by_barcode: bool,
    pub by_catalog: bool,
}

/// Projects bae-core's `identify::IdentifyState`. The `SignalsContext`
/// internals that drive core triangulation don't cross; terminal states carry
/// the full match data an MCP client acts on.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationIdentifyState {
    Idle,
    Triangulating {
        discid: AutomationDiscidProgress,
        barcode: AutomationBarcodeProgress,
    },
    Found {
        groups: Vec<AutomationReleaseGroup>,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        provenance: Vec<AutomationResultProvenance>,
    },
    NotFoundAnywhere,
    ManualOnly {
        track_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationSearchQuery {
    General {
        artist: String,
        album: String,
        source: AutomationMetadataSource,
    },
    CatalogNumber {
        catalog_number: String,
        source: AutomationMetadataSource,
    },
    Barcode {
        barcode: String,
        source: AutomationMetadataSource,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationMetadataSource {
    MusicBrainz,
    Discogs,
}

impl From<AutomationMetadataSource> for MetadataSource {
    fn from(value: AutomationMetadataSource) -> Self {
        match value {
            AutomationMetadataSource::MusicBrainz => MetadataSource::MusicBrainz,
            AutomationMetadataSource::Discogs => MetadataSource::Discogs,
        }
    }
}

impl From<MetadataSource> for AutomationMetadataSource {
    fn from(value: MetadataSource) -> Self {
        match value {
            MetadataSource::MusicBrainz => AutomationMetadataSource::MusicBrainz,
            MetadataSource::Discogs => AutomationMetadataSource::Discogs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationReleaseReseed {
    ExternalRelease {
        source: AutomationMetadataSource,
        release_id: String,
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
    pub cover_art: Option<AutomationRemoteCover>,
    pub source_label: String,
    pub group_url: Option<String>,
    pub year_min: Option<i32>,
    pub year_max: Option<i32>,
    pub pressings: Vec<AutomationMetadataResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationMetadataResult {
    pub source: AutomationMetadataSource,
    pub release_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
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
    pub source: AutomationMetadataSource,
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
    pub source: AutomationMetadataSource,
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
        source: AutomationMetadataSource,
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
    ReadingFolder,
    ParsingMetadata,
    WritingCoverArt,
    DiscoveringFiles,
    ValidatingTracks,
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
        progress_percent: u32,
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
pub struct AutomationRelease {
    pub summary: AutomationReleaseSummary,
    pub display_name: String,
    pub year: Option<i32>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub total_duration_ms: i64,
    pub tracks: Vec<AutomationTrackDetail>,
    pub track_groups: Vec<AutomationTrackGroup>,
    pub files: Vec<AutomationFileDetail>,
    pub image_files: Vec<AutomationFileDetail>,
    pub gallery_items: Vec<AutomationGalleryItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// Where the audio lives. Orthogonal to `pinned`.
    pub storage_state: AutomationReleaseStorageState,
    /// Whether a Remote release is kept offline on this device — the orthogonal
    /// cache property, never folded into `storage_state`.
    pub pinned: bool,
    /// The transitions available right now, derived by the core.
    pub storage_actions: Vec<AutomationReleaseStorageAction>,
    /// The transition currently in flight, if any — so a client can tell a release
    /// is mid-transfer rather than reading `storage_actions` and guessing.
    pub transfer_action: Option<AutomationReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    pub cover: Option<AutomationImageRef>,
}

/// A release's storage state. Mirrors `bae_core::album_detail::ReleaseStorageState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationReleaseStorageState {
    Local,
    Remote,
}

impl From<ReleaseStorageState> for AutomationReleaseStorageState {
    fn from(state: ReleaseStorageState) -> Self {
        match state {
            ReleaseStorageState::Local => Self::Local,
            ReleaseStorageState::Remote => Self::Remote,
        }
    }
}

/// A storage transition a release allows. Mirrors
/// `bae_core::album_detail::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationReleaseStorageAction {
    MakeRemote,
    Pin,
    Unpin,
    MakeLocal,
}

impl From<ReleaseStorageAction> for AutomationReleaseStorageAction {
    fn from(action: ReleaseStorageAction) -> Self {
        match action {
            ReleaseStorageAction::MakeRemote => Self::MakeRemote,
            ReleaseStorageAction::Pin => Self::Pin,
            ReleaseStorageAction::Unpin => Self::Unpin,
            ReleaseStorageAction::MakeLocal => Self::MakeLocal,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationImageRef {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackDetail {
    pub id: String,
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    pub artist_names: String,
    pub position_text: String,
    pub position: AutomationTrackPosition,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackPosition {
    Sided { side_letter: String, number: i32 },
    SidedUnnumbered { side_letter: String },
    Disc { disc: i32, number: i32 },
    DiscUnnumbered { disc: i32 },
    Flat { number: i32 },
    Unnumbered,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackGroup {
    pub side: AutomationTrackSide,
    pub tracks: Vec<AutomationTrackDetail>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationTrackSide {
    Sided { side_letter: String },
    Disc { disc: i32 },
    Flat,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationFileDetail {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub is_image: bool,
    pub content_type: String,
    pub audio_format: Option<AutomationAudioFormat>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationAudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationGalleryItem {
    pub id: String,
    pub label: String,
    pub source: AutomationGallerySource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationGallerySource {
    Cover { image: AutomationImageRef },
    ReleaseFile { file_id: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationLibrarySearchResults {
    pub albums: Vec<AutomationAlbumSearchResult>,
    pub tracks: Vec<AutomationTrackSearchResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    pub cover: Option<AutomationImageRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
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

/// The metadata source a candidate will be committed from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationMetadataProvenance {
    ExternalRelease {
        source: AutomationMetadataSource,
        release_id: String,
    },
    FileTags,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CandidateMetadataProvenanceInput {
    pub candidate_key: String,
    pub provenance: AutomationMetadataProvenance,
}

/// One album-level field of a candidate's metadata form. `year` is text
/// because the form is text; the commit parses it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AutomationCandidateEditField {
    AlbumTitle,
    Year,
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
    /// The files are at their new path and the cloud copies are tombstoned —
    /// this one completes before it returns.
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
