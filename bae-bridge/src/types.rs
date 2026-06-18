#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiscogsTokenStatus {
    NotConfigured,
    Valid,
    Unvalidated,
    Rejected,
}

/// What `save_discogs_token` did with a submitted key. The UI receives the
/// failure mode as a typed value, not a formatted string, so it can decide
/// whether to keep the draft (`Rejected`) or clear it. Desktop-only: the import
/// service that writes the key doesn't run on mobile.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl From<bae_core::import::DiscogsSaveOutcome> for BridgeDiscogsSaveOutcome {
    fn from(outcome: bae_core::import::DiscogsSaveOutcome) -> Self {
        use bae_core::import::DiscogsSaveOutcome;
        match outcome {
            DiscogsSaveOutcome::Valid => Self::Valid,
            DiscogsSaveOutcome::Unvalidated => Self::Unvalidated,
            DiscogsSaveOutcome::Rejected => Self::Rejected,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMetadataSource {
    MusicBrainz,
    Discogs,
}

impl BridgeMetadataSource {
    pub fn to_core(self) -> bae_core::import::MetadataSource {
        match self {
            BridgeMetadataSource::MusicBrainz => bae_core::import::MetadataSource::MusicBrainz,
            BridgeMetadataSource::Discogs => bae_core::import::MetadataSource::Discogs,
        }
    }

    pub fn from_core(source: bae_core::import::MetadataSource) -> Self {
        match source {
            bae_core::import::MetadataSource::MusicBrainz => BridgeMetadataSource::MusicBrainz,
            bae_core::import::MetadataSource::Discogs => BridgeMetadataSource::Discogs,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeCloudProvider {
    S3,
    GoogleDrive,
    Dropbox,
    OneDrive,
    CloudKit,
}

/// How a cloud home stores its objects, chosen when connecting the home.
/// `Opaque` encrypts every object at rest under the library key and uses
/// obfuscated, content-addressed keys; `Browsable` stores objects in the clear
/// at readable paths. This is not access control — the provider's own
/// credentials gate the bucket either way; it is only about whether what's
/// stored is legible to someone who already has bucket access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeHomeStorage {
    Opaque,
    Browsable,
}

/// A library known to bae on this device. A library is created here or restored
/// onto this device from another of the owner's devices; every device holds the
/// same single-owner library with full read/write, stored at `path`. `is_active`
/// marks the currently-opened library so the sidebar can highlight it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibrary {
    pub id: String,
    pub name: String,
    pub path: String,
    /// Short label naming the library's cloud provider for display
    /// ("S3-compatible", "iCloud", …), or "Local only" when it syncs nowhere.
    /// Pre-built in core so the UI renders it directly.
    pub cloud_provider_label: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    /// Comma-joined artist names for display
    pub artist_names: String,
    /// All release IDs for this album, ordered by created_at
    pub release_ids: Vec<String>,
    /// Canonical release. Provides the album's cover art; default playback target.
    /// User-settable via `set_album_primary_release`. Falls back to the first
    /// release when unset in the DB. Always set: every album has at least one
    /// release.
    pub primary_release_id: String,
    /// Cache-bustable identifier for the album's cover (`<path>#v=<mtime>`), or
    /// `None` when no cover is cached on disk. Carried on the summary so a cover
    /// change moves a field and the UI re-renders; the image loader strips the
    /// `#v=…` suffix before opening the file.
    pub cover_path: Option<String>,
}

/// Where a release's files live. Mirrors
/// `bae_core::album_detail::ReleaseStorageState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageState {
    Unmanaged,
    Pinned,
    CloudOnly,
}

impl BridgeReleaseStorageState {
    pub fn from_core(state: bae_core::album_detail::ReleaseStorageState) -> Self {
        use bae_core::album_detail::ReleaseStorageState;
        match state {
            ReleaseStorageState::Unmanaged => Self::Unmanaged,
            ReleaseStorageState::Pinned => Self::Pinned,
            ReleaseStorageState::CloudOnly => Self::CloudOnly,
        }
    }
}

/// A storage transition available from the release "Storage…" sheet.
/// Mirrors `bae_core::album_detail::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageAction {
    Manage,
    Pin,
    Unpin,
    Unmanage,
}

impl BridgeReleaseStorageAction {
    pub fn from_core(action: bae_core::album_detail::ReleaseStorageAction) -> Self {
        use bae_core::album_detail::ReleaseStorageAction;
        match action {
            ReleaseStorageAction::Manage => Self::Manage,
            ReleaseStorageAction::Pin => Self::Pin,
            ReleaseStorageAction::Unpin => Self::Unpin,
            ReleaseStorageAction::Unmanage => Self::Unmanage,
        }
    }
}

/// Slim per-release summary: the projection list views render one row
/// per release (storage manager, release pickers, etc.). The fat
/// sibling is `BridgeRelease` — composition at the resolver layer in
/// bae-core; the bridge mirrors each half as its own type so UI
/// consumers can populate separate stores for summary vs. detail.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseSummary {
    pub id: String,
    pub album_id: String,
    pub format: Option<String>,
    /// Where this release's files live.
    pub storage_state: BridgeReleaseStorageState,
    /// Storage transitions available right now, gated on cloud-home by the
    /// core. The in-flight-uploads gate lives in the UI: it consults the
    /// outbox snapshot's `per_release` map before showing actions. The Storage
    /// Manager row context menu renders these.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    /// Pre-formatted total size, e.g. "350 MB".
    pub total_size_label: String,
    /// Cache-bustable identifier for this release's own cover
    /// (`<path>#v=<mtime>`), or `None` when no cover is cached. Keyed on the
    /// release id so each release renders its own art; the image loader strips
    /// the `#v=…` suffix before opening the file.
    pub cover_path: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRelease {
    pub id: String,
    pub album_id: String,
    pub display_name: String,
    pub release_name: Option<String>,
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    /// Compact pressing-metadata line: whichever of `year`, `format`,
    /// `label`, `catalog_number`, `country` are set, joined by ` · `.
    /// Empty when none are set. The UI renders this as-is.
    pub compact_metadata: String,
    /// Where this release's files live.
    pub storage_state: BridgeReleaseStorageState,
    /// Storage transitions available right now, gated on cloud-home by the
    /// core. The in-flight-uploads gate lives in the UI: it consults the
    /// outbox snapshot's `per_release` map before showing actions.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    pub tracks: Vec<BridgeTrack>,
    pub track_groups: Vec<BridgeTrackGroup>,
    pub files: Vec<BridgeFile>,
    pub image_files: Vec<BridgeFile>,
    /// Cover first (if on disk), then every image file the release has —
    /// including cloud-only ones, which carry no local path and are fetched on
    /// demand. Consumers render this as-is.
    pub gallery_items: Vec<BridgeGalleryItem>,
    pub total_duration_label: String,
    pub file_count: i64,
    pub total_size: i64,
    /// Pre-formatted total size, e.g. "350 MB".
    pub total_size_label: String,
    /// Cache-bustable identifier for this release's own cover
    /// (`<path>#v=<mtime>`), or `None` when no cover is cached. Mirrors
    /// `BridgeReleaseSummary.cover_path` so a summary rebuilt from this fat
    /// payload keeps its per-release art.
    pub cover_path: Option<String>,
}

/// User's identity claim from the import flow. Mirrors
/// `bae_core::import::IdentityChoice` — Exact / Approximate carry the
/// picked release reference; Unknown carries none (the worker seeds
/// from embedded file tags). Carried on the import command; the
/// commit pipeline post-processes the mapper's identity vec to NULL
/// out `source_release_id` when Approximate.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentityChoice {
    Exact {
        release_id: String,
        source: BridgeMetadataSource,
    },
    Approximate {
        release_id: String,
        source: BridgeMetadataSource,
    },
    Unknown,
}

