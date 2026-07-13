#![deny(unreachable_pub, dead_code)]

use bae_core::album_detail::{
    AudioFormat, FileDetail, GalleryItem, GallerySource, ImageRef, ReleaseDetail,
    ReleaseStorageAction, ReleaseStorageState, SearchResults, TrackDetail, TrackPosition,
    TrackSide,
};
use bae_core::config::McpConfig;
use bae_core::db::LibraryStatus;
use bae_core::import::cover_art::RemoteCover;
use bae_core::import::folder_scanner::{FolderCandidate, InvalidCandidate};
use bae_core::import::release_group::ReleaseGroup;
use bae_core::import::search::{ImportSearchReleaseDetail, MetadataResult};
use bae_core::import::{
    shape_user_edit_for_choice, CoverSelection, GroupedSearchResults, IdentityChoice, ImportError,
    ImportEvent, ImportPhase, ImportProgress, MetadataRef, MetadataSource, PrepareStep,
    PressingEdit, ScanEvent, SearchQuery, StorageMode, TrackUserEdit,
};
use bae_core::library::{AppServices, LibraryError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::warn;

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
pub struct AutomationStatus {
    pub config: AutomationConfig,
    pub event_indexing: AutomationEventIndexing,
    pub candidate_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEventIndexing {
    Started,
    NotStarted,
    Failed { message: String },
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
    },
    Invalid {
        #[serde(flatten)]
        common: AutomationCandidateCommon,
        invalid_reason: String,
    },
}

impl AutomationCandidate {
    fn common(&self) -> &AutomationCandidateCommon {
        match self {
            Self::Valid { common, .. } | Self::Invalid { common, .. } => common,
        }
    }

    fn common_mut(&mut self) -> &mut AutomationCandidateCommon {
        match self {
            Self::Valid { common, .. } | Self::Invalid { common, .. } => common,
        }
    }

    fn key(&self) -> &str {
        &self.common().key
    }

    fn path(&self) -> &str {
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
    pub runtime: Option<AutomationCandidateRuntime>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationCandidateRuntime {
    pub identify_state: Option<AutomationIdentifyState>,
    pub toolbar: Option<Vec<AutomationToolbarSignal>>,
    pub signals: Option<AutomationSignals>,
    pub progress: Option<AutomationImportProgress>,
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

/// Mirrors bae-core's `identify::IdentifySource`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationIdentifySource {
    Discid,
    Barcode,
    Combined,
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
        source: AutomationIdentifySource,
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

/// What picking a release gives the confirmation step: the display `detail` and
/// the metadata editor's `seed`. The seed is projected from the release exactly
/// as the commit worker maps it, so it — never the detail — is what an import's
/// `user_edit` overlay is built from.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AutomationReleasePrefetch {
    pub detail: AutomationReleaseDetail,
    pub seed: AutomationReleaseUserEdit,
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
    pub folder: String,
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
    ParsingMetadata,
    WritingCoverArt,
    DiscoveringFiles,
    ValidatingTracks,
    SavingToDatabase,
}

/// Mirrors bae-core's `import::ImportPhase`.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationImportPhase {
    ReferencingFiles,
    MeasuringLoudness,
    Finalizing,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutomationImportProgress {
    Preparing {
        import_id: String,
        step: AutomationPrepareStep,
        album_title: String,
        artist_name: String,
    },
    Started {
        id: String,
        import_id: String,
    },
    Progress {
        id: String,
        percent: u8,
        phase: AutomationImportPhase,
        import_id: String,
    },
    Complete {
        id: String,
        import_id: String,
        album_id: String,
    },
    RemoteUploadQueued {
        id: String,
        import_id: String,
        album_id: String,
    },
    Failed {
        error: String,
        import_id: String,
    },
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
    pub storage_state: String,
    pub pinned: bool,
    pub storage_actions: Vec<String>,
    pub file_count: i64,
    pub total_size: i64,
    pub cover: Option<AutomationImageRef>,
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseSourceInput {
    pub source: AutomationMetadataSource,
    pub release_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShapeReleaseEditInput {
    /// The editor seed from `import_release_prefetch`.
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
/// export queue; poll `export_status` for progress.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationReleaseExport {
    pub release_id: String,
}

/// A queued export's state, mirroring bae-core's `ExportState`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationExportState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// One queued export in the `export_status` snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationExportOp {
    pub release_id: String,
    pub target_dir: String,
    pub title: String,
    pub file_count: i64,
    pub total_size: i64,
    pub created_at: i64,
    pub state: AutomationExportState,
}

/// Per-state counts for the export queue.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AutomationExportProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory export queue snapshot returned by `export_status`.
#[derive(Debug, Clone, Serialize)]
pub struct AutomationExportSnapshot {
    pub exports: Vec<AutomationExportOp>,
    pub total: AutomationExportProgress,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "message")]
pub enum AutomationError {
    Database(String),
    Import(String),
    NotFound(String),
    Validation(String),
    Unavailable(String),
    Timeout(String),
    Internal(String),
}

impl AutomationError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Database(_) => "database",
            Self::Import(_) => "import",
            Self::NotFound(_) => "not_found",
            Self::Validation(_) => "validation",
            Self::Unavailable(_) => "unavailable",
            Self::Timeout(_) => "timeout",
            Self::Internal(_) => "internal",
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Database(message)
            | Self::Import(message)
            | Self::NotFound(message)
            | Self::Validation(message)
            | Self::Unavailable(message)
            | Self::Timeout(message)
            | Self::Internal(message) => message,
        }
    }

    fn import(message: impl Into<String>) -> Self {
        Self::Import(message.into())
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound(message.into())
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }
}

impl std::fmt::Display for AutomationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind(), self.message())
    }
}

impl std::error::Error for AutomationError {}

impl From<LibraryError> for AutomationError {
    fn from(value: LibraryError) -> Self {
        match value {
            LibraryError::Database(e) => Self::Database(e.to_string()),
            LibraryError::Io(e) => Self::Unavailable(e.to_string()),
            LibraryError::Import(e) => Self::Import(e),
            LibraryError::TrackMapping(e) => Self::Import(e),
            LibraryError::Encryption(e) => Self::Unavailable(e.to_string()),
            LibraryError::Storage(e) => Self::Unavailable(e),
            LibraryError::Playback(e) => Self::Unavailable(e),
            LibraryError::Internal(e) => Self::Internal(e),
            LibraryError::Config(e) => Self::Internal(e.to_string()),
            LibraryError::Keyring(e) => Self::Unavailable(e.to_string()),
            LibraryError::CloudHome(e) => Self::Unavailable(e.to_string()),
            LibraryError::CloudSetup(e) => Self::Unavailable(e.to_string()),
            LibraryError::Sync(e) => Self::Unavailable(e.to_string()),
            LibraryError::Validation(e) => Self::Validation(e),
            LibraryError::MasterKey(e) => Self::Unavailable(e.to_string()),
        }
    }
}

impl From<ImportError> for AutomationError {
    /// Import failures cross as an opaque `import` error carrying the typed
    /// error's Display as the message.
    fn from(value: ImportError) -> Self {
        Self::Import(value.to_string())
    }
}

struct AutomationState {
    candidates: RwLock<HashMap<String, AutomationCandidate>>,
    event_indexing: RwLock<AutomationEventIndexing>,
}

impl AutomationState {
    fn new() -> Self {
        Self {
            candidates: RwLock::new(HashMap::new()),
            event_indexing: RwLock::new(AutomationEventIndexing::NotStarted),
        }
    }

    fn start_event_indexing(&self) -> bool {
        let mut event_indexing = self
            .event_indexing
            .write()
            .expect("event indexing state poisoned");
        match *event_indexing {
            AutomationEventIndexing::NotStarted => {
                *event_indexing = AutomationEventIndexing::Started;
                true
            }
            AutomationEventIndexing::Started | AutomationEventIndexing::Failed { .. } => false,
        }
    }

