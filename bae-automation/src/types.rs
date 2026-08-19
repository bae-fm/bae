use super::*;

mod error;

pub use error::AutomationError;

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
        format_label: String,
        content_hash: String,
        /// What the identify and import pipelines have recorded against this
        /// candidate. Every scanned folder carries one — idle until something
        /// runs — because the import service keeps it alongside the candidate.
        runtime: AutomationCandidateRuntime,
    },
    Invalid {
        #[serde(flatten)]
        common: AutomationCandidateCommon,
        invalid_reason: String,
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

/// Mirrors bae-core's `import::CandidateRuntimeSnapshot`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationCandidateRuntime {
    pub identify_state: AutomationIdentifyState,
    pub toolbar: Vec<AutomationToolbarSignal>,
    pub signals: Option<AutomationSignals>,
    pub import_status: Option<AutomationImportStatus>,
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

/// Mirrors bae-core's `identify::SignalRole`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationSignalRole {
    Identity,
    Filter,
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
    Confirms { count: u32 },
}

/// Mirrors bae-core's `identify::ToolbarSignal`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationToolbarSignal {
    pub kind: AutomationSignalKind,
    pub role: AutomationSignalRole,
    pub value: Option<String>,
    pub origin: AutomationSignalOrigin,
    pub state: AutomationSignalState,
    pub excluded: bool,
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
    pub matches_catalog: bool,
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
        group: AutomationReleaseGroup,
        library_statuses: Vec<AutomationLibraryStatus>,
        track_count: u32,
        provenance: Vec<AutomationResultProvenance>,
    },
    Conflict {
        discid_results: Vec<AutomationMetadataResult>,
        discid_library_statuses: Vec<AutomationLibraryStatus>,
        barcode_results: Vec<AutomationMetadataResult>,
        barcode_library_statuses: Vec<AutomationLibraryStatus>,
        matched_barcode: Option<String>,
        track_count: u32,
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
pub enum AutomationIdentityChoice {
    Exact {
        source: AutomationMetadataSource,
        release_id: String,
    },
    Approximate {
        source: AutomationMetadataSource,
        release_id: String,
    },
    Unknown,
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

/// What picking a release gives the confirmation step: the display `detail`,
/// the metadata editor's seed, and the identity `claim` a pick records. The
/// seed is projected from the release exactly as the commit worker maps it, so
/// it — never the detail — is what an import's `user_edit` overlay is built
/// from.
///
/// The field is `unmasked_seed`, not `seed`, because it is the projection
/// *before* a claim is applied to it — an album-level claim blanks the pressing
/// block, and this still carries it. The desktop surfaces receive the masked
/// form under `seed` and bind it directly; an automation caller has to decide,
/// because it is the one caller that can commit a claim other than `claim`.
/// Pass this through `import_release_edit_shape` with whichever claim is being
/// committed, and use its output as the overlay. Binding this value directly
/// writes pressing fields the claim says are not known.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleasePrefetch {
    pub detail: AutomationReleaseDetail,
    pub unmasked_seed: AutomationReleaseUserEdit,
    pub claim: AutomationClaimLine,
}

/// What identified the picked release. It explains the pick and decides
/// nothing: a pick claims the pressing whatever turned it up.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationClaimEvidence {
    DiscIdAlone,
    DiscIdShared { match_count: u32 },
    Barcode,
    Search,
}

/// What a pick claims, and the release the metadata came from. `choice` is the
/// pressing claim a pick records and what `import_start` carries to commit it;
/// a caller claiming only the album passes an `approximate` `identity_choice`
/// instead.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationClaimLine {
    pub choice: AutomationIdentityChoice,
    pub evidence: AutomationClaimEvidence,
    /// The picked release's pressing facts, `·`-joined, or absent when it
    /// states none.
    pub release: Option<String>,
    pub track_count: Option<u32>,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleaseUserEdit {
    pub album_title: String,
    pub album_artist_names: Vec<String>,
    pub pressing: AutomationPressingEdit,
    pub tracks: Vec<AutomationTrackUserEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationPressingEdit {
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationTrackUserEdit {
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub artist_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationStartImport {
    pub candidate_key: String,
    pub selected_cover: Option<AutomationCoverSelection>,
    pub storage_mode: AutomationStorageMode,
    pub pin: bool,
    pub identity_choice: AutomationIdentityChoice,
    pub user_edit: Option<AutomationReleaseUserEdit>,
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

/// Where a candidate's own import run stands, mirroring bae-core's
/// `import::CandidateImportStatusSnapshot`.
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
    CloudUploadQueued {
        release_id: String,
        album_id: String,
        outbox_revision: u64,
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

/// Picking a release for a candidate. The candidate is part of the input
/// because the claim line reads that candidate's identify evidence for the
/// clause explaining what turned the release up — a key with no evidence reads
/// as "found by searching".
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleasePrefetchInput {
    pub candidate_key: String,
    pub source: AutomationMetadataSource,
    pub release_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShapeReleaseEditInput {
    /// The editor seed from `import_release_prefetch`'s `unmasked_seed`.
    pub seed: AutomationReleaseUserEdit,
    pub choice: AutomationIdentityChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseIdInput {
    pub release_id: String,
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
    pub choice: AutomationIdentityChoice,
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