impl BridgeIdentityChoice {
    pub fn to_core(self) -> bae_core::import::IdentityChoice {
        match self {
            Self::Exact { release_id, source } => bae_core::import::IdentityChoice::Exact {
                release_ref: bae_core::import::MetadataRef::new(release_id, source.to_core()),
            },
            Self::Approximate { release_id, source } => {
                bae_core::import::IdentityChoice::Approximate {
                    release_ref: bae_core::import::MetadataRef::new(release_id, source.to_core()),
                }
            }
            Self::Unknown => bae_core::import::IdentityChoice::Unknown,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackGroup {
    pub side_label: String,
    pub tracks: Vec<BridgeTrack>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrack {
    pub id: String,
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Pre-formatted duration, e.g. "3:07". Empty if duration is None.
    pub duration_label: String,
    /// Effective comma-joined artist names for display (the track's own
    /// artists when it has per-track artist rows, otherwise the album
    /// artists). Always populated.
    pub artist_names: String,
    /// Human-readable side label: "Side A", "Disc 2", or empty for single-side digital
    pub side_label: String,
    /// Human-readable track position: "A1", "1", "1-2", etc.
    pub position_label: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFile {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    /// Pre-formatted file size, e.g. "35 MB".
    pub file_size_label: String,
    pub content_type: String,
    pub is_image: bool,
    /// Audio-format descriptor, e.g. "FLAC · 44.1 kHz · 16-bit · stereo".
    /// `None` for non-audio files. The UI renders it as-is.
    pub audio_format_label: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeGalleryItem {
    /// Stable identifier: "cover" for the release cover, or the file id. For a
    /// cloud-only image, the file id the lightbox passes to `fetch_gallery_image`.
    pub id: String,
    /// Display label: "Cover" or the file's original filename.
    pub label: String,
    /// Absolute local path when the image is on disk; `None` for a cloud-only
    /// image file not downloaded here — the lightbox fetches its bytes on demand.
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumDetail {
    pub album: BridgeAlbum,
    pub releases: Vec<BridgeRelease>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRepeatMode {
    None,
    Track,
    Album,
}

impl BridgeRepeatMode {
    pub fn to_core(self) -> bae_core::playback::RepeatMode {
        match self {
            Self::None => bae_core::playback::RepeatMode::None,
            Self::Track => bae_core::playback::RepeatMode::Track,
            Self::Album => bae_core::playback::RepeatMode::Album,
        }
    }

    pub fn from_core(mode: bae_core::playback::RepeatMode) -> Self {
        match mode {
            bae_core::playback::RepeatMode::None => Self::None,
            bae_core::playback::RepeatMode::Track => Self::Track,
            bae_core::playback::RepeatMode::Album => Self::Album,
        }
    }
}

/// The target track's display metadata, carried by a loading state once core
/// has resolved it. Mirror of `bae_core::playback::LoadingTrack` across the
/// uniffi boundary.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLoadingTrackInfo {
    pub track_title: String,
    pub artist_names: String,
    pub album_id: String,
    pub album_title: String,
    pub cover_image_id: Option<String>,
    pub duration_ms: u64,
    pub duration_label: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackState {
    Stopped,
    Loading {
        track_id: String,
        /// The target track's metadata, once resolved. `None` until core's
        /// prepare step completes.
        track: Option<BridgeLoadingTrackInfo>,
    },
    Playing {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
        duration_label: String,
    },
    Paused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
        duration_label: String,
    },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePreviewState {
    Idle,
    Playing {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
    Paused {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRemoteCover {
    pub cover_choice: BridgeCoverChoice,
    pub label: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRemoteCoverSelection {
    pub url: String,
    pub source: BridgeMetadataSource,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverImageSource {
    Remote { url: String },
    Local { path: String },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverSelection {
    ReleaseImage {
        file_id: String,
    },
    RemoteCover {
        selection: BridgeRemoteCoverSelection,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverChoice {
    pub selection: BridgeCoverSelection,
    pub preview_source: BridgeCoverImageSource,
    pub thumbnail_source: BridgeCoverImageSource,
}

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
    /// Why the folder failed validation, ready to render next to the warning
    /// icon (e.g. "corrupt or zero-byte audio file: 01.flac").
    pub reason: String,
}

/// A folder the user watches for imports — one candidate-list group.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWatchedFolder {
    /// Absolute path of the watched folder.
    pub path: String,
    /// Final path component — the group header label.
    pub name: String,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl BridgeWatchedFolder {
    pub fn from_core(folder: bae_core::import::WatchedFolder) -> Self {
        Self {
            path: folder.path,
            name: folder.name,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFileInfo {
    pub name: String,
    pub size: u64,
    /// Pre-formatted size, e.g. "35 MB".
    pub size_label: String,
    /// Directory prefix for display, e.g. "Artwork/". `None` when the file
    /// sits at the candidate-folder root.
    pub dir_prefix: Option<String>,
    /// Filename without directory, e.g. "front.jpg".
    pub file_name: String,
    /// Absolute filesystem path of the file on disk.
    pub local_path: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtworkFile {
    pub file: BridgeFileInfo,
    pub cover_choice: BridgeCoverChoice,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCueFlacPair {
    pub cue_name: String,
    pub cue_size: u64,
    /// Pre-formatted CUE file size, e.g. "1 KB".
    pub cue_size_label: String,
    /// Absolute filesystem path of the CUE file on disk.
    pub cue_local_path: String,
    pub flac_name: String,
    /// Absolute filesystem path of the audio file on disk.
    pub flac_local_path: String,
    pub total_size: u64,
    /// Pre-formatted total size, e.g. "340 MB".
    pub total_size_label: String,
    /// `None` when the CUE hasn't been parsed yet.
    pub track_count: Option<u32>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeAudioContent {
    CueFlacPairs { pairs: Vec<BridgeCueFlacPair> },
    TrackFiles { files: Vec<BridgeFileInfo> },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateFiles {
    pub audio: BridgeAudioContent,
    pub artwork: Vec<BridgeArtworkFile>,
    pub documents: Vec<BridgeFileInfo>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeImportStatus {
    Importing {
        progress_percent: u32,
        /// "acquire" (ripping/downloading) or "store" (encrypting/storing), or nil for folder imports
        phase: Option<String>,
        /// Human-readable status text (e.g. "Downloading cover art...")
        status_text: Option<String>,
    },
    Complete {
        album_id: String,
    },
    Error {
        message: String,
    },
}

/// One pressing under a release-group card. The card carries the album's
/// title, artist, and cover, so the pressing projection keeps only the
/// pressing-distinguishing fields the row renders plus the id/source the
/// import commit needs. Grouping happens in core, so the group id isn't
/// surfaced here.
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn library_status_to_bridge(s: bae_core::db::LibraryStatus) -> BridgeLibraryStatus {
    BridgeLibraryStatus {
        release_id: s.release_id,
        release_in_library: s.release_in_library,
        album_in_library: s.album_in_library,
        album_title: s.album_title,
        album_id: s.album_id,
    }
}

/// Which signal(s) backed a terminal identify match. `Combined` indicates
/// both disc-ID and barcode contributed via intersection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeIdentifySource {
    Discid,
    Barcode,
    Combined,
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
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub fn to_core(self) -> bae_core::identify::ExcludedSignal {
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn signal_origin_to_bridge(o: bae_core::signals::SignalOrigin) -> BridgeSignalOrigin {
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

/// A signal value paired with its origin — a catalog candidate or a barcode
/// code. Mirrors `bae_core::signals::SourcedValue`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSourcedValue {
    pub value: String,
    pub origin: BridgeSignalOrigin,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn sourced_value_to_bridge(s: bae_core::signals::SourcedValue) -> BridgeSourcedValue {
    BridgeSourcedValue {
        value: s.value,
        origin: signal_origin_to_bridge(s.origin),
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

/// The live lookup/match state of one toolbar badge. Mirrors
/// `bae_core::identify::SignalState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSignalState {
    LookingUp,
    Found { count: u32 },
    NoMatch,
    Skipped,
    Failed { message: String },
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn signal_state_to_bridge(s: bae_core::identify::SignalState) -> BridgeSignalState {
    use bae_core::identify::SignalState;
    match s {
        SignalState::LookingUp => BridgeSignalState::LookingUp,
        SignalState::Found { count } => BridgeSignalState::Found { count },
        SignalState::NoMatch => BridgeSignalState::NoMatch,
        SignalState::Skipped => BridgeSignalState::Skipped,
        SignalState::Failed { message } => BridgeSignalState::Failed { message },
        SignalState::Confirms { count } => BridgeSignalState::Confirms { count },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn toolbar_signal_to_bridge(s: bae_core::identify::ToolbarSignal) -> BridgeToolbarSignal {
    use bae_core::identify::{SignalKind, SignalRole};
    BridgeToolbarSignal {
        kind: match s.kind {
            SignalKind::DiscId => BridgeSignalKind::DiscId,
            SignalKind::Barcode => BridgeSignalKind::Barcode,
            SignalKind::Catalog => BridgeSignalKind::Catalog,
        },
        role: match s.role {
            SignalRole::Identity => BridgeSignalRole::Identity,
            SignalRole::Filter => BridgeSignalRole::Filter,
        },
        value: s.value,
        origin: signal_origin_to_bridge(s.origin),
        state: signal_state_to_bridge(s.state),
        excluded: s.excluded,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn toolbar_to_bridge(
    toolbar: Vec<bae_core::identify::ToolbarSignal>,
) -> BridgeSignalsToolbar {
    BridgeSignalsToolbar {
        signals: toolbar.into_iter().map(toolbar_signal_to_bridge).collect(),
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
        message: String,
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
    /// Pre-formatted year span + pressing count, e.g. "1992 – 2012 · 4 pressings".
    pub meta_label: String,
    pub pressings: Vec<BridgeMetadataResult>,
}

/// The disc-ID signal. Mirrors `bae_core::signals::DiscIdSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeDiscIdSignal {
    Computed { disc_id: String, track_count: u32 },
    Absent { track_count: u32 },
    Failed { message: String, track_count: u32 },
}

/// The barcode signal — the UPC/EAN code payloads with their origins. Mirrors
/// `bae_core::signals::BarcodeSignal`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeBarcodeSignal {
    Scanning { codes: Vec<BridgeSourcedValue> },
    Settled { codes: Vec<BridgeSourcedValue> },
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
        source: BridgeIdentifySource,
        /// Per-pressing provenance keyed by release id — the per-row signal
        /// badges.
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
        /// Human-readable source the disc-id results came from
        /// ("MusicBrainz"). `None` when the disc-id side is empty. The
        /// conflict surface names it in the disc-id section header.
        discid_source_label: Option<String>,
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

/// Callback for the unified UI event stream.
#[uniffi::export(callback_interface)]
pub trait UiEventCallback: Send + Sync {
    fn on_event(&self, event: BridgeUiEvent);
}

/// Everything one Vision pass over an image surfaces — barcode payloads and
/// recognized text lines from a single image decode. Mirrors
/// `bae_core::identify::ArtworkAnalysis`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtworkAnalysis {
    pub barcodes: Vec<String>,
    pub text_lines: Vec<String>,
}

/// Platform-provided artwork analyzer. One `analyze` pass over an image yields
/// both barcodes and text, so the signal-extraction pass decodes each image
/// exactly once.
///
/// Sync by design: `VNImageRequestHandler.perform` is synchronous, and the
/// Rust side calls this from `tokio::task::spawn_blocking` so the async
/// runtime isn't parked while Vision churns.
///
/// First request/response callback in this bridge — the precedent for
/// future ones. Contrast with `UiEventCallback`, which is fire-and-forget.
#[uniffi::export(callback_interface)]
pub trait ArtworkAnalyzerCallback: Send + Sync {
    /// Detect barcodes and recognize text in one image decode. Empty
    /// payloads/lines on failure or when absent.
    fn analyze(&self, path: String) -> BridgeArtworkAnalysis;
}

/// Top-level UI event. Every distinct state is a top-level variant with
/// fields inlined — no sub-enums.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUiEvent {
    // ── Playback ───────────────────────────────────────────────────
    PlaybackStopped,
    /// Playback couldn't start or continue — e.g. a cloud-only track that isn't
    /// downloaded yet, or an in-core decode failure. The UI surfaces `message`;
    /// playback itself falls back to stopped.
    PlaybackError {
        message: String,
    },
    PlaybackLoading {
        track_id: String,
        /// The target track's metadata, once core has resolved it. `None` in the
        /// first loading event; `Some` once the prepared track is in hand, so the
        /// UI can switch the now-playing bar to the target while audio is still
        /// downloading.
        track: Option<BridgeLoadingTrackInfo>,
    },
    PlaybackPlaying {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
        duration_label: String,
    },
    PlaybackPaused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image_id: Option<String>,
        duration_ms: u64,
        duration_label: String,
    },
    /// Position update — goes to NSView. Carries both regular ticks from the
    /// position listener and one-off updates emitted after a seek completes.
    PlaybackProgress {
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), so the media-control
        /// update reads it from the event instead of the now-playing slice.
        duration_ms: u64,
        progress: f64,
        elapsed_label: String,
        remaining_label: String,
    },
    VolumeChanged {
        volume: f32,
    },
    MuteChanged {
        is_muted: bool,
    },
    RepeatModeChanged {
        mode: BridgeRepeatMode,
    },
    QueueUpdated {
        items: Vec<BridgeQueueItem>,
        has_next: bool,
        has_previous: bool,
    },
    /// Tracks were appended/inserted into the queue. Carries the count for
    /// a transient "+N" badge in the UI. Suppressed when count is zero.
    QueueItemsAdded {
        count: u32,
    },

    // ── Preview ────────────────────────────────────────────────────
    PreviewIdle,
    PreviewPlaying {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
    PreviewPaused {
        path: String,
        duration_ms: u64,
        duration_label: String,
    },
    /// High-frequency tick — goes to NSView, not store.
    PreviewProgress {
        position_ms: u64,
        progress: f64,
        elapsed_label: String,
    },

    // ── Candidate-scoped (key inlined) ─────────────────────────────
    /// Identify pipeline transitioned. `state` carries the full new state —
    /// the reducer switches on its variant. `toolbar` is the pre-shaped
    /// signals badge row projected from the same transition (BridgeIdentifyState
    /// drops the signals context, so it travels separately); the reducer writes
    /// it onto the candidate wholesale.
    CandidateIdentifyStateChanged {
        key: String,
        state: BridgeIdentifyState,
        toolbar: BridgeSignalsToolbar,
    },
    /// Full snapshot of a candidate's extracted signals (disc ID, barcodes,
    /// classified text). Core emits this on every extraction transition; the
    /// reducer writes the whole snapshot wholesale (no delta logic).
    CandidateSignalsUpdated {
        key: String,
        signals: BridgeSignals,
    },
    CandidateImportImporting {
        key: String,
        progress_percent: u32,
        phase: Option<String>,
        status_text: Option<String>,
    },
    CandidateImportComplete {
        key: String,
        /// The release the import created — the UI's join key for
        /// candidate-level invalidation (release deleted) and the
        /// per-release upload queue.
        release_id: String,
        album_id: String,
    },
    CandidateImportError {
        key: String,
        message: String,
    },

    // ── Scan ───────────────────────────────────────────────────────
    /// The watched-folder list changed (loaded, or after add/remove). The
    /// reducer replaces its list and drops candidates whose watched folder is
    /// no longer present.
    WatchedFoldersChanged {
        folders: Vec<BridgeWatchedFolder>,
    },
    FolderCandidateAdded {
        candidate: BridgeFolderCandidate,
    },
    /// A leaf folder looked like a release but failed validation — the reducer
    /// surfaces it under the Skipped tab with its reason and drops the folder
    /// from the valid-candidate list if it was there before.
    InvalidCandidate {
        candidate: BridgeInvalidCandidate,
    },
    /// A candidate's folder was re-scanned and the release is gone — the reducer
    /// removes it by key.
    ScanCandidateRemoved {
        key: String,
    },
    /// The user manually skipped or unskipped a candidate — the reducer flips
    /// its `skipped` flag, re-tabbing it New ↔ Skipped.
    CandidateSkipChanged {
        key: String,
        skipped: bool,
    },
    ScanFinished,

    // ── Library ────────────────────────────────────────────────────
    AlbumAdded {
        album: BridgeAlbumDetail,
    },
    AlbumUpdated {
        album: BridgeAlbumDetail,
    },
    AlbumRemoved {
        album_id: String,
        release_ids: Vec<String>,
    },
    ReleaseAdded {
        album: BridgeAlbum,
        release: BridgeRelease,
    },
    ReleaseUpdated {
        album_id: String,
        release: BridgeRelease,
    },
    ReleaseRemoved {
        album_id: String,
        release_id: String,
        album: Option<BridgeAlbum>,
    },
    ConfigChanged {
        config: BridgeConfig,
        /// Whether the sync loop is running right now. Runtime status, not
        /// configuration — carried alongside `config` rather than inside it
        /// so the UI can land it on the store next to `syncError` instead of
        /// on the persisted-config mirror.
        sync_ready: bool,
    },
    /// Sync loop's current error state. `None` clears a prior failure.
    SyncError {
        message: Option<String>,
    },
    /// Wall-clock time of the latest successful sync cycle, as Unix epoch
    /// milliseconds. `None` until the first cycle completes.
    SyncTimeChanged {
        time: Option<i64>,
    },
    /// Whether the sync loop is currently mid-cycle. Drives the spinner the
    /// sidebar overlays on the active library row.
    SyncingChanged {
        syncing: bool,
    },
    /// The cloud outbox processing snapshot changed — the Storage Manager
    /// re-renders its queue panel from this.
    OutboxChanged {
        snapshot: BridgeOutboxSnapshot,
    },
    /// A pin/unpin/manage/unmanage transition advanced. `percent` is the
    /// overall release progress; `label` is a ready-to-render line. The UI
    /// shows a determinate bar on the release row until `ReleaseTransferEnded`.
    ReleaseTransferProgress {
        release_id: String,
        percent: u8,
        label: String,
    },
    /// A transition finished (success or failure) — the UI clears its transfer
    /// indicator. Failure text still arrives via the thrown error.
    ReleaseTransferEnded {
        release_id: String,
    },
    /// The in-memory download (pin) queue changed — the Storage Manager
    /// re-renders its Downloads pane from this.
    DownloadQueueChanged {
        snapshot: BridgeDownloadSnapshot,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        message: String,
    },
    ErrorCleared,
}

/// An upload's current processing state. `Failed` carries the error in
/// the item's `last_error`. `Active` carries the bytes uploaded so far
/// (always 0 today; populated once sub-file progress lands in coven).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadState {
    Queued,
    Active,
    Failed,
}

/// One queued upload. Mirror of bae-core's `UploadOp` across the FFI; carries
/// raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadOp {
    pub id: i64,
    pub file_id: String,
    /// Owning release id; `None` for an orphaned file.
    pub release_id: Option<String>,
    /// Album title for display; `None` for an orphaned file.
    pub title: Option<String>,
    pub cloud_key: String,
    pub bytes_total: u64,
    pub bytes_done: u64,
    /// Pre-formatted file size, e.g. `"70 MB"`.
    pub size_label: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub attempt_count: i64,
    pub state: BridgeUploadState,
    /// The most recent error, present only when `state` is `Failed`.
    pub last_error: Option<String>,
}

/// One pending cloud delete.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDeleteOp {
    pub id: i64,
    pub file_id: String,
    pub cloud_key: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// Per-state counts plus byte progress. Used both per-release (drives the
/// "Uploading (N)" badge on each storage row and gates per-release storage
/// actions) and as the overall total (drives the master progress bar).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// A queued download's state. `Active` carries the overall release percent in
/// the op's `percent`; `Failed` carries the reason in the op's `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDownloadState {
    Queued,
    Active,
    Failed,
}

/// One queued download — a whole release being pinned. Mirror of bae-core's
/// `DownloadOp`; carries raw fields the UI renders directly. `percent` is the
/// overall release progress while `state` is `Active` (0 otherwise); `error`
/// is the reason while `state` is `Failed`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadOp {
    pub release_id: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Pre-formatted total size, e.g. `"350 MB"`.
    pub size_label: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeDownloadState,
    /// Overall release percent while `state` is `Active`; 0 otherwise.
    pub percent: u8,
    /// The failure reason, present only when `state` is `Failed`.
    pub error: Option<String>,
}

/// Per-state counts for the download queue. Used per-release (the storage-row
/// "Downloading" badge) and as the overall total (the pane header). No bytes:
/// downloads track an overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeDownloadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory download (pin) queue snapshot the Storage Manager's Downloads
/// pane renders. The rolled-up counts and the one-line `summary` are computed in
/// bae-core; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadSnapshot {
    pub downloads: Vec<BridgeDownloadOp>,
    pub total: BridgeDownloadProgress,
    /// True when the user paused the download queue. Drives the pause/resume
    /// toggle in the Downloads pane.
    pub paused: bool,
    pub summary: String,
}

/// The cloud-outbox processing snapshot the Storage Manager renders. The
/// counts, per-release aggregates, one-line `summary`, throughput, and ETA
/// are computed in bae-core; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    pub uploads: Vec<BridgeUploadOp>,
    pub deletes: Vec<BridgeDeleteOp>,
    /// Per-release aggregate, keyed by release id. Releases with no pending
    /// work are absent from the map.
    pub per_release: std::collections::HashMap<String, BridgeUploadProgress>,
    pub total: BridgeUploadProgress,
    pub pending_deletes: u32,
    /// True when the user has paused the upload pipeline. Drives the
    /// pause/resume toggle and suppresses throughput/ETA in the UI.
    pub paused: bool,
    pub summary: String,
    /// Rolling-window upload throughput in bytes per second.
    pub throughput_bps: u64,
    /// Pre-formatted throughput label, e.g. `"5.2 MB/s"`. Empty when idle.
    pub throughput_label: String,
    /// Estimated seconds remaining at the current rate.
    pub eta_seconds: Option<u64>,
    /// Pre-formatted ETA, e.g. `"2m 14s remaining"`. Empty when not computable.
    pub eta_label: String,
    /// Pre-formatted bytes-done/total label, e.g. `"1.2 GB of 14.4 GB"`.
    /// Empty when there's nothing to upload.
    pub bytes_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSearchQueryKind {
    General,
    CatalogNumber,
    Barcode,
}

/// Returned from `search_for_candidate`. Echoes the `tab` and `source` the
/// search ran against so the caller can route results into the matching
/// (tab, source) slot — the user may have changed tabs or sources during
/// the await.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCandidateSearchResults {
    pub tab: BridgeSearchQueryKind,
    pub source: BridgeMetadataSource,
    /// Results grouped into release-group cards, one card per group with its
    /// pressings beneath.
    pub groups: Vec<BridgeReleaseGroup>,
    /// Per-release library dupe statuses, looked up by release id.
    pub statuses: Vec<BridgeLibraryStatus>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseDetail {
    pub release_id: String,
    pub source: BridgeMetadataSource,
    /// Per-source release group: MB release-group ID for MusicBrainz,
    /// Discogs master ID for Discogs. `None` when the source didn't
    /// surface a group — the picked release commits without a group
    /// identity row, but Approximate is still meaningful as "I don't
    /// claim this specific pressing."
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
    pub track_count_mismatch: bool,
    pub tracks: Vec<BridgeReleaseTrack>,
    pub cover_art: Vec<BridgeRemoteCover>,
    pub default_cover: Option<BridgeCoverChoice>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseTrack {
    pub title: String,
    pub artist: Option<String>,
    pub duration_ms: Option<u64>,
    /// Pre-formatted duration, e.g. "3:07". Empty if duration is None.
    pub duration_label: String,
    pub position: String,
    pub side: u32,
    /// Human-readable side label: "Side A", "Disc 2", or empty for single-side digital
    pub side_label: String,
    /// Human-readable track position: "A1", "1", "1-2", etc.
    pub position_label: String,
}

/// Mirror of `bae_core::import::ReleaseUserEdit` — a normalized, validated
/// metadata edit ready to apply.
///
/// `tracks` MUST line up with the release's existing tracks in order; edits
/// cannot add or remove tracks. `album_artist_names` is positional —
/// element 0 becomes the primary album artist (`album.artist_id`),
/// subsequent elements get higher `album_artists.position` rows.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseUserEdit {
    pub album_title: String,
    pub album_artist_names: Vec<String>,
    pub pressing: BridgePressingEdit,
    pub tracks: Vec<BridgeTrackUserEdit>,
}

/// Mirror of `bae_core::import::PressingEdit`. Groups the six pressing
/// fields a release carries; per-field `None` means "this field isn't set".
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePressingEdit {
    pub year: Option<i32>,
    pub format: Option<String>,
    pub label: Option<String>,
    pub catalog_number: Option<String>,
    pub country: Option<String>,
    pub barcode: Option<String>,
}

/// One per existing track, in track order. `artist_names` empty means the
/// track shares the album artist (no per-track artist rows). Non-empty is
/// positional — element N becomes `track_artists.position = N`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackUserEdit {
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub artist_names: Vec<String>,
}

/// Raw edit-metadata form values, exactly as the editor holds them — text
/// as typed, not yet normalized. Mirrors `bae_core::import::RawReleaseEdit`.
/// The editor binds directly to this shape and calls `shape_release_edit` to
/// normalize + validate it into a wire `BridgeReleaseUserEdit`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawReleaseEdit {
    pub album_title: String,
    /// Comma-separated artist text in positional order, as typed.
    pub album_artist_text: String,
    pub pressing: BridgeRawPressingEdit,
    pub tracks: Vec<BridgeRawTrackEdit>,
}

/// Raw pressing fields as the editor holds them. Mirrors
/// `bae_core::import::RawPressingEdit`: each is the text the user typed,
/// empty meaning "not set"; `year` is text (parsed at shape time).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawPressingEdit {
    pub year: String,
    pub format: String,
    pub label: String,
    pub catalog_number: String,
    pub country: String,
    pub barcode: String,
}

/// One raw track row from the editor. Mirrors
/// `bae_core::import::RawTrackEdit`: `id` is the stable `ForEach` row
/// identity; `artist_text` empty means "share the album artist".
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRawTrackEdit {
    pub id: String,
    pub title: String,
    pub artist_text: String,
    pub side: i32,
    pub track_number: Option<i32>,
}

/// Why a release edit can't be saved. An FFI mirror of bae-core's
/// `EditValidationError`; the UI renders each variant by resolving its
/// localization key — see `bridge_validation_reason_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeValidationReason {
    EmptyAlbumTitle,
    NoAlbumArtist,
    InvalidYear,
}

impl BridgeValidationReason {
    /// The catalog key the UI resolves against the generated `Core` string
    /// table — the single source of the variant→key mapping for every platform.
    /// Only the desktop edit flow produces a validation reason, so the mapping
    /// is compiled there (and under test for the cross-check).
    #[cfg(any(feature = "desktop", test))]
    pub(crate) fn loc_key(self) -> &'static str {
        match self {
            Self::EmptyAlbumTitle => "core.import.validation.empty_album_title",
            Self::NoAlbumArtist => "core.import.validation.no_album_artist",
            Self::InvalidYear => "core.import.validation.invalid_year",
        }
    }
}

/// Outcome of shaping a raw edit form (`shape_release_edit`). `Valid` carries
/// the savable wire edit; `Invalid` carries the typed reason it can't be saved.
/// The editor enables Save on `Valid` and renders the localized reason on
/// `Invalid` — bae-core decides which reason, the UI localizes it.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeShapeResult {
    Valid { edit: BridgeReleaseUserEdit },
    Invalid { reason: BridgeValidationReason },
}

#[cfg(test)]
mod validation_reason_tests {
    use super::BridgeValidationReason;

    /// Every validation reason's localization key must exist in the master
    /// catalog, so a renamed key or a dropped catalog entry fails the build
    /// instead of rendering a raw key in the UI.
    #[test]
    fn keys_exist_in_catalog() {
        let cat = bae_loc::Catalog::from_toml(include_str!("../loc/catalog.toml"))
            .expect("catalog parses");
        for reason in [
            BridgeValidationReason::EmptyAlbumTitle,
            BridgeValidationReason::NoAlbumArtist,
            BridgeValidationReason::InvalidYear,
        ] {
            assert!(
                cat.messages.contains_key(reason.loc_key()),
                "catalog is missing key `{}`",
                reason.loc_key()
            );
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    pub library_id: String,
    pub library_name: String,
    pub library_path: String,
    pub encryption_key_stored: bool,
    pub encryption_key_fingerprint: Option<String>,
    pub discogs_token_status: BridgeDiscogsTokenStatus,
    /// Whether Discogs can be used as a metadata source (a stored key that
    /// isn't rejected). Core decides the policy via `DiscogsTokenStatus::
    /// is_usable`; the UI reads this flag rather than re-deriving it from the
    /// status.
    pub discogs_usable: bool,
    /// The configured cloud provider, present whenever YAML carries one — so
    /// the settings tab can render the previous selection even when sync is
    /// broken. Does not imply sync is working: that's runtime status carried
    /// by `BridgeUiEvent::ConfigChanged.sync_ready`, not config.
    pub sync: Option<BridgeSyncConfig>,
}

/// Cloud sync settings for a connected provider. `provider` carries the
/// provider-specific fields; the rest are shared across providers. Whether
/// sync is actually running is `BridgeConfig.sync_ready`, kept orthogonal.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncConfig {
    pub provider: BridgeSyncProvider,
    /// Short label naming `provider` for display ("S3-compatible", "iCloud",
    /// …). Pre-built in core so the UI renders it directly instead of
    /// switching on `provider`.
    pub cloud_provider_label: String,
    /// Display name for the connected account (e.g. "s3://bucket", "iCloud").
    pub cloud_account_display: Option<String>,
}

/// The connected cloud provider with its provider-specific display fields.
/// Providers without extra fields (OAuth, CloudKit) are fieldless variants.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSyncProvider {
    S3 {
        bucket: Option<String>,
        region: Option<String>,
        endpoint: Option<String>,
    },
    GoogleDrive,
    Dropbox,
    OneDrive,
    CloudKit,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSaveSyncConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub key_prefix: Option<String>,
    pub access_key: String,
    pub secret_key: String,
    /// Whether the home is opaque (encrypted) or browsable (stored in the clear).
    pub storage: BridgeHomeStorage,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSearchResults {
    pub albums: Vec<BridgeAlbumSearchResult>,
    pub tracks: Vec<BridgeTrackSearchResult>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    /// Album's primary release. Always set: every album has at least one
    /// release.
    pub primary_release_id: String,
    pub artist_name: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    /// Pre-formatted duration, e.g. "3:07". Empty if duration is None.
    pub duration_label: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

/// One row on the Storage Manager: a release paired with its parent
/// album. The UI splits these into separate slices (releases +
/// summaries) so in-band metadata changes (album rename, pin toggle)
/// re-render affected rows without list rebuilds.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageRow {
    pub release: BridgeReleaseSummary,
    pub album: BridgeAlbum,
}

/// One page of the Storage Manager list. `total_count` reflects the
/// filtered subset, not the full library — so paginated list machinery
/// knows where to stop.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStoragePage {
    pub rows: Vec<BridgeStorageRow>,
    pub total_count: u64,
}

/// Column the Storage Manager can sort by. Mirrors the sortable
/// columns `StorageManagerView` renders today.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortField {
    AlbumTitle,
    ArtistNames,
    Format,
    FileCount,
    TotalSize,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct BridgeStorageSort {
    pub field: BridgeStorageSortField,
    pub direction: BridgeStorageSortDirection,
}

/// Filter chip applied to the Storage Manager list. Mirrors the four
/// mutually-exclusive chips the UI exposes.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeStorageFilter {
    All,
    Managed,
    Unmanaged,
    Uploading,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueItem {
    pub track_id: String,
    pub title: String,
    pub artist_names: String,
    pub duration_ms: Option<i64>,
    /// Pre-formatted duration, e.g. "3:07". Empty if duration is None.
    pub duration_label: String,
    pub album_title: String,
    pub cover_image_id: Option<String>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSortField {
    Title,
    Artist,
    Year,
    DateAdded,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSortCriterion {
    pub field: BridgeSortField,
    pub direction: BridgeSortDirection,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageMode {
    Unmanaged,
    ManagedPinned,
    ManagedUnpinned,
}

#[derive(Debug, uniffi::Enum)]
pub enum BridgeRestoreSource {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
    CloudKit,
    GoogleDrive {
        folder_id: String,
        oauth_token_json: String,
    },
    Dropbox {
        folder_path: String,
        oauth_token_json: String,
    },
    OneDrive {
        drive_id: String,
        folder_id: String,
        oauth_token_json: String,
    },
}

/// Decoded restore code info for UI preview (before the actual restore).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRestoreCodeInfo {
    pub library_id: String,
    pub library_name: String,
    pub cloud_provider: BridgeCloudProvider,
    /// Short label naming `cloud_provider` for display ("S3-compatible",
    /// "iCloud", …). Pre-built in core so the UI renders it directly instead
    /// of switching on `cloud_provider`.
    pub cloud_provider_label: String,
    pub needs_oauth: bool,
}

/// Per-provider form fields for validating a manual restore configuration.
#[derive(Debug, uniffi::Enum)]
pub enum BridgeRestoreFormFields {
    S3 {
        library_id: String,
        encryption_key: String,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
    },
    GoogleDrive {
        library_id: String,
        encryption_key: String,
        folder_id: String,
        has_oauth_token: bool,
    },
    Dropbox {
        library_id: String,
        encryption_key: String,
        folder_path: String,
        has_oauth_token: bool,
    },
    OneDrive {
        library_id: String,
        encryption_key: String,
        drive_id: String,
        folder_id: String,
        has_oauth_token: bool,
    },
    CloudKit {
        library_id: String,
        encryption_key: String,
    },
}

impl BridgeRestoreFormFields {
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::S3 {
                library_id,
                encryption_key,
                bucket,
                region,
                access_key,
                secret_key,
            } => {
                !library_id.is_empty()
                    && !encryption_key.is_empty()
                    && !bucket.is_empty()
                    && !region.is_empty()
                    && !access_key.is_empty()
                    && !secret_key.is_empty()
            }
            Self::GoogleDrive {
                library_id,
                encryption_key,
                folder_id,
                has_oauth_token,
            } => {
                !library_id.is_empty()
                    && !encryption_key.is_empty()
                    && !folder_id.is_empty()
                    && *has_oauth_token
            }
            Self::Dropbox {
                library_id,
                encryption_key,
                folder_path,
                has_oauth_token,
            } => {
                !library_id.is_empty()
                    && !encryption_key.is_empty()
                    && !folder_path.is_empty()
                    && *has_oauth_token
            }
            Self::OneDrive {
                library_id,
                encryption_key,
                drive_id,
                folder_id,
                has_oauth_token,
            } => {
                !library_id.is_empty()
                    && !encryption_key.is_empty()
                    && !drive_id.is_empty()
                    && !folder_id.is_empty()
                    && *has_oauth_token
            }
            Self::CloudKit {
                library_id,
                encryption_key,
            } => !library_id.is_empty() && !encryption_key.is_empty(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeExportFormat {
    Flac,
    Mp3,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    #[error("Cancelled")]
    Cancelled,
    #[error("Not found: {msg}")]
    NotFound { msg: String },
    #[error("Configuration error: {msg}")]
    Config { msg: String },
    #[error("Database error: {msg}")]
    Database { msg: String },
    #[error("Internal error: {msg}")]
    Internal { msg: String },
    #[error("Import error: {msg}")]
    Import { msg: String },
    #[error("Export error: {msg}")]
    Export { msg: String },
}

#[cfg(feature = "cloudkit")]
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CloudKitError {
    #[error("not found: {msg}")]
    NotFound { msg: String },
    #[error("CloudKit error: {msg}")]
    Storage { msg: String },
}

// =========================================================================
// Core <-> Bridge conversions
// =========================================================================

#[cfg(feature = "desktop")]
pub(crate) fn bridge_storage_mode_to_core(
    mode: BridgeStorageMode,
) -> bae_core::import::StorageMode {
    match mode {
        BridgeStorageMode::Unmanaged => bae_core::import::StorageMode::Unmanaged,
        BridgeStorageMode::ManagedPinned => bae_core::import::StorageMode::Managed { pin: true },
        BridgeStorageMode::ManagedUnpinned => bae_core::import::StorageMode::Managed { pin: false },
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn bridge_cover_to_import(c: BridgeCoverSelection) -> bae_core::import::CoverSelection {
    match c {
        BridgeCoverSelection::ReleaseImage { file_id } => {
            bae_core::import::CoverSelection::Local(file_id)
        }
        BridgeCoverSelection::RemoteCover { selection } => {
            bae_core::import::CoverSelection::Remote(selection.url, selection.source.to_core())
        }
    }
}

pub(crate) fn bridge_cloud_provider(p: &bae_core::config::CloudProvider) -> BridgeCloudProvider {
    use bae_core::config::CloudProvider;
    match p {
        CloudProvider::S3 => BridgeCloudProvider::S3,
        CloudProvider::GoogleDrive => BridgeCloudProvider::GoogleDrive,
        CloudProvider::Dropbox => BridgeCloudProvider::Dropbox,
        CloudProvider::OneDrive => BridgeCloudProvider::OneDrive,
        CloudProvider::CloudKit => BridgeCloudProvider::CloudKit,
    }
}

pub(crate) fn bridge_cloud_provider_to_core(
    p: BridgeCloudProvider,
) -> bae_core::config::CloudProvider {
    use bae_core::config::CloudProvider;
    match p {
        BridgeCloudProvider::S3 => CloudProvider::S3,
        BridgeCloudProvider::GoogleDrive => CloudProvider::GoogleDrive,
        BridgeCloudProvider::Dropbox => CloudProvider::Dropbox,
        BridgeCloudProvider::OneDrive => CloudProvider::OneDrive,
        BridgeCloudProvider::CloudKit => CloudProvider::CloudKit,
    }
}

pub(crate) fn bridge_home_storage_to_core(s: BridgeHomeStorage) -> bae_core::config::HomeStorage {
    use bae_core::config::HomeStorage;
    match s {
        BridgeHomeStorage::Opaque => HomeStorage::Opaque,
        BridgeHomeStorage::Browsable => HomeStorage::Browsable,
    }
}

pub(crate) fn bridge_storage_sort_to_core(
    sort: &BridgeStorageSort,
) -> bae_core::album_detail::StorageSort {
    bae_core::album_detail::StorageSort {
        field: match sort.field {
            BridgeStorageSortField::AlbumTitle => {
                bae_core::album_detail::StorageSortField::AlbumTitle
            }
            BridgeStorageSortField::ArtistNames => {
                bae_core::album_detail::StorageSortField::ArtistNames
            }
            BridgeStorageSortField::Format => bae_core::album_detail::StorageSortField::Format,
            BridgeStorageSortField::FileCount => {
                bae_core::album_detail::StorageSortField::FileCount
            }
            BridgeStorageSortField::TotalSize => {
                bae_core::album_detail::StorageSortField::TotalSize
            }
        },
        direction: match sort.direction {
            BridgeStorageSortDirection::Ascending => {
                bae_core::album_detail::StorageSortDirection::Ascending
            }
            BridgeStorageSortDirection::Descending => {
                bae_core::album_detail::StorageSortDirection::Descending
            }
        },
    }
}

pub(crate) fn bridge_storage_filter_to_core(
    filter: BridgeStorageFilter,
) -> bae_core::album_detail::StorageFilter {
    match filter {
        BridgeStorageFilter::All => bae_core::album_detail::StorageFilter::All,
        BridgeStorageFilter::Managed => bae_core::album_detail::StorageFilter::Managed,
        BridgeStorageFilter::Unmanaged => bae_core::album_detail::StorageFilter::Unmanaged,
        BridgeStorageFilter::Uploading => bae_core::album_detail::StorageFilter::Uploading,
    }
}

pub(crate) fn bridge_sort_to_core(c: &BridgeSortCriterion) -> bae_core::db::AlbumSortCriterion {
    bae_core::db::AlbumSortCriterion {
        field: match c.field {
            BridgeSortField::Title => bae_core::db::AlbumSortField::Title,
            BridgeSortField::Artist => bae_core::db::AlbumSortField::Artist,
            BridgeSortField::Year => bae_core::db::AlbumSortField::Year,
            BridgeSortField::DateAdded => bae_core::db::AlbumSortField::DateAdded,
        },
        direction: match c.direction {
            BridgeSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
            BridgeSortDirection::Descending => bae_core::db::SortDirection::Descending,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn metadata_result_to_bridge(
    r: bae_core::import::search::MetadataResult,
) -> BridgeMetadataResult {
    BridgeMetadataResult {
        source: BridgeMetadataSource::from_core(r.source),
        release_id: r.release_id,
        year: r.year,
        format: r.format,
        label: r.label,
        catalog_number: r.catalog_number,
        country: r.country,
    }
}

#[cfg(any(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
pub(crate) fn remote_cover_data_to_bridge(
    c: bae_core::import::cover_art::RemoteCover,
) -> BridgeRemoteCover {
    let selection = bridge_remote_cover_selection(c.url, c.source);
    let cover_choice = remote_cover_choice_to_bridge(&selection, &c.thumbnail_url);
    BridgeRemoteCover {
        cover_choice,
        label: c.label,
    }
}

#[cfg(any(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
fn bridge_remote_cover_selection(
    url: String,
    source: bae_core::import::MetadataSource,
) -> BridgeRemoteCoverSelection {
    BridgeRemoteCoverSelection {
        url,
        source: BridgeMetadataSource::from_core(source),
    }
}

#[cfg(any(
    feature = "desktop",
    not(any(target_os = "ios", target_os = "android"))
))]
fn remote_cover_choice_to_bridge(
    selection: &BridgeRemoteCoverSelection,
    thumbnail_url: &str,
) -> BridgeCoverChoice {
    BridgeCoverChoice {
        selection: BridgeCoverSelection::RemoteCover {
            selection: selection.clone(),
        },
        preview_source: BridgeCoverImageSource::Remote {
            url: selection.url.clone(),
        },
        thumbnail_source: BridgeCoverImageSource::Remote {
            url: thumbnail_url.to_string(),
        },
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn release_detail_to_bridge(
    d: bae_core::import::search::ImportSearchReleaseDetail,
    local_track_count: Option<u32>,
) -> BridgeReleaseDetail {
    let track_count_mismatch = d.track_count_mismatch(local_track_count);
    let default_cover = d
        .default_cover()
        .cloned()
        .map(remote_cover_data_to_bridge)
        .map(|c| c.cover_choice);
    let cover_art: Vec<BridgeRemoteCover> = d
        .cover_art
        .into_iter()
        .map(remote_cover_data_to_bridge)
        .collect();
    BridgeReleaseDetail {
        release_id: d.release_id,
        source: BridgeMetadataSource::from_core(d.source),
        source_group_id: d.source_group_id,
        title: d.title,
        artist: d.artist,
        year: d.year,
        format: d.format,
        label: d.label,
        catalog_number: d.catalog_number,
        country: d.country,
        barcode: d.barcode,
        track_count: d.track_count,
        track_count_mismatch,
        tracks: d
            .tracks
            .into_iter()
            .map(|t| BridgeReleaseTrack {
                title: t.title,
                artist: t.artist,
                duration_label: t.duration_label,
                duration_ms: t.duration_ms,
                position: t.position,
                side: t.side,
                side_label: t.side_label,
                position_label: t.position_label,
            })
            .collect(),
        cover_art,
        default_cover,
    }
}

#[cfg(feature = "desktop")]
fn bridge_remote_cover_to_core(c: BridgeRemoteCover) -> bae_core::import::cover_art::RemoteCover {
    let BridgeCoverChoice {
        selection,
        thumbnail_source,
        ..
    } = c.cover_choice;
    let BridgeCoverSelection::RemoteCover { selection } = selection else {
        unreachable!("BridgeRemoteCover must carry a remote cover selection");
    };
    let BridgeCoverImageSource::Remote { url: thumbnail_url } = thumbnail_source else {
        unreachable!("BridgeRemoteCover must carry a remote cover thumbnail");
    };
    bae_core::import::cover_art::RemoteCover {
        url: selection.url,
        thumbnail_url,
        label: c.label,
        source: selection.source.to_core(),
    }
}

/// Reverse of [`release_detail_to_bridge`]. Used by the user-edit shaping
/// path (the bridge has the detail in bridge shape from the prefetch, and
/// the shaping function in bae-core takes the core type). Drops the
/// bridge-computed `track_count_mismatch` field — shaping doesn't read it.
#[cfg(feature = "desktop")]
pub(crate) fn release_detail_from_bridge(
    d: BridgeReleaseDetail,
) -> bae_core::import::search::ImportSearchReleaseDetail {
    bae_core::import::search::ImportSearchReleaseDetail {
        release_id: d.release_id,
        source: d.source.to_core(),
        source_group_id: d.source_group_id,
        title: d.title,
        artist: d.artist,
        year: d.year,
        format: d.format,
        label: d.label,
        catalog_number: d.catalog_number,
        country: d.country,
        barcode: d.barcode,
        track_count: d.track_count,
        tracks: d
            .tracks
            .into_iter()
            .map(|t| bae_core::import::search::ReleaseTrack {
                title: t.title,
                artist: t.artist,
                duration_ms: t.duration_ms,
                duration_label: t.duration_label,
                position: t.position,
                side: t.side,
                side_label: t.side_label,
                position_label: t.position_label,
            })
            .collect(),
        cover_art: d
            .cover_art
            .into_iter()
            .map(bridge_remote_cover_to_core)
            .collect(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn identify_source_to_bridge(
    s: bae_core::identify::IdentifySource,
) -> BridgeIdentifySource {
    match s {
        bae_core::identify::IdentifySource::Discid => BridgeIdentifySource::Discid,
        bae_core::identify::IdentifySource::Barcode => BridgeIdentifySource::Barcode,
        bae_core::identify::IdentifySource::Combined => BridgeIdentifySource::Combined,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn discid_progress_to_bridge(p: bae_core::identify::DiscidProgress) -> BridgeDiscidProgress {
    use bae_core::identify::DiscidProgress;
    match p {
        DiscidProgress::Computing => BridgeDiscidProgress::Computing,
        DiscidProgress::LookingUp => BridgeDiscidProgress::LookingUp,
        DiscidProgress::Done { results, .. } => BridgeDiscidProgress::Done {
            n_results: results.len() as u32,
        },
        DiscidProgress::Skipped { .. } => BridgeDiscidProgress::Skipped,
        DiscidProgress::Failed { message } => BridgeDiscidProgress::Failed { message },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn barcode_progress_to_bridge(p: bae_core::identify::BarcodeProgress) -> BridgeBarcodeProgress {
    use bae_core::identify::BarcodeProgress;
    match p {
        BarcodeProgress::Scanning => BridgeBarcodeProgress::Scanning,
        BarcodeProgress::LookingUp {
            current,
            position,
            total,
            ..
        } => BridgeBarcodeProgress::LookingUp {
            current,
            position,
            total,
        },
        BarcodeProgress::Done { results, .. } => BridgeBarcodeProgress::Done {
            n_results: results.len() as u32,
        },
        BarcodeProgress::Skipped => BridgeBarcodeProgress::Skipped,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn release_group_to_bridge(
    g: bae_core::import::release_group::ReleaseGroup,
) -> BridgeReleaseGroup {
    BridgeReleaseGroup {
        id: g.id,
        title: g.title,
        artist: g.artist,
        cover_art: g.cover_art.map(remote_cover_data_to_bridge),
        source_label: g.source_label,
        group_url: g.group_url,
        meta_label: g.meta_label,
        pressings: g
            .pressings
            .into_iter()
            .map(metadata_result_to_bridge)
            .collect(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn signals_to_bridge(s: bae_core::signals::Signals) -> BridgeSignals {
    use bae_core::signals::{BarcodeSignal, DiscIdSignal, TextSignal};

    let disc_id = match s.disc_id {
        DiscIdSignal::Computed {
            disc_id,
            track_count,
        } => BridgeDiscIdSignal::Computed {
            disc_id,
            track_count,
        },
        DiscIdSignal::Absent { track_count } => BridgeDiscIdSignal::Absent { track_count },
        DiscIdSignal::Failed {
            message,
            track_count,
        } => BridgeDiscIdSignal::Failed {
            message,
            track_count,
        },
    };

    let barcode = match s.barcode {
        BarcodeSignal::Scanning { codes } => BridgeBarcodeSignal::Scanning {
            codes: codes.into_iter().map(sourced_value_to_bridge).collect(),
        },
        BarcodeSignal::Settled { codes } => BridgeBarcodeSignal::Settled {
            codes: codes.into_iter().map(sourced_value_to_bridge).collect(),
        },
        BarcodeSignal::Absent => BridgeBarcodeSignal::Absent,
    };

    let text = match s.text {
        TextSignal::Scanning {
            catalogs,
            free_text,
        } => BridgeTextSignal::Scanning {
            catalogs: catalogs.into_iter().map(sourced_value_to_bridge).collect(),
            free_text,
        },
        TextSignal::Settled {
            catalogs,
            free_text,
        } => BridgeTextSignal::Settled {
            catalogs: catalogs.into_iter().map(sourced_value_to_bridge).collect(),
            free_text,
        },
    };

    BridgeSignals {
        disc_id,
        barcode,
        text,
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn provenance_to_bridge(p: bae_core::identify::ResultProvenance) -> BridgeResultProvenance {
    BridgeResultProvenance {
        by_disc_id: p.by_disc_id,
        by_barcode: p.by_barcode,
        matches_catalog: p.matches_catalog,
    }
}

/// Convert a core identify state into its bridge mirror.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn identify_state_to_bridge(
    s: bae_core::identify::IdentifyState,
) -> BridgeIdentifyState {
    use bae_core::identify::IdentifyState;
    match s {
        IdentifyState::Idle => BridgeIdentifyState::Idle,
        IdentifyState::Triangulating {
            discid, barcode, ..
        } => BridgeIdentifyState::Triangulating {
            discid: discid_progress_to_bridge(discid),
            barcode: barcode_progress_to_bridge(barcode),
        },
        IdentifyState::Found {
            matches,
            library_statuses,
            track_count,
            group,
            source,
            provenance,
            context: _,
        } => {
            // `matches` all share `group` (combine guarantees a single group)
            // and `provenance` is index-aligned with them. Key the provenance
            // by release id before folding the matches into the group card so
            // the UI looks each pressing's badges up directly.
            let provenance = matches
                .iter()
                .map(|m| m.release_id.clone())
                .zip(provenance.into_iter().map(provenance_to_bridge))
                .collect();
            let group = bae_core::import::release_group::ReleaseGroup::from_group(group, matches);
            BridgeIdentifyState::Found {
                group: release_group_to_bridge(group),
                library_statuses: library_statuses
                    .into_iter()
                    .map(|s| (s.release_id.clone(), library_status_to_bridge(s)))
                    .collect(),
                track_count,
                source: identify_source_to_bridge(source),
                provenance,
            }
        }
        IdentifyState::Conflict { context } => {
            // The per-signal sections come from the context's settled results.
            // Disc-id results all share one source; name it for the section
            // header. `None` when the disc-id side is empty (no section).
            let discid_source_label = context
                .discid_results
                .first()
                .map(|(m, _)| m.source.display_name().to_string());
            let (discid_matches, discid_statuses) = results_and_status_map(context.discid_results);
            let (barcode_matches, barcode_statuses) =
                results_and_status_map(context.barcode_results);
            BridgeIdentifyState::Conflict {
                discid_results: discid_matches,
                discid_library_statuses: discid_statuses,
                barcode_results: barcode_matches,
                barcode_library_statuses: barcode_statuses,
                discid_source_label,
                matched_barcode: context.matched_barcode,
                track_count: context.track_count,
            }
        }
        IdentifyState::NotFoundAnywhere { .. } => BridgeIdentifyState::NotFoundAnywhere,
        IdentifyState::ManualOnly { track_count, .. } => {
            BridgeIdentifyState::ManualOnly { track_count }
        }
    }
}

/// Split per-signal `(result, status)` pairs into an ordered results list
/// (display order matters) plus a status map keyed by release id (the UI
/// looks up each row's status by id).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn results_and_status_map(
    pairs: Vec<(
        bae_core::import::search::MetadataResult,
        bae_core::db::LibraryStatus,
    )>,
) -> (
    Vec<BridgeMetadataResult>,
    std::collections::HashMap<String, BridgeLibraryStatus>,
) {
    let mut matches = Vec::with_capacity(pairs.len());
    let mut statuses = std::collections::HashMap::with_capacity(pairs.len());
    for (m, s) in pairs {
        statuses.insert(s.release_id.clone(), library_status_to_bridge(s));
        matches.push(metadata_result_to_bridge(m));
    }
    (matches, statuses)
}

fn scanned_file_to_bridge(f: bae_core::import::folder_scanner::ScannedFile) -> BridgeFileInfo {
    BridgeFileInfo {
        name: f.relative_path,
        size_label: f.size_label,
        size: f.size,
        dir_prefix: f.dir_prefix,
        file_name: f.file_name,
        local_path: f.path.to_string_lossy().to_string(),
    }
}

fn scanned_artwork_to_bridge(
    f: bae_core::import::folder_scanner::ScannedFile,
) -> BridgeArtworkFile {
    let file_id = f.relative_path.clone();
    let path = f.path.to_string_lossy().to_string();
    BridgeArtworkFile {
        file: scanned_file_to_bridge(f),
        cover_choice: BridgeCoverChoice {
            selection: BridgeCoverSelection::ReleaseImage { file_id },
            preview_source: BridgeCoverImageSource::Local { path: path.clone() },
            thumbnail_source: BridgeCoverImageSource::Local { path },
        },
    }
}

pub(crate) fn categorized_files_to_bridge(
    files: bae_core::import::folder_scanner::CategorizedFiles,
) -> BridgeCandidateFiles {
    use bae_core::import::folder_scanner::AudioContent;

    let audio = match files.audio {
        AudioContent::CueFlacPairs { pairs, .. } => BridgeAudioContent::CueFlacPairs {
            pairs: pairs
                .into_iter()
                .map(|p| BridgeCueFlacPair {
                    cue_name: p.cue_file.relative_path,
                    cue_size_label: p.cue_file.size_label,
                    cue_size: p.cue_file.size,
                    cue_local_path: p.cue_file.path.to_string_lossy().to_string(),
                    flac_name: p.audio_file.relative_path,
                    flac_local_path: p.audio_file.path.to_string_lossy().to_string(),
                    total_size_label: p.total_size_label,
                    total_size: p.total_size,
                    track_count: p.cue_sheet.as_ref().map(|s| s.tracks.len() as u32),
                })
                .collect(),
        },
        AudioContent::TrackFiles { tracks, .. } => BridgeAudioContent::TrackFiles {
            files: tracks.into_iter().map(scanned_file_to_bridge).collect(),
        },
    };

    BridgeCandidateFiles {
        audio,
        artwork: files
            .artwork
            .into_iter()
            .map(scanned_artwork_to_bridge)
            .collect(),
        documents: files
            .documents
            .into_iter()
            .map(scanned_file_to_bridge)
            .collect(),
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn release_user_edit_from_bridge(
    e: BridgeReleaseUserEdit,
) -> bae_core::import::ReleaseUserEdit {
    bae_core::import::ReleaseUserEdit {
        album_title: e.album_title,
        album_artist_names: e.album_artist_names,
        pressing: bae_core::import::PressingEdit {
            year: e.pressing.year,
            format: e.pressing.format,
            label: e.pressing.label,
            catalog_number: e.pressing.catalog_number,
            country: e.pressing.country,
            barcode: e.pressing.barcode,
        },
        tracks: e
            .tracks
            .into_iter()
            .map(|t| bae_core::import::TrackUserEdit {
                title: t.title,
                side: t.side,
                track_number: t.track_number,
                artist_names: t.artist_names,
            })
            .collect(),
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn release_user_edit_to_bridge(
    e: bae_core::import::ReleaseUserEdit,
) -> BridgeReleaseUserEdit {
    BridgeReleaseUserEdit {
        album_title: e.album_title,
        album_artist_names: e.album_artist_names,
        pressing: BridgePressingEdit {
            year: e.pressing.year,
            format: e.pressing.format,
            label: e.pressing.label,
            catalog_number: e.pressing.catalog_number,
            country: e.pressing.country,
            barcode: e.pressing.barcode,
        },
        tracks: e
            .tracks
            .into_iter()
            .map(|t| BridgeTrackUserEdit {
                title: t.title,
                side: t.side,
                track_number: t.track_number,
                artist_names: t.artist_names,
            })
            .collect(),
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn raw_release_edit_from_bridge(
    e: BridgeRawReleaseEdit,
) -> bae_core::import::RawReleaseEdit {
    bae_core::import::RawReleaseEdit {
        album_title: e.album_title,
        album_artist_text: e.album_artist_text,
        pressing: bae_core::import::RawPressingEdit {
            year: e.pressing.year,
            format: e.pressing.format,
            label: e.pressing.label,
            catalog_number: e.pressing.catalog_number,
            country: e.pressing.country,
            barcode: e.pressing.barcode,
        },
        tracks: e
            .tracks
            .into_iter()
            .map(|t| bae_core::import::RawTrackEdit {
                id: t.id,
                title: t.title,
                artist_text: t.artist_text,
                side: t.side,
                track_number: t.track_number,
            })
            .collect(),
    }
}

#[cfg(feature = "desktop")]
pub(crate) fn raw_release_edit_to_bridge(
    e: bae_core::import::RawReleaseEdit,
) -> BridgeRawReleaseEdit {
    BridgeRawReleaseEdit {
        album_title: e.album_title,
        album_artist_text: e.album_artist_text,
        pressing: BridgeRawPressingEdit {
            year: e.pressing.year,
            format: e.pressing.format,
            label: e.pressing.label,
            catalog_number: e.pressing.catalog_number,
            country: e.pressing.country,
            barcode: e.pressing.barcode,
        },
        tracks: e
            .tracks
            .into_iter()
            .map(|t| BridgeRawTrackEdit {
                id: t.id,
                title: t.title,
                artist_text: t.artist_text,
                side: t.side,
                track_number: t.track_number,
            })
            .collect(),
    }
}