    fn apply_event(&self, event: ImportEvent) {
        match event {
            ImportEvent::Scan(event) => self.apply_scan_event(event),
            ImportEvent::ImportProgress {
                candidate_key,
                progress,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().runtime_mut().progress =
                    Some(automation_import_progress(progress));
            }),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::ImportLoudnessProgress { .. } => {}
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::IdentifyStateChanged {
                candidate_key,
                state,
                toolbar,
            } => self.update_candidate(candidate_key, |candidate| {
                let common = candidate.common_mut();
                let runtime = common.runtime_mut();
                runtime.identify_state = Some(automation_identify_state(state));
                runtime.toolbar =
                    Some(toolbar.into_iter().map(automation_toolbar_signal).collect());
            }),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            ImportEvent::SignalsUpdated {
                candidate_key,
                signals,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().runtime_mut().signals = Some(automation_signals(signals));
            }),
        }
    }

    fn apply_scan_event(&self, event: ScanEvent) {
        match event {
            ScanEvent::WatchedFoldersChanged { .. } => {}
            ScanEvent::FolderCandidate(candidate) => {
                self.insert_candidate(automation_candidate_from_folder(candidate));
            }
            ScanEvent::InvalidCandidate(candidate) => {
                self.insert_candidate(automation_candidate_from_invalid(candidate));
            }
            ScanEvent::CandidateRemoved { candidate_key } => {
                let removed = self
                    .candidates
                    .write()
                    .expect("candidate index poisoned")
                    .remove(&candidate_key);
                if removed.is_none() {
                    self.fail_missing_candidate("removal", &candidate_key);
                }
            }
            ScanEvent::CandidateSkipChanged {
                candidate_key,
                skipped,
            } => self.update_candidate(candidate_key, |candidate| {
                candidate.common_mut().skipped = skipped;
            }),
            ScanEvent::Failed { error } => self.fail_event_indexing(error),
            ScanEvent::Finished => {}
        }
    }

    fn insert_candidate(&self, candidate: AutomationCandidate) {
        self.candidates
            .write()
            .expect("candidate index poisoned")
            .insert(candidate.key().to_string(), candidate);
    }

    fn update_candidate(
        &self,
        candidate_key: String,
        update: impl FnOnce(&mut AutomationCandidate),
    ) {
        let updated = {
            let mut candidates = self.candidates.write().expect("candidate index poisoned");
            if let Some(candidate) = candidates.get_mut(&candidate_key) {
                update(candidate);
                true
            } else {
                false
            }
        };
        if !updated {
            self.fail_missing_candidate("update", &candidate_key);
        }
    }

    fn fail_missing_candidate(&self, action: &str, candidate_key: &str) {
        let message =
            format!("automation candidate {action} referenced unknown candidate '{candidate_key}'");
        warn!("{message}");
        self.fail_event_indexing(message);
    }

    fn fail_event_indexing(&self, message: String) {
        let mut event_indexing = self
            .event_indexing
            .write()
            .expect("event indexing state poisoned");
        *event_indexing = AutomationEventIndexing::Failed { message };
    }

    fn event_indexing(&self) -> AutomationEventIndexing {
        self.event_indexing
            .read()
            .expect("event indexing state poisoned")
            .clone()
    }

    fn ensure_event_index_ready(&self) -> Result<(), AutomationError> {
        let event_indexing = self
            .event_indexing
            .read()
            .expect("event indexing state poisoned");
        match &*event_indexing {
            AutomationEventIndexing::Failed { message } => {
                Err(AutomationError::Unavailable(message.clone()))
            }
            AutomationEventIndexing::Started | AutomationEventIndexing::NotStarted => Ok(()),
        }
    }

    fn candidate_count(&self) -> usize {
        self.candidates
            .read()
            .expect("candidate index poisoned")
            .len()
    }

    fn list_candidates(&self) -> Result<Vec<AutomationCandidate>, AutomationError> {
        self.ensure_event_index_ready()?;
        let mut candidates = self
            .candidates
            .read()
            .expect("candidate index poisoned")
            .values()
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by(|a, b| a.path().cmp(b.path()));
        Ok(candidates)
    }
}

#[derive(Clone)]
pub struct Automation {
    services: AppServices,
    runtime_handle: tokio::runtime::Handle,
    state: Arc<AutomationState>,
}

impl Automation {
    pub fn new(services: AppServices, runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            services,
            runtime_handle,
            state: Arc::new(AutomationState::new()),
        }
    }

    pub fn start_event_indexing(&self) {
        if !self.state.start_event_indexing() {
            return;
        }
        let state = self.state.clone();
        let mut rx = self.services.import().subscribe_events();
        self.runtime_handle.spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => state.apply_event(event),
                    Err(error) => {
                        let message = match error {
                            broadcast::error::RecvError::Lagged(n) => {
                                format!("automation import event index lagged by {n} events")
                            }
                            broadcast::error::RecvError::Closed => {
                                "automation import event channel closed".to_string()
                            }
                        };
                        warn!("{message}");
                        state.fail_event_indexing(message);
                        break;
                    }
                }
            }
        });
    }

    pub fn status(&self) -> AutomationStatus {
        AutomationStatus {
            config: self.config_get(),
            event_indexing: self.state.event_indexing(),
            candidate_count: self.state.candidate_count(),
        }
    }

    pub fn config_get(&self) -> AutomationConfig {
        let config = self.services.library_manager().get_config();
        AutomationConfig {
            library_id: config.store_id.clone(),
            library_name: config.store_name.clone(),
            library_path: config.store_dir.to_string_lossy().to_string(),
            mcp: config.mcp.into(),
        }
    }

    pub fn watched_folders(&self) -> Vec<AutomationWatchedFolder> {
        self.services
            .import()
            .watched_folders()
            .into_iter()
            .map(|folder| AutomationWatchedFolder {
                path: folder.path,
                name: folder.name,
            })
            .collect()
    }

    pub fn add_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import().add_watched_folder(path)?;
        Ok(self.watched_folders())
    }

    pub fn remove_watched_folder(
        &self,
        path: String,
    ) -> Result<Vec<AutomationWatchedFolder>, AutomationError> {
        self.services.import().remove_watched_folder(path)?;
        Ok(self.watched_folders())
    }

    pub async fn scan_watched_folders(
        &self,
        wait: ScanWait,
    ) -> Result<AutomationScanResult, AutomationError> {
        match wait {
            ScanWait::NoWait => {
                self.services.import().scan_watched_folders()?;
            }
            ScanWait::UntilFinished { timeout_ms } => {
                let mut rx = self.services.import().subscribe_folder_scan_events();
                self.services.import().scan_watched_folders()?;
                let wait_for_finish = async {
                    while let Some(event) = rx.recv().await {
                        match event {
                            ScanEvent::Finished => return Ok::<(), AutomationError>(()),
                            ScanEvent::Failed { error } => {
                                return Err(AutomationError::import(error));
                            }
                            _ => {}
                        }
                    }
                    Err(AutomationError::Unavailable(
                        "scan event channel closed before finish".to_string(),
                    ))
                };
                tokio::time::timeout(Duration::from_millis(timeout_ms), wait_for_finish)
                    .await
                    .map_err(|_| {
                        AutomationError::Timeout(
                            "timed out waiting for watched-folder scan".to_string(),
                        )
                    })??;
            }
        }
        Ok(AutomationScanResult {
            watched_folders: self.watched_folders(),
            candidates: self.list_candidates()?,
        })
    }

    pub fn list_candidates(&self) -> Result<Vec<AutomationCandidate>, AutomationError> {
        self.state.list_candidates()
    }

    pub fn get_candidate(
        &self,
        candidate_key: String,
    ) -> Result<AutomationCandidate, AutomationError> {
        self.state.ensure_event_index_ready()?;
        self.state
            .candidates
            .read()
            .expect("candidate index poisoned")
            .get(&candidate_key)
            .cloned()
            .ok_or_else(|| {
                AutomationError::not_found(format!("candidate '{candidate_key}' not found"))
            })
    }

    pub fn set_candidate_skipped(
        &self,
        candidate_key: String,
        skipped: bool,
    ) -> Result<(), AutomationError> {
        self.services
            .import()
            .set_candidate_skipped(candidate_key, skipped)?;
        Ok(())
    }

    pub async fn search_imports(
        &self,
        query: AutomationSearchQuery,
    ) -> Result<AutomationSearchResults, AutomationError> {
        let results = self
            .services
            .import()
            .search_with_status(search_query(query))
            .await?;
        Ok(automation_search_results(results))
    }

    pub async fn prefetch_release(
        &self,
        source: AutomationMetadataSource,
        release_id: String,
    ) -> Result<AutomationReleasePrefetch, AutomationError> {
        let prefetch = self
            .services
            .import()
            .prefetch_release(&release_id, source.into())
            .await?;
        Ok(AutomationReleasePrefetch {
            detail: automation_release_detail(prefetch.detail),
            seed: automation_release_user_edit(prefetch.seed),
        })
    }

    pub async fn preview_file_tags(
        &self,
        folder: String,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        let edit = self
            .services
            .import()
            .preview_file_tags_for_folder(PathBuf::from(folder))
            .await?;
        Ok(automation_release_user_edit(edit))
    }

    pub async fn shape_release_edit(
        &self,
        seed: AutomationReleaseUserEdit,
        choice: AutomationIdentityChoice,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        let seed = release_user_edit(seed);
        let choice = identity_choice(choice);
        Ok(automation_release_user_edit(shape_user_edit_for_choice(
            &seed, &choice,
        )))
    }

    pub fn start_import(
        &self,
        request: AutomationStartImport,
    ) -> Result<AutomationImportStarted, AutomationError> {
        let import_id = self.services.import().start_import(
            &request.candidate_key,
            PathBuf::from(request.folder),
            request.selected_cover.map(cover_selection),
            storage_mode(request.storage_mode),
            request.pin,
            identity_choice(request.identity_choice),
            request.user_edit.map(release_user_edit),
        )?;
        Ok(AutomationImportStarted { import_id })
    }

    pub async fn release_detail(
        &self,
        release_id: String,
    ) -> Result<AutomationRelease, AutomationError> {
        self.services
            .library_manager()
            .find_release_detail(&release_id)
            .await?
            .map(automation_release)
            .ok_or_else(|| AutomationError::not_found(format!("release '{release_id}' not found")))
    }

    /// Enqueue a release to export its files verbatim to `target_dir`. Returns
    /// immediately once queued; the copy runs on the background export queue.
    pub async fn export_release(
        &self,
        release_id: String,
        target_dir: String,
    ) -> Result<AutomationReleaseExport, AutomationError> {
        self.services
            .library_manager()
            .enqueue_export(
                &release_id,
                PathBuf::from(target_dir),
                bae_core::config::ExportSelection::Original,
            )
            .await?;
        Ok(AutomationReleaseExport { release_id })
    }

    /// The current export-queue snapshot: per-release state and rolled-up counts.
    pub fn export_status(&self) -> AutomationExportSnapshot {
        automation_export_snapshot(self.services.library_manager().export_snapshot())
    }

    pub async fn reidentify_release(
        &self,
        release_id: String,
        choice: AutomationIdentityChoice,
    ) -> Result<(), AutomationError> {
        self.services
            .library_manager()
            .re_identify_release(&release_id, identity_choice(choice))
            .await?;
        Ok(())
    }

    pub async fn reset_release_metadata(
        &self,
        release_id: String,
    ) -> Result<AutomationReleaseUserEdit, AutomationError> {
        self.services
            .library_manager()
            .reset_metadata_to_source(&release_id)
            .await
            .map(automation_release_user_edit)
            .map_err(AutomationError::from)
    }

    pub async fn update_release_metadata(
        &self,
        release_id: String,
        edit: AutomationReleaseUserEdit,
    ) -> Result<(), AutomationError> {
        self.services
            .library_manager()
            .apply_release_metadata_user_edit(&release_id, &release_user_edit(edit))
            .await?;
        Ok(())
    }

    pub async fn search_library(
        &self,
        query: String,
    ) -> Result<AutomationLibrarySearchResults, AutomationError> {
        self.services
            .library_manager()
            .search_library(&query, 50)
            .await
            .map(automation_library_search_results)
            .map_err(AutomationError::from)
    }

    pub async fn call_tool(
        &self,
        tool: AutomationTool,
        args: Value,
    ) -> Result<Value, AutomationError> {
        match tool {
            AutomationTool::ConfigGet => {
                expect_no_args(args, tool.name())?;
                to_value(self.config_get())
            }
            AutomationTool::WatchedFoldersList => {
                expect_no_args(args, tool.name())?;
                to_list_value("watched_folders", self.watched_folders())
            }
            AutomationTool::WatchedFolderAdd => {
                let input: PathInput = from_value(args)?;
                to_list_value("watched_folders", self.add_watched_folder(input.path)?)
            }
            AutomationTool::WatchedFolderRemove => {
                let input: PathInput = from_value(args)?;
                to_list_value("watched_folders", self.remove_watched_folder(input.path)?)
            }
            AutomationTool::WatchedFoldersScan => {
                let wait: ScanWait = from_value(args)?;
                to_value(self.scan_watched_folders(wait).await?)
            }
            AutomationTool::ImportCandidatesList => {
                expect_no_args(args, tool.name())?;
                to_list_value("candidates", self.list_candidates()?)
            }
            AutomationTool::ImportCandidateGet => {
                let input: CandidateKeyInput = from_value(args)?;
                to_value(self.get_candidate(input.candidate_key)?)
            }
            AutomationTool::ImportCandidateSkipSet => {
                let input: CandidateSkipSetInput = from_value(args)?;
                self.set_candidate_skipped(input.candidate_key, input.skipped)?;
                to_value(EmptyResponse {})
            }
            AutomationTool::ImportSearch => {
                let query: AutomationSearchQuery = from_value(args)?;
                to_value(self.search_imports(query).await?)
            }
            AutomationTool::ImportReleasePrefetch => {
                let input: ReleaseSourceInput = from_value(args)?;
                to_value(
                    self.prefetch_release(input.source, input.release_id)
                        .await?,
                )
            }
            AutomationTool::ImportFileTagsPreview => {
                let input: FolderInput = from_value(args)?;
                to_value(self.preview_file_tags(input.folder).await?)
            }
            AutomationTool::ImportReleaseEditShape => {
                let input: ShapeReleaseEditInput = from_value(args)?;
                to_value(self.shape_release_edit(input.seed, input.choice).await?)
            }
            AutomationTool::ImportStart => {
                let input: AutomationStartImport = from_value(args)?;
                to_value(self.start_import(input)?)
            }
            AutomationTool::ReleaseDetailGet => {
                let input: ReleaseIdInput = from_value(args)?;
                to_value(self.release_detail(input.release_id).await?)
            }
            AutomationTool::ReleaseExport => {
                let input: ReleaseExportInput = from_value(args)?;
                to_value(
                    self.export_release(input.release_id, input.target_dir)
                        .await?,
                )
            }
            AutomationTool::ExportStatus => {
                expect_no_args(args, tool.name())?;
                to_value(self.export_status())
            }
            AutomationTool::ReleaseReidentify => {
                let input: ReleaseReidentifyInput = from_value(args)?;
                self.reidentify_release(input.release_id, input.choice)
                    .await?;
                to_value(EmptyResponse {})
            }
            AutomationTool::ReleaseMetadataReset => {
                let input: ReleaseIdInput = from_value(args)?;
                to_value(self.reset_release_metadata(input.release_id).await?)
            }
            AutomationTool::ReleaseMetadataUpdate => {
                let input: ReleaseMetadataUpdateInput = from_value(args)?;
                self.update_release_metadata(input.release_id, input.edit)
                    .await?;
                to_value(EmptyResponse {})
            }
            AutomationTool::LibrarySearch => {
                let input: LibrarySearchInput = from_value(args)?;
                to_value(self.search_library(input.query).await?)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutomationTool {
    ConfigGet,
    WatchedFoldersList,
    WatchedFolderAdd,
    WatchedFolderRemove,
    WatchedFoldersScan,
    ImportCandidatesList,
    ImportCandidateGet,
    ImportCandidateSkipSet,
    ImportSearch,
    ImportReleasePrefetch,
    ImportFileTagsPreview,
    ImportReleaseEditShape,
    ImportStart,
    ReleaseDetailGet,
    ReleaseExport,
    ExportStatus,
    ReleaseReidentify,
    ReleaseMetadataReset,
    ReleaseMetadataUpdate,
    LibrarySearch,
}

impl AutomationTool {
    const DESCRIPTORS: [AutomationToolDescriptor; 20] = [
        AutomationToolDescriptor {
            tool: AutomationTool::ConfigGet,
            name: "config_get",
            description: "Get active library automation config",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersList,
            name: "watched_folders_list",
            description: "List watched import folders",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderAdd,
            name: "watched_folder_add",
            description: "Add a watched import folder",
            input: AutomationToolInput::Path,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFolderRemove,
            name: "watched_folder_remove",
            description: "Remove a watched import folder",
            input: AutomationToolInput::Path,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::WatchedFoldersScan,
            name: "watched_folders_scan",
            description: "Scan watched import folders",
            input: AutomationToolInput::ScanWait,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidatesList,
            name: "import_candidates_list",
            description: "List indexed import candidates",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateGet,
            name: "import_candidate_get",
            description: "Get an indexed import candidate",
            input: AutomationToolInput::CandidateKey,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportCandidateSkipSet,
            name: "import_candidate_skip_set",
            description: "Set candidate skipped state",
            input: AutomationToolInput::CandidateSkipSet,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportSearch,
            name: "import_search",
            description: "Search metadata sources for import",
            input: AutomationToolInput::SearchQuery,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportReleasePrefetch,
            name: "import_release_prefetch",
            description: "Prefetch a release: display detail plus the metadata editor's seed",
            input: AutomationToolInput::ReleaseSource,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportFileTagsPreview,
            name: "import_file_tags_preview",
            description: "Preview file-tag metadata for a folder",
            input: AutomationToolInput::Folder,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportReleaseEditShape,
            name: "import_release_edit_shape",
            description: "Shape a prefetched editor seed for an identity choice",
            input: AutomationToolInput::ShapeReleaseEdit,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ImportStart,
            name: "import_start",
            description: "Start an import through the core import service",
            input: AutomationToolInput::StartImport,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseDetailGet,
            name: "release_detail_get",
            description: "Get library release detail",
            input: AutomationToolInput::ReleaseId,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseExport,
            name: "release_export",
            description: "Enqueue a byte-accurate export of a release's files to a directory",
            input: AutomationToolInput::ReleaseExport,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ExportStatus,
            name: "export_status",
            description: "Get the export queue snapshot (per-release progress)",
            input: AutomationToolInput::Empty,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseReidentify,
            name: "release_reidentify",
            description: "Set release identity",
            input: AutomationToolInput::ReleaseReidentify,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataReset,
            name: "release_metadata_reset",
            description: "Project release metadata from its source",
            input: AutomationToolInput::ReleaseId,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::ReleaseMetadataUpdate,
            name: "release_metadata_update",
            description: "Apply release metadata edit",
            input: AutomationToolInput::ReleaseMetadataUpdate,
        },
        AutomationToolDescriptor {
            tool: AutomationTool::LibrarySearch,
            name: "library_search",
            description: "Search the library",
            input: AutomationToolInput::LibrarySearch,
        },
    ];

    pub fn all() -> impl Iterator<Item = Self> {
        Self::DESCRIPTORS.iter().map(|descriptor| descriptor.tool)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.name == name)
            .map(|descriptor| descriptor.tool)
    }

    pub fn name(&self) -> &'static str {
        self.descriptor().name
    }

    pub fn description(&self) -> &'static str {
        self.descriptor().description
    }

    pub fn input_schema(&self) -> Map<String, Value> {
        self.descriptor().input.schema()
    }

    pub fn accepts_missing_arguments(&self) -> bool {
        self.descriptor().input.accepts_missing_arguments()
    }

    fn descriptor(&self) -> &'static AutomationToolDescriptor {
        Self::DESCRIPTORS
            .iter()
            .find(|descriptor| descriptor.tool == *self)
            .expect("automation tool descriptor")
    }
}

#[derive(Debug, Clone, Copy)]
struct AutomationToolDescriptor {
    tool: AutomationTool,
    name: &'static str,
    description: &'static str,
    input: AutomationToolInput,
}

#[derive(Debug, Clone, Copy)]
enum AutomationToolInput {
    Empty,
    Path,
    ScanWait,
    CandidateKey,
    CandidateSkipSet,
    SearchQuery,
    ReleaseSource,
    Folder,
    ShapeReleaseEdit,
    StartImport,
    ReleaseId,
    ReleaseExport,
    ReleaseReidentify,
    ReleaseMetadataUpdate,
    LibrarySearch,
}

impl AutomationToolInput {
    fn schema(&self) -> Map<String, Value> {
        match self {
            Self::Empty => empty_input_schema(),
            Self::Path => schema_object::<PathInput>(),
            Self::ScanWait => schema_object::<ScanWait>(),
            Self::CandidateKey => schema_object::<CandidateKeyInput>(),
            Self::CandidateSkipSet => schema_object::<CandidateSkipSetInput>(),
            Self::SearchQuery => schema_object::<AutomationSearchQuery>(),
            Self::ReleaseSource => schema_object::<ReleaseSourceInput>(),
            Self::Folder => schema_object::<FolderInput>(),
            Self::ShapeReleaseEdit => schema_object::<ShapeReleaseEditInput>(),
            Self::StartImport => schema_object::<AutomationStartImport>(),
            Self::ReleaseId => schema_object::<ReleaseIdInput>(),
            Self::ReleaseExport => schema_object::<ReleaseExportInput>(),
            Self::ReleaseReidentify => schema_object::<ReleaseReidentifyInput>(),
            Self::ReleaseMetadataUpdate => schema_object::<ReleaseMetadataUpdateInput>(),
            Self::LibrarySearch => schema_object::<LibrarySearchInput>(),
        }
    }

    fn accepts_missing_arguments(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

fn expect_no_args(args: Value, tool_name: &str) -> Result<(), AutomationError> {
    match args {
        Value::Null => Ok(()),
        Value::Object(map) if map.is_empty() => Ok(()),
        other => Err(AutomationError::validation(format!(
            "tool '{tool_name}' does not accept arguments, got {other}"
        ))),
    }
}

fn from_value<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AutomationError> {
    serde_json::from_value(value).map_err(|e| AutomationError::validation(e.to_string()))
}

fn to_value<T: Serialize>(value: T) -> Result<Value, AutomationError> {
    serde_json::to_value(value).map_err(|e| AutomationError::internal(e.to_string()))
}

/// Wrap a list result under a named key. MCP `structuredContent` must be a JSON
/// object, so a tool returning a bare `Vec` has to nest it under a field.
fn to_list_value<T: Serialize>(key: &str, values: Vec<T>) -> Result<Value, AutomationError> {
    let mut map = Map::new();
    map.insert(key.to_string(), to_value(values)?);
    Ok(Value::Object(map))
}

fn schema_object<T: JsonSchema>() -> Map<String, Value> {
    let value = serde_json::to_value(schemars::schema_for!(T)).expect("serialize JSON schema");
    let mut map = match value {
        Value::Object(map) => map,
        _ => unreachable!("JSON schema is an object"),
    };
    // MCP requires the root inputSchema to declare `type: "object"`. Struct
    // schemas already do; internally-tagged enum schemas emit a root `oneOf`
    // with no root type. Every automation tool input is an object in all
    // variants, so assert it at the root.
    map.entry("type".to_string())
        .or_insert_with(|| Value::String("object".to_string()));
    map
}

fn empty_input_schema() -> Map<String, Value> {
    let mut schema = Map::new();
    schema.insert("type".to_string(), Value::String("object".to_string()));
    schema.insert("properties".to_string(), Value::Object(Map::new()));
    schema
}

fn automation_export_snapshot(
    snapshot: bae_core::library::ExportSnapshot,
) -> AutomationExportSnapshot {
    use bae_core::library::ExportState;
    let exports = snapshot
        .ops
        .into_iter()
        .map(|op| {
            let state = match op.state {
                ExportState::Queued => AutomationExportState::Queued,
                ExportState::Active { progress } => {
                    AutomationExportState::Active { percent: progress }
                }
                ExportState::Failed { error } => AutomationExportState::Failed { error },
            };
            AutomationExportOp {
                release_id: op.release_id,
                target_dir: op.payload.target_dir.to_string_lossy().to_string(),
                title: op.title,
                file_count: op.file_count,
                total_size: op.total_size,
                created_at: op.created_at,
                state,
            }
        })
        .collect();
    AutomationExportSnapshot {
        exports,
        total: AutomationExportProgress {
            queued: snapshot.total.queued,
            active: snapshot.total.active,
            failed: snapshot.total.failed,
        },
        paused: snapshot.paused,
    }
}

fn automation_candidate_from_folder(candidate: FolderCandidate) -> AutomationCandidate {
    let track_count = candidate.files.audio.track_count();
    let format_label = candidate.files.audio.format_label().to_string();
    AutomationCandidate::Valid {
        common: automation_candidate_common(
            candidate.path,
            candidate.name,
            candidate.watched_folder_path,
            candidate.skipped,
            candidate.is_added,
        ),
        track_count,
        format_label,
        content_hash: candidate.files.content_hash(),
    }
}

fn automation_candidate_from_invalid(candidate: InvalidCandidate) -> AutomationCandidate {
    AutomationCandidate::Invalid {
        common: automation_candidate_common(
            candidate.path,
            candidate.name,
            candidate.watched_folder_path,
            true,
            false,
        ),
        invalid_reason: candidate.reason.to_string(),
    }
}

fn automation_candidate_common(
    path: PathBuf,
    name: String,
    watched_folder_path: String,
    skipped: bool,
    is_added: bool,
) -> AutomationCandidateCommon {
    let path = path.to_string_lossy().to_string();
    AutomationCandidateCommon {
        key: path.clone(),
        path,
        name,
        watched_folder_path,
        skipped,
        is_added,
        runtime: None,
    }
}

impl AutomationCandidateCommon {
    fn runtime_mut(&mut self) -> &mut AutomationCandidateRuntime {
        self.runtime
            .get_or_insert_with(AutomationCandidateRuntime::default)
    }
}

fn search_query(query: AutomationSearchQuery) -> SearchQuery {
    match query {
        AutomationSearchQuery::General {
            artist,
            album,
            source,
        } => SearchQuery::General {
            artist,
            album,
            source: source.into(),
        },
        AutomationSearchQuery::CatalogNumber {
            catalog_number,
            source,
        } => SearchQuery::CatalogNumber {
            catalog_number,
            source: source.into(),
        },
        AutomationSearchQuery::Barcode { barcode, source } => SearchQuery::Barcode {
            barcode,
            source: source.into(),
        },
    }
}

fn identity_choice(choice: AutomationIdentityChoice) -> IdentityChoice {
    match choice {
        AutomationIdentityChoice::Exact { source, release_id } => IdentityChoice::Exact {
            release_ref: MetadataRef::new(release_id, source.into()),
        },
        AutomationIdentityChoice::Approximate { source, release_id } => {
            IdentityChoice::Approximate {
                release_ref: MetadataRef::new(release_id, source.into()),
            }
        }
        AutomationIdentityChoice::Unknown => IdentityChoice::Unknown,
    }
}

fn automation_search_results(results: GroupedSearchResults) -> AutomationSearchResults {
    AutomationSearchResults {
        groups: results
            .groups
            .into_iter()
            .map(automation_release_group)
            .collect(),
        statuses: results
            .statuses
            .into_iter()
            .map(automation_library_status)
            .collect(),
    }
}

fn automation_release_group(group: ReleaseGroup) -> AutomationReleaseGroup {
    AutomationReleaseGroup {
        id: group.id,
        title: group.title,
        artist: group.artist,
        cover_art: group.cover_art.map(automation_remote_cover),
        source_label: group.source_label,
        group_url: group.group_url,
        year_min: group.year_min,
        year_max: group.year_max,
        pressings: group
            .pressings
            .into_iter()
            .map(automation_metadata_result)
            .collect(),
    }
}

fn automation_metadata_result(result: MetadataResult) -> AutomationMetadataResult {
    AutomationMetadataResult {
        source: result.source.into(),
        release_id: result.release_id,
        title: result.title,
        artist: result.artist,
        year: result.year,
        format: result.format,
        label: result.label,
        catalog_number: result.catalog_number,
        country: result.country,
        cover_art: result.cover_art.map(automation_remote_cover),
        source_group_id: result.source_group_id,
    }
}

fn automation_library_status(status: LibraryStatus) -> AutomationLibraryStatus {
    AutomationLibraryStatus {
        release_id: status.release_id,
        release_in_library: status.release_in_library,
        album_in_library: status.album_in_library,
        album_title: status.album_title,
        album_id: status.album_id,
    }
}

fn automation_remote_cover(cover: RemoteCover) -> AutomationRemoteCover {
    AutomationRemoteCover {
        url: cover.url,
        thumbnail_url: cover.thumbnail_url,
        label: cover.label,
        source: cover.source.into(),
    }
}

fn automation_release_detail(detail: ImportSearchReleaseDetail) -> AutomationReleaseDetail {
    AutomationReleaseDetail {
        release_id: detail.release_id,
        source: detail.source.into(),
        source_group_id: detail.source_group_id,
        title: detail.title,
        artist: detail.artist,
        year: detail.year,
        format: detail.format,
        label: detail.label,
        catalog_number: detail.catalog_number,
        country: detail.country,
        barcode: detail.barcode,
        track_count: detail.track_count,
        tracks: detail
            .tracks
            .into_iter()
            .map(|track| AutomationReleaseTrack {
                title: track.title,
                artist: track.artist,
                duration_ms: track.duration_ms,
                position: track.position,
                side: track.side,
            })
            .collect(),
        cover_art: detail
            .cover_art
            .into_iter()
            .map(automation_remote_cover)
            .collect(),
    }
}

fn automation_release_user_edit(
    edit: bae_core::import::ReleaseUserEdit,
) -> AutomationReleaseUserEdit {
    AutomationReleaseUserEdit {
        album_title: edit.album_title,
        album_artist_names: edit.album_artist_names,
        pressing: AutomationPressingEdit {
            year: edit.pressing.year,
            format: edit.pressing.format,
            label: edit.pressing.label,
            catalog_number: edit.pressing.catalog_number,
            country: edit.pressing.country,
            barcode: edit.pressing.barcode,
        },
        tracks: edit
            .tracks
            .into_iter()
            .map(|track| AutomationTrackUserEdit {
                title: track.title,
                side: track.side,
                track_number: track.track_number,
                artist_names: track.artist_names,
            })
            .collect(),
    }
}

fn release_user_edit(edit: AutomationReleaseUserEdit) -> bae_core::import::ReleaseUserEdit {
    bae_core::import::ReleaseUserEdit {
        album_title: edit.album_title,
        album_artist_names: edit.album_artist_names,
        pressing: PressingEdit {
            year: edit.pressing.year,
            format: edit.pressing.format,
            label: edit.pressing.label,
            catalog_number: edit.pressing.catalog_number,
            country: edit.pressing.country,
            barcode: edit.pressing.barcode,
        },
        tracks: edit
            .tracks
            .into_iter()
            .map(|track| TrackUserEdit {
                title: track.title,
                side: track.side,
                track_number: track.track_number,
                artist_names: track.artist_names,
            })
            .collect(),
    }
}

fn cover_selection(selection: AutomationCoverSelection) -> CoverSelection {
    match selection {
        AutomationCoverSelection::Remote { url, source } => {
            CoverSelection::Remote(url, source.into())
        }
        AutomationCoverSelection::Local { path } => CoverSelection::Local(path),
    }
}

fn storage_mode(mode: AutomationStorageMode) -> StorageMode {
    match mode {
        AutomationStorageMode::Local => StorageMode::Local,
        AutomationStorageMode::Remote => StorageMode::Remote,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_lookup_failure(failure: bae_core::signals::LookupFailure) -> AutomationLookupFailure {
    use bae_core::signals::LookupFailure;
    match failure {
        LookupFailure::Network => AutomationLookupFailure::Network,
        LookupFailure::Provider { status } => AutomationLookupFailure::Provider { status },
        LookupFailure::Timeout => AutomationLookupFailure::Timeout,
        LookupFailure::ArtworkAnalysis => AutomationLookupFailure::ArtworkAnalysis,
        LookupFailure::Diagnostic { detail } => AutomationLookupFailure::Diagnostic { detail },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_signal_origin(origin: bae_core::signals::SignalOrigin) -> AutomationSignalOrigin {
    use bae_core::signals::SignalOrigin;
    match origin {
        SignalOrigin::DiscToc => AutomationSignalOrigin::DiscToc,
        SignalOrigin::CueSheet => AutomationSignalOrigin::CueSheet,
        SignalOrigin::Artwork => AutomationSignalOrigin::Artwork,
        SignalOrigin::FolderName => AutomationSignalOrigin::FolderName,
        SignalOrigin::Filename => AutomationSignalOrigin::Filename,
        SignalOrigin::TextFile => AutomationSignalOrigin::TextFile,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_sourced_value(value: bae_core::signals::SourcedValue) -> AutomationSourcedValue {
    AutomationSourcedValue {
        value: value.value,
        origin: automation_signal_origin(value.origin),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_sourced_values(
    values: Vec<bae_core::signals::SourcedValue>,
) -> Vec<AutomationSourcedValue> {
    values.into_iter().map(automation_sourced_value).collect()
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_disc_id_signal(signal: bae_core::signals::DiscIdSignal) -> AutomationDiscIdSignal {
    use bae_core::signals::DiscIdSignal;
    match signal {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
        } => AutomationDiscIdSignal::Computed {
            disc_id,
            track_count,
        },
        DiscIdSignal::Absent { track_count } => AutomationDiscIdSignal::Absent { track_count },
        DiscIdSignal::Failed {
            failure,
            track_count,
        } => AutomationDiscIdSignal::Failed {
            failure: automation_lookup_failure(failure),
            track_count,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_barcode_signal(signal: bae_core::signals::BarcodeSignal) -> AutomationBarcodeSignal {
    use bae_core::signals::BarcodeSignal;
    match signal {
        BarcodeSignal::Scanning { codes } => AutomationBarcodeSignal::Scanning {
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Settled { codes } => AutomationBarcodeSignal::Settled {
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Failed { failure, codes } => AutomationBarcodeSignal::Failed {
            failure: automation_lookup_failure(failure),
            codes: automation_sourced_values(codes),
        },
        BarcodeSignal::Absent => AutomationBarcodeSignal::Absent,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_text_signal(signal: bae_core::signals::TextSignal) -> AutomationTextSignal {
    use bae_core::signals::TextSignal;
    match signal {
        TextSignal::Scanning {
            catalogs,
            free_text,
        } => AutomationTextSignal::Scanning {
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
        TextSignal::Settled {
            catalogs,
            free_text,
        } => AutomationTextSignal::Settled {
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
        TextSignal::Failed {
            failure,
            catalogs,
            free_text,
        } => AutomationTextSignal::Failed {
            failure: automation_lookup_failure(failure),
            catalogs: automation_sourced_values(catalogs),
            free_text,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_signals(signals: bae_core::signals::Signals) -> AutomationSignals {
    AutomationSignals {
        disc_id: automation_disc_id_signal(signals.disc_id),
        barcode: automation_barcode_signal(signals.barcode),
        text: automation_text_signal(signals.text),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_signal_kind(kind: bae_core::identify::SignalKind) -> AutomationSignalKind {
    use bae_core::identify::SignalKind;
    match kind {
        SignalKind::DiscId => AutomationSignalKind::DiscId,
        SignalKind::Barcode => AutomationSignalKind::Barcode,
        SignalKind::Catalog => AutomationSignalKind::Catalog,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_signal_role(role: bae_core::identify::SignalRole) -> AutomationSignalRole {
    use bae_core::identify::SignalRole;
    match role {
        SignalRole::Identity => AutomationSignalRole::Identity,
        SignalRole::Filter => AutomationSignalRole::Filter,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_signal_state(state: bae_core::identify::SignalState) -> AutomationSignalState {
    use bae_core::identify::SignalState;
    match state {
        SignalState::LookingUp => AutomationSignalState::LookingUp,
        SignalState::Found { count } => AutomationSignalState::Found { count },
        SignalState::NoMatch => AutomationSignalState::NoMatch,
        SignalState::Skipped => AutomationSignalState::Skipped,
        SignalState::Failed { failure } => AutomationSignalState::Failed {
            failure: automation_lookup_failure(failure),
        },
        SignalState::Confirms { count } => AutomationSignalState::Confirms { count },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_toolbar_signal(signal: bae_core::identify::ToolbarSignal) -> AutomationToolbarSignal {
    AutomationToolbarSignal {
        kind: automation_signal_kind(signal.kind),
        role: automation_signal_role(signal.role),
        value: signal.value,
        origin: automation_signal_origin(signal.origin),
        state: automation_signal_state(signal.state),
        excluded: signal.excluded,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_discid_progress(
    progress: bae_core::identify::DiscidProgressView,
) -> AutomationDiscidProgress {
    use bae_core::identify::DiscidProgressView;
    match progress {
        DiscidProgressView::Computing => AutomationDiscidProgress::Computing,
        DiscidProgressView::LookingUp => AutomationDiscidProgress::LookingUp,
        DiscidProgressView::Done { n_results } => AutomationDiscidProgress::Done { n_results },
        DiscidProgressView::Skipped => AutomationDiscidProgress::Skipped,
        DiscidProgressView::Failed { failure } => AutomationDiscidProgress::Failed {
            failure: automation_lookup_failure(failure),
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_barcode_progress(
    progress: bae_core::identify::BarcodeProgressView,
) -> AutomationBarcodeProgress {
    use bae_core::identify::BarcodeProgressView;
    match progress {
        BarcodeProgressView::Scanning => AutomationBarcodeProgress::Scanning,
        BarcodeProgressView::LookingUp {
            current,
            position,
            total,
        } => AutomationBarcodeProgress::LookingUp {
            current,
            position,
            total,
        },
        BarcodeProgressView::Done { n_results } => AutomationBarcodeProgress::Done { n_results },
        BarcodeProgressView::Failed { failure } => AutomationBarcodeProgress::Failed {
            failure: automation_lookup_failure(failure),
        },
        BarcodeProgressView::Skipped => AutomationBarcodeProgress::Skipped,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_identify_source(
    source: bae_core::identify::IdentifySource,
) -> AutomationIdentifySource {
    use bae_core::identify::IdentifySource;
    match source {
        IdentifySource::Discid => AutomationIdentifySource::Discid,
        IdentifySource::Barcode => AutomationIdentifySource::Barcode,
        IdentifySource::Combined => AutomationIdentifySource::Combined,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_result_provenance(
    release_id: String,
    provenance: bae_core::identify::ResultProvenance,
) -> AutomationResultProvenance {
    let bae_core::identify::ResultProvenance {
        by_disc_id,
        by_barcode,
        matches_catalog,
    } = provenance;
    AutomationResultProvenance {
        release_id,
        by_disc_id,
        by_barcode,
        matches_catalog,
    }
}

/// Unzip core's paired rows into the two parallel lists the JSON surface carries,
/// each record keeping its own `release_id`.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_results_and_statuses(
    rows: Vec<bae_core::identify::ResultRow>,
) -> (Vec<AutomationMetadataResult>, Vec<AutomationLibraryStatus>) {
    rows.into_iter()
        .map(|bae_core::identify::ResultRow { result, status }| {
            (
                automation_metadata_result(result),
                automation_library_status(status),
            )
        })
        .unzip()
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the JSON enum. Core has
/// already folded the matches into their group, keyed the provenance, reduced the
/// in-flight payloads to counts, and dropped what must not cross — this is a field
/// copy per variant and nothing else.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn automation_identify_state(state: bae_core::identify::IdentifyState) -> AutomationIdentifyState {
    use bae_core::identify::IdentifyStateView;
    match IdentifyStateView::from(state) {
        IdentifyStateView::Idle => AutomationIdentifyState::Idle,
        IdentifyStateView::Triangulating { discid, barcode } => {
            AutomationIdentifyState::Triangulating {
                discid: automation_discid_progress(discid),
                barcode: automation_barcode_progress(barcode),
            }
        }
        IdentifyStateView::Found {
            group,
            library_statuses,
            track_count,
            source,
            provenance,
        } => AutomationIdentifyState::Found {
            group: automation_release_group(group),
            library_statuses: library_statuses
                .into_iter()
                .map(automation_library_status)
                .collect(),
            track_count,
            source: automation_identify_source(source),
            provenance: provenance
                .into_iter()
                .map(|(release_id, p)| automation_result_provenance(release_id, p))
                .collect(),
        },
        IdentifyStateView::Conflict {
            discid_results,
            barcode_results,
            // The disc-id section header names its source on the desktop UI. The
            // JSON surface doesn't carry a header, and each result already states
            // its own `source`, so a client that wants the label reads it there.
            matched_barcode,
            track_count,
        } => {
            let (discid_results, discid_library_statuses) =
                automation_results_and_statuses(discid_results);
            let (barcode_results, barcode_library_statuses) =
                automation_results_and_statuses(barcode_results);
            AutomationIdentifyState::Conflict {
                discid_results,
                discid_library_statuses,
                barcode_results,
                barcode_library_statuses,
                matched_barcode,
                track_count,
            }
        }
        IdentifyStateView::NotFoundAnywhere => AutomationIdentifyState::NotFoundAnywhere,
        IdentifyStateView::ManualOnly { track_count } => {
            AutomationIdentifyState::ManualOnly { track_count }
        }
    }
}

fn automation_prepare_step(step: PrepareStep) -> AutomationPrepareStep {
    match step {
        PrepareStep::ParsingMetadata => AutomationPrepareStep::ParsingMetadata,
        PrepareStep::WritingCoverArt => AutomationPrepareStep::WritingCoverArt,
        PrepareStep::DiscoveringFiles => AutomationPrepareStep::DiscoveringFiles,
        PrepareStep::ValidatingTracks => AutomationPrepareStep::ValidatingTracks,
        PrepareStep::SavingToDatabase => AutomationPrepareStep::SavingToDatabase,
    }
}

fn automation_import_phase(phase: ImportPhase) -> AutomationImportPhase {
    match phase {
        ImportPhase::ReferencingFiles => AutomationImportPhase::ReferencingFiles,
        ImportPhase::MeasuringLoudness => AutomationImportPhase::MeasuringLoudness,
        ImportPhase::Finalizing => AutomationImportPhase::Finalizing,
    }
}

fn automation_import_progress(progress: ImportProgress) -> AutomationImportProgress {
    match progress {
        ImportProgress::Preparing {
            import_id,
            step,
            album_title,
            artist_name,
        } => AutomationImportProgress::Preparing {
            import_id,
            step: automation_prepare_step(step),
            album_title,
            artist_name,
        },
        ImportProgress::Started { id, import_id } => {
            AutomationImportProgress::Started { id, import_id }
        }
        ImportProgress::Progress {
            id,
            percent,
            phase,
            import_id,
        } => AutomationImportProgress::Progress {
            id,
            percent,
            phase: automation_import_phase(phase),
            import_id,
        },
        ImportProgress::Complete {
            id,
            import_id,
            album_id,
        } => AutomationImportProgress::Complete {
            id,
            import_id,
            album_id,
        },
        ImportProgress::RemoteUploadQueued {
            id,
            import_id,
            album_id,
        } => AutomationImportProgress::RemoteUploadQueued {
            id,
            import_id,
            album_id,
        },
        ImportProgress::Failed { error, import_id } => {
            AutomationImportProgress::Failed { error, import_id }
        }
    }
}

fn automation_release(release: ReleaseDetail) -> AutomationRelease {
    AutomationRelease {
        summary: automation_release_summary(release.summary),
        display_name: release.display_name,
        year: release.year,
        label: release.label,
        catalog_number: release.catalog_number,
        country: release.country,
        total_duration_ms: release.total_duration_ms,
        tracks: release
            .tracks
            .into_iter()
            .map(automation_track_detail)
            .collect(),
        track_groups: release
            .track_groups
            .into_iter()
            .map(|group| AutomationTrackGroup {
                side: automation_track_side(group.side),
                tracks: group
                    .tracks
                    .into_iter()
                    .map(automation_track_detail)
                    .collect(),
            })
            .collect(),
        files: release
            .files
            .into_iter()
            .map(automation_file_detail)
            .collect(),
        image_files: release
            .image_files
            .into_iter()
            .map(automation_file_detail)
            .collect(),
        gallery_items: release
            .gallery_items
            .into_iter()
            .map(automation_gallery_item)
            .collect(),
    }
}

fn automation_release_summary(
    summary: bae_core::album_detail::ReleaseSummary,
) -> AutomationReleaseSummary {
    AutomationReleaseSummary {
        id: summary.id,
        album_id: summary.album_id,
        format: summary.format,
        storage_state: release_storage_state(summary.storage_state).to_string(),
        pinned: summary.pinned,
        storage_actions: summary
            .storage_actions
            .into_iter()
            .map(release_storage_action)
            .map(str::to_string)
            .collect(),
        file_count: summary.file_count,
        total_size: summary.total_size,
        cover: summary.cover.map(automation_image_ref),
    }
}

fn release_storage_state(state: ReleaseStorageState) -> &'static str {
    match state {
        ReleaseStorageState::Local => "local",
        ReleaseStorageState::Remote => "remote",
    }
}

fn release_storage_action(action: ReleaseStorageAction) -> &'static str {
    match action {
        ReleaseStorageAction::MakeRemote => "make_remote",
        ReleaseStorageAction::Pin => "pin",
        ReleaseStorageAction::Unpin => "unpin",
        ReleaseStorageAction::MakeLocal => "make_local",
    }
}

fn automation_image_ref(image: ImageRef) -> AutomationImageRef {
    AutomationImageRef {
        id: image.id,
        version: image.version,
    }
}

fn automation_track_detail(track: TrackDetail) -> AutomationTrackDetail {
    AutomationTrackDetail {
        id: track.id,
        title: track.title,
        side: track.side,
        track_number: track.track_number,
        duration_ms: track.duration_ms,
        artist_names: track.artist_names,
        position_text: track.position_text,
        position: automation_track_position(track.position),
    }
}

fn automation_track_position(position: TrackPosition) -> AutomationTrackPosition {
    match position {
        TrackPosition::Sided {
            side_letter,
            number,
        } => AutomationTrackPosition::Sided {
            side_letter,
            number,
        },
        TrackPosition::SidedUnnumbered { side_letter } => {
            AutomationTrackPosition::SidedUnnumbered { side_letter }
        }
        TrackPosition::Disc { disc, number } => AutomationTrackPosition::Disc { disc, number },
        TrackPosition::DiscUnnumbered { disc } => AutomationTrackPosition::DiscUnnumbered { disc },
        TrackPosition::Flat { number } => AutomationTrackPosition::Flat { number },
        TrackPosition::Unnumbered => AutomationTrackPosition::Unnumbered,
    }
}

fn automation_track_side(side: TrackSide) -> AutomationTrackSide {
    match side {
        TrackSide::Sided { side_letter } => AutomationTrackSide::Sided { side_letter },
        TrackSide::Disc { disc } => AutomationTrackSide::Disc { disc },
        TrackSide::Flat => AutomationTrackSide::Flat,
    }
}

fn automation_file_detail(file: FileDetail) -> AutomationFileDetail {
    AutomationFileDetail {
        id: file.id,
        original_filename: file.original_filename,
        file_size: file.file_size,
        is_image: file.is_image,
        content_type: file.content_type,
        audio_format: file.audio_format.map(automation_audio_format),
    }
}

fn automation_audio_format(format: AudioFormat) -> AutomationAudioFormat {
    AutomationAudioFormat {
        codec: format.codec,
        sample_rate_hz: format.sample_rate_hz,
        bits_per_sample: format.bits_per_sample,
        bitrate_kbps: format.bitrate_kbps,
        channels: format.channels,
    }
}

fn automation_gallery_item(item: GalleryItem) -> AutomationGalleryItem {
    AutomationGalleryItem {
        id: item.id,
        label: item.label,
        source: match item.source {
            GallerySource::Cover(image) => AutomationGallerySource::Cover {
                image: automation_image_ref(image),
            },
            GallerySource::ReleaseFile { file_id } => {
                AutomationGallerySource::ReleaseFile { file_id }
            }
        },
    }
}

fn automation_library_search_results(results: SearchResults) -> AutomationLibrarySearchResults {
    AutomationLibrarySearchResults {
        albums: results
            .albums
            .into_iter()
            .map(|album| AutomationAlbumSearchResult {
                id: album.id,
                title: album.title,
                year: album.year,
                artist_name: album.artist_name,
                cover: album.cover.map(automation_image_ref),
            })
            .collect(),
        tracks: results
            .tracks
            .into_iter()
            .map(|track| AutomationTrackSearchResult {
                id: track.id,
                title: track.title,
                duration_ms: track.duration_ms,
                album_id: track.album_id,
                album_title: track.album_title,
                artist_name: track.artist_name,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_input_schema_has_root_object_type() {
        for tool in AutomationTool::all() {
            let schema = tool.input_schema();
            assert_eq!(
                schema.get("type").and_then(Value::as_str),
                Some("object"),
                "tool {} inputSchema must have root type object",
                tool.name(),
            );
        }
    }

    #[test]
    fn import_progress_step_and_phase_serialize_snake_case() {
        let preparing = automation_import_progress(ImportProgress::Preparing {
            import_id: "imp-1".to_string(),
            step: PrepareStep::ParsingMetadata,
            album_title: "Album Title".to_string(),
            artist_name: "Artist Name".to_string(),
        });
        let json = serde_json::to_value(preparing).unwrap();
        assert_eq!(json["kind"], "preparing");
        assert_eq!(json["step"], "parsing_metadata");

        let progress = automation_import_progress(ImportProgress::Progress {
            id: "id-1".to_string(),
            percent: 50,
            phase: ImportPhase::ReferencingFiles,
            import_id: "imp-1".to_string(),
        });
        let json = serde_json::to_value(progress).unwrap();
        assert_eq!(json["kind"], "progress");
        assert_eq!(json["phase"], "referencing_files");
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    mod identify_mirrors {
        use super::*;
        use bae_core::db::LibraryStatus;
        use bae_core::identify::combine::{GroupKey, ResultProvenance};
        use bae_core::identify::state::SignalsContext;
        use bae_core::identify::{
            BarcodeProgress, DiscidProgress, IdentifySource, IdentifyState, SignalKind, SignalRole,
            SignalState, ToolbarSignal,
        };
        use bae_core::import::search::MetadataResult;
        use bae_core::import::MetadataSource;
        use bae_core::signals::{
            BarcodeSignal, DiscIdSignal, LookupFailure, SignalOrigin, Signals, SourcedValue,
            TextSignal,
        };
        use std::collections::HashSet;

        fn metadata_result(release_id: &str, group_id: &str) -> MetadataResult {
            MetadataResult {
                source: MetadataSource::MusicBrainz,
                release_id: release_id.to_string(),
                title: "Album Title".to_string(),
                artist: Some("Artist Name".to_string()),
                year: Some(1999),
                format: Some("CD".to_string()),
                label: Some("Label Name".to_string()),
                catalog_number: Some("CAT-1".to_string()),
                country: Some("US".to_string()),
                cover_art: None,
                source_group_id: Some(group_id.to_string()),
            }
        }

        fn library_status(release_id: &str) -> LibraryStatus {
            LibraryStatus {
                release_id: release_id.to_string(),
                release_in_library: false,
                album_in_library: false,
                album_title: None,
                album_id: None,
            }
        }

        fn empty_context() -> SignalsContext {
            SignalsContext {
                disc_id: DiscIdSignal::Absent { track_count: 0 },
                barcode_codes: Vec::new(),
                had_barcode_source: false,
                catalogs: Vec::new(),
                excluded: HashSet::new(),
                discid_results: Vec::new(),
                barcode_results: Vec::new(),
                barcode_failure: None,
                matched_barcode: None,
                track_count: 0,
            }
        }

        #[test]
        fn found_state_aligns_provenance_and_pressings_by_release_id() {
            let matches = vec![
                metadata_result("rel-1", "group-1"),
                metadata_result("rel-2", "group-1"),
            ];
            let state = IdentifyState::Found {
                matches: matches.clone(),
                library_statuses: vec![library_status("rel-1"), library_status("rel-2")],
                track_count: 12,
                group: GroupKey {
                    source: MetadataSource::MusicBrainz,
                    source_group_id: "group-1".to_string(),
                },
                source: IdentifySource::Combined,
                provenance: vec![
                    ResultProvenance {
                        by_disc_id: true,
                        by_barcode: false,
                        matches_catalog: false,
                    },
                    ResultProvenance {
                        by_disc_id: false,
                        by_barcode: true,
                        matches_catalog: true,
                    },
                ],
                context: empty_context(),
            };

            let json = serde_json::to_value(automation_identify_state(state)).unwrap();
            assert_eq!(json["kind"], "found");
            assert_eq!(json["source"], "combined");
            let pressings = json["group"]["pressings"].as_array().unwrap();
            assert_eq!(pressings[0]["release_id"], "rel-1");
            assert_eq!(pressings[1]["release_id"], "rel-2");
            let provenance = json["provenance"].as_array().unwrap();
            assert_eq!(provenance[0]["release_id"], "rel-1");
            assert_eq!(provenance[0]["by_disc_id"], true);
            assert_eq!(provenance[1]["release_id"], "rel-2");
            assert_eq!(provenance[1]["by_barcode"], true);
            assert_eq!(provenance[1]["matches_catalog"], true);
            let statuses = json["library_statuses"].as_array().unwrap();
            assert_eq!(statuses[0]["release_id"], "rel-1");
            assert_eq!(statuses[1]["release_id"], "rel-2");
        }

        #[test]
        fn conflict_state_splits_results_and_statuses_per_signal() {
            let mut context = empty_context();
            context.discid_results = vec![(
                metadata_result("rel-disc", "g-d"),
                library_status("rel-disc"),
            )];
            context.barcode_results =
                vec![(metadata_result("rel-bar", "g-b"), library_status("rel-bar"))];
            context.matched_barcode = Some("0123456789012".to_string());
            context.track_count = 9;
            let state = IdentifyState::Conflict { context };

            let json = serde_json::to_value(automation_identify_state(state)).unwrap();
            assert_eq!(json["kind"], "conflict");
            assert_eq!(json["discid_results"][0]["release_id"], "rel-disc");
            assert_eq!(json["discid_library_statuses"][0]["release_id"], "rel-disc");
            assert_eq!(json["barcode_results"][0]["release_id"], "rel-bar");
            assert_eq!(json["barcode_library_statuses"][0]["release_id"], "rel-bar");
            assert_eq!(json["matched_barcode"], "0123456789012");
            assert_eq!(json["track_count"], 9);
        }

        #[test]
        fn triangulating_barcode_looking_up_drops_remaining() {
            let state = IdentifyState::Triangulating {
                discid: DiscidProgress::LookingUp,
                barcode: BarcodeProgress::LookingUp {
                    current: "0123456789012".to_string(),
                    position: 2,
                    total: 3,
                    remaining: vec!["9999999999999".to_string()],
                },
                context: empty_context(),
            };

            let json = serde_json::to_value(automation_identify_state(state)).unwrap();
            assert_eq!(json["kind"], "triangulating");
            assert_eq!(json["discid"]["kind"], "looking_up");
            assert_eq!(json["barcode"]["kind"], "looking_up");
            assert_eq!(json["barcode"]["current"], "0123456789012");
            assert_eq!(json["barcode"]["position"], 2);
            assert_eq!(json["barcode"]["total"], 3);
            assert!(json["barcode"].get("remaining").is_none());
        }

        #[test]
        fn toolbar_signal_maps_snake_case_and_structured_failure() {
            let signal = ToolbarSignal {
                kind: SignalKind::DiscId,
                role: SignalRole::Identity,
                value: Some("disc-hash".to_string()),
                origin: SignalOrigin::DiscToc,
                state: SignalState::Failed {
                    failure: LookupFailure::Provider { status: Some(503) },
                },
                excluded: false,
            };

            let json = serde_json::to_value(automation_toolbar_signal(signal)).unwrap();
            assert_eq!(json["kind"], "disc_id");
            assert_eq!(json["role"], "identity");
            assert_eq!(json["origin"], "disc_toc");
            assert_eq!(json["state"]["kind"], "failed");
            assert_eq!(json["state"]["failure"]["kind"], "provider");
            assert_eq!(json["state"]["failure"]["status"], 503);
        }

        #[test]
        fn signals_map_all_three_subsignals() {
            let signals = Signals {
                disc_id: DiscIdSignal::Computed {
                    disc_id: "disc-hash".to_string(),
                    track_count: 10,
                },
                barcode: BarcodeSignal::Settled {
                    codes: vec![SourcedValue::new(
                        "0123456789012".to_string(),
                        SignalOrigin::Artwork,
                    )],
                },
                text: TextSignal::Settled {
                    catalogs: vec![SourcedValue::new(
                        "CAT-1".to_string(),
                        SignalOrigin::CueSheet,
                    )],
                    free_text: vec!["Album Title".to_string()],
                },
            };

            let json = serde_json::to_value(automation_signals(signals)).unwrap();
            assert_eq!(json["disc_id"]["kind"], "computed");
            assert_eq!(json["disc_id"]["disc_id"], "disc-hash");
            assert_eq!(json["disc_id"]["track_count"], 10);
            assert_eq!(json["barcode"]["kind"], "settled");
            assert_eq!(json["barcode"]["codes"][0]["value"], "0123456789012");
            assert_eq!(json["barcode"]["codes"][0]["origin"], "artwork");
            assert_eq!(json["text"]["kind"], "settled");
            assert_eq!(json["text"]["catalogs"][0]["value"], "CAT-1");
            assert_eq!(json["text"]["catalogs"][0]["origin"], "cue_sheet");
            assert_eq!(json["text"]["free_text"][0], "Album Title");
        }
    }
}
