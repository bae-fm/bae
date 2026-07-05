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
    /// The library's cloud provider, or `None` when it syncs nowhere
    /// (local-only). The UI renders the name: brand names pass through, the
    /// generic cases resolve a catalog key.
    pub cloud_provider: Option<BridgeCloudProvider>,
    pub is_active: bool,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLibraryImageType {
    Cover,
    Artist,
}

impl From<bae_core::db::LibraryImageType> for BridgeLibraryImageType {
    fn from(value: bae_core::db::LibraryImageType) -> Self {
        match value {
            bae_core::db::LibraryImageType::Cover => Self::Cover,
            bae_core::db::LibraryImageType::Artist => Self::Artist,
        }
    }
}

impl From<BridgeLibraryImageType> for bae_core::db::LibraryImageType {
    fn from(value: BridgeLibraryImageType) -> Self {
        match value {
            BridgeLibraryImageType::Cover => Self::Cover,
            BridgeLibraryImageType::Artist => Self::Artist,
        }
    }
}

/// A reference to a host-provided library image (a cover or an artist image):
/// the image kind, subject id, and content version. The UI passes the whole ref
/// to `fetch_image_bytes`, so core dispatches to the known image namespace.
/// `version` is the image row's `_updated_at`, which moves when the bytes
/// change. Mirrors `bae_core::album_detail::ImageRef`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImageRef {
    pub id: String,
    pub version: String,
    pub image_type: BridgeLibraryImageType,
}

impl BridgeImageRef {
    pub fn from_core(r: bae_core::album_detail::ImageRef) -> Self {
        Self {
            id: r.id,
            version: r.version,
            image_type: r.image_type.into(),
        }
    }

    pub fn into_core(self) -> bae_core::album_detail::ImageRef {
        bae_core::album_detail::ImageRef {
            id: self.id,
            version: self.version,
            image_type: self.image_type.into(),
        }
    }
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
    /// Reference to the album's cover (the primary release's cover), or `None`
    /// when it has no cover. The UI fetches the bytes by id and caches under
    /// `(id, version)`; the version moves when the cover changes.
    pub cover: Option<BridgeImageRef>,
}

/// A release's storage state — Local (a local file the user owns) or Remote
/// (a cloud blob). Mirrors `bae_core::album_detail::ReleaseStorageState`. Whether
/// a remote release is kept offline is the ORTHOGONAL `pinned` bool on
/// `BridgeReleaseSummary`/`BridgeRelease`, never folded into this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageState {
    Local,
    Remote,
}

impl BridgeReleaseStorageState {
    pub fn from_core(state: bae_core::album_detail::ReleaseStorageState) -> Self {
        use bae_core::album_detail::ReleaseStorageState;
        match state {
            ReleaseStorageState::Local => Self::Local,
            ReleaseStorageState::Remote => Self::Remote,
        }
    }
}

/// A storage transition available from the release "Storage…" sheet.
/// Mirrors `bae_core::album_detail::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageAction {
    MakeRemote,
    Pin,
    Unpin,
    MakeLocal,
}

impl BridgeReleaseStorageAction {
    pub fn from_core(action: bae_core::album_detail::ReleaseStorageAction) -> Self {
        use bae_core::album_detail::ReleaseStorageAction;
        match action {
            ReleaseStorageAction::MakeRemote => Self::MakeRemote,
            ReleaseStorageAction::Pin => Self::Pin,
            ReleaseStorageAction::Unpin => Self::Unpin,
            ReleaseStorageAction::MakeLocal => Self::MakeLocal,
        }
    }

    fn transfer_loc_key(self) -> &'static str {
        match self {
            Self::Pin => "core.transfer.action.pin",
            Self::Unpin => "core.transfer.action.unpin",
            Self::MakeRemote => "core.transfer.action.manage",
            Self::MakeLocal => "core.transfer.action.unmanage",
        }
    }
}

/// Localization key for a transfer's present-continuous progress verb
/// ("Pinning for offline"). The UI resolves it against the `Core` table.
#[uniffi::export]
pub fn bridge_transfer_action_key(action: BridgeReleaseStorageAction) -> String {
    action.transfer_loc_key().to_string()
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
    /// The release's storage state — Local (local) or Remote (cloud).
    pub storage_state: BridgeReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the ORTHOGONAL coven-cache property. Meaningful only when
    /// `storage_state` is `Remote`. Kept separate from `storage_state` so the UI
    /// never conflates "in the cloud" with "kept offline".
    pub pinned: bool,
    /// Storage transitions available right now, gated on cloud-home by the
    /// core. The in-flight-uploads gate lives in the UI: it consults
    /// per-release outbox progress before showing actions. The Storage
    /// Manager row context menu renders these.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    pub file_count: i64,
    pub total_size: i64,
    /// Reference to this release's own cover (image id + version), or `None` when
    /// it has no cover. Keyed on the release id so each release renders its own
    /// art; the UI caches the bytes under `(id, version)`.
    pub cover: Option<BridgeImageRef>,
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
    /// The release's storage state — Local (local) or Remote (cloud).
    pub storage_state: BridgeReleaseStorageState,
    /// Whether coven keeps this release's blobs pinned (kept offline) on this
    /// device — the ORTHOGONAL coven-cache property, separate from `storage_state`.
    pub pinned: bool,
    /// Storage transitions available right now, gated on cloud-home by the
    /// core. The in-flight-uploads gate lives in the UI: it consults
    /// per-release outbox progress before showing actions.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    pub tracks: Vec<BridgeTrack>,
    pub track_groups: Vec<BridgeTrackGroup>,
    pub files: Vec<BridgeFile>,
    pub image_files: Vec<BridgeFile>,
    /// Cover slot first (if the release has one), then every image file the
    /// release has. Each item's bytes are read through `fetch_gallery_bytes`,
    /// which takes the item's `source` and dispatches the read. Consumers render
    /// this as-is.
    pub gallery_items: Vec<BridgeGalleryItem>,
    /// Total duration across all tracks, in milliseconds. The UI formats it.
    pub total_duration_ms: i64,
    pub file_count: i64,
    pub total_size: i64,
    /// Reference to this release's own cover (image id + version), or `None` when
    /// it has no cover. Mirrors `BridgeReleaseSummary.cover` so a summary rebuilt
    /// from this fat payload keeps its per-release art.
    pub cover: Option<BridgeImageRef>,
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

/// Structured track position. The case carries the domain decision (sided
/// physical medium / multi-disc digital / flat single-disc); the UI composes
/// the position string ("A1", "2-3", "5") mechanically from the fields and
/// resolves the "Side"/"Disc" header word from `bridge_track_*` catalog keys.
/// No prose crosses the bridge — only the side letter, disc number, optional
/// track number, and the case.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackPosition {
    /// Vinyl/cassette: header "Side {side_letter}", position "{side_letter}{number}".
    Sided {
        side_letter: String,
        number: Option<i32>,
    },
    /// Multi-disc digital: header "Disc {disc}", position "{disc}-{number}".
    Disc { disc: i32, number: Option<i32> },
    /// Single-disc digital: position "{number}", no header.
    Flat { number: Option<i32> },
}

impl BridgeTrackPosition {
    pub(crate) fn from_core(p: bae_core::album_detail::TrackPosition) -> Self {
        use bae_core::album_detail::TrackPosition;
        match p {
            TrackPosition::Sided {
                side_letter,
                number,
            } => Self::Sided {
                side_letter,
                number,
            },
            TrackPosition::Disc { disc, number } => Self::Disc { disc, number },
            TrackPosition::Flat { number } => Self::Flat { number },
        }
    }
}

/// A track group's side discriminant — the header the UI renders ("Side A" /
/// "Disc 2"). `Flat` means no header (single-disc digital). Distinct from
/// `BridgeTrackPosition` because a header carries no per-track number.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackSide {
    Sided { side_letter: String },
    Disc { disc: i32 },
    Flat,
}

impl BridgeTrackSide {
    pub(crate) fn from_core(s: bae_core::album_detail::TrackSide) -> Self {
        use bae_core::album_detail::TrackSide;
        match s {
            TrackSide::Sided { side_letter } => Self::Sided { side_letter },
            TrackSide::Disc { disc } => Self::Disc { disc },
            TrackSide::Flat => Self::Flat,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackGroup {
    /// The group's side discriminant; the UI renders the "Side A" / "Disc 2"
    /// header from it (`Flat` means no header).
    pub side: BridgeTrackSide,
    pub tracks: Vec<BridgeTrack>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrack {
    pub id: String,
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    pub duration_ms: Option<i64>,
    /// Effective comma-joined artist names for display (the track's own
    /// artists when it has per-track artist rows, otherwise the album
    /// artists). Always populated.
    pub artist_names: String,
    /// Structured position: the UI composes "A1"/"2-3"/"5" from the case.
    pub position: BridgeTrackPosition,
}

/// Localization key for a track group's header word, given its side, or `None`
/// for `Flat` (single-disc digital has no header). The UI resolves the key and
/// substitutes the side letter / disc number the side carries.
#[uniffi::export]
pub fn bridge_track_header_key(side: BridgeTrackSide) -> Option<String> {
    match side {
        BridgeTrackSide::Sided { .. } => Some("core.track.side".to_string()),
        BridgeTrackSide::Disc { .. } => Some("core.track.disc".to_string()),
        BridgeTrackSide::Flat => None,
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFile {
    pub id: String,
    pub original_filename: String,
    pub file_size: i64,
    pub content_type: String,
    pub is_image: bool,
    /// Structured audio format; `None` for non-audio files. The UI composes the
    /// one-line descriptor from it.
    pub audio_format: Option<BridgeAudioFormat>,
}

/// Mirror of bae-core's `AudioFormat`. The UI composes "FLAC · 44.1 kHz ·
/// 16-bit · stereo" from these parts: the codec is a proper noun, the channel
/// count maps to a localized word (`bridge_audio_channels_key`), and numbers
/// format per locale.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAudioFormat {
    pub codec: String,
    pub sample_rate_hz: i64,
    pub bits_per_sample: Option<i64>,
    pub bitrate_kbps: Option<i64>,
    pub channels: i64,
}

pub(crate) fn audio_format_to_bridge(f: bae_core::album_detail::AudioFormat) -> BridgeAudioFormat {
    BridgeAudioFormat {
        codec: f.codec,
        sample_rate_hz: f.sample_rate_hz,
        bits_per_sample: f.bits_per_sample,
        bitrate_kbps: f.bitrate_kbps,
        channels: f.channels,
    }
}

/// Localization key for a channel count's word ("mono"/"stereo"), or `None` for
/// counts the UI renders as "{n}ch". One source of the keys for every platform.
#[uniffi::export]
pub fn bridge_audio_channels_key(channels: i64) -> Option<String> {
    match channels {
        1 => Some("core.audio.channels.mono".to_string()),
        2 => Some("core.audio.channels.stereo".to_string()),
        _ => None,
    }
}

/// Localization key for a cloud provider's display name, or `None` for the
/// brand-name providers the UI passes through verbatim (iCloud, Google Drive,
/// Dropbox, OneDrive). `None` provider means local-only. One source of these
/// keys for every platform; the UI resolves the key through its string catalog.
#[uniffi::export]
pub fn bridge_cloud_provider_label_key(provider: Option<BridgeCloudProvider>) -> Option<String> {
    match provider {
        None => Some("core.cloud.local_only".to_string()),
        Some(BridgeCloudProvider::S3) => Some("core.cloud.s3_compatible".to_string()),
        Some(
            BridgeCloudProvider::CloudKit
            | BridgeCloudProvider::GoogleDrive
            | BridgeCloudProvider::Dropbox
            | BridgeCloudProvider::OneDrive,
        ) => None,
    }
}

/// Which byte source a gallery slot is read from — each variant self-contained.
/// `Cover` carries the cover's `BridgeImageRef` (id + version); `ReleaseFile`
/// carries its file id. The UI passes the whole value to `fetch_gallery_bytes`
/// and never inspects it to pick a fetch.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeGallerySource {
    Cover { image: BridgeImageRef },
    ReleaseFile { file_id: String },
}

impl BridgeGallerySource {
    pub fn into_core(self) -> bae_core::album_detail::GallerySource {
        match self {
            BridgeGallerySource::Cover { image } => {
                bae_core::album_detail::GallerySource::Cover(image.into_core())
            }
            BridgeGallerySource::ReleaseFile { file_id } => {
                bae_core::album_detail::GallerySource::ReleaseFile { file_id }
            }
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeGalleryItem {
    /// Stable list/ForEach identity only: `"cover"` for the cover slot, else the
    /// release-file id. The fetch id lives in `source`.
    pub id: String,
    /// Display label: "Cover" or the file's original filename.
    pub label: String,
    /// Which byte source to read this slot from.
    pub source: BridgeGallerySource,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumDetail {
    pub album: BridgeAlbum,
    pub releases: Vec<BridgeRelease>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeRepeatMode {
    Off,
    Track,
    Context,
}

impl BridgeRepeatMode {
    pub fn to_core(self) -> bae_core::playback::RepeatMode {
        match self {
            Self::Off => bae_core::playback::RepeatMode::Off,
            Self::Track => bae_core::playback::RepeatMode::Track,
            Self::Context => bae_core::playback::RepeatMode::Context,
        }
    }

    pub fn from_core(mode: bae_core::playback::RepeatMode) -> Self {
        match mode {
            bae_core::playback::RepeatMode::Off => Self::Off,
            bae_core::playback::RepeatMode::Track => Self::Track,
            bae_core::playback::RepeatMode::Context => Self::Context,
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
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSidePausePrompt {
    pub id: String,
    pub title_key: String,
    pub side_letter: String,
    pub message_key: String,
}

impl BridgeSidePausePrompt {
    pub(crate) fn from_core(prompt: bae_core::playback::PlaybackSidePausePrompt) -> Self {
        Self {
            id: prompt.id,
            title_key: prompt.title_key.to_string(),
            side_letter: prompt.side_letter,
            message_key: prompt.message_key.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgePlaybackPauseReason {
    Manual,
    SideEnded { prompt: BridgeSidePausePrompt },
}

impl BridgePlaybackPauseReason {
    pub(crate) fn from_core(reason: bae_core::playback::PlaybackPauseReason) -> Self {
        match reason {
            bae_core::playback::PlaybackPauseReason::Manual => Self::Manual,
            bae_core::playback::PlaybackPauseReason::SideEnded(prompt) => Self::SideEnded {
                prompt: BridgeSidePausePrompt::from_core(prompt),
            },
        }
    }
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePreviewState {
    Idle,
    Playing { path: String, duration_ms: u64 },
    Paused { path: String, duration_ms: u64 },
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

/// Mirror of bae-core's `InvalidReason`. The UI localizes each variant via its
/// catalog key (`bridge_invalid_reason_key`), interpolating the path where set.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInvalidReason {
    CorruptAudioFile { path: String },
    CorruptImage { path: String },
    CueMissingAudio,
    NoValidAudio,
}

impl BridgeInvalidReason {
    pub(crate) fn loc_key(&self) -> &'static str {
        match self {
            Self::CorruptAudioFile { .. } => "core.import.invalid.corrupt_audio",
            Self::CorruptImage { .. } => "core.import.invalid.corrupt_image",
            Self::CueMissingAudio => "core.import.invalid.cue_missing_audio",
            Self::NoValidAudio => "core.import.invalid.no_valid_audio",
        }
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub(crate) fn invalid_reason_to_bridge(r: bae_core::import::InvalidReason) -> BridgeInvalidReason {
    use bae_core::import::InvalidReason as R;
    match r {
        R::CorruptAudioFile { path } => BridgeInvalidReason::CorruptAudioFile { path },
        R::CorruptImage { path } => BridgeInvalidReason::CorruptImage { path },
        R::CueMissingAudio => BridgeInvalidReason::CueMissingAudio,
        R::NoValidAudio => BridgeInvalidReason::NoValidAudio,
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
    /// Why the folder failed validation — the UI localizes this typed reason.
    pub reason: BridgeInvalidReason,
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
    /// Absolute filesystem path of the CUE file on disk.
    pub cue_local_path: String,
    pub flac_name: String,
    /// Absolute filesystem path of the audio file on disk.
    pub flac_local_path: String,
    pub total_size: u64,
    pub track_count: u32,
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

pub(crate) fn import_step_to_bridge(s: bae_core::import::ImportStep) -> BridgeImportStep {
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn lookup_failure_to_bridge(f: bae_core::signals::LookupFailure) -> BridgeLookupFailure {
    use bae_core::signals::LookupFailure;
    match f {
        LookupFailure::Network => BridgeLookupFailure::Network,
        LookupFailure::Provider { status } => BridgeLookupFailure::Provider { status },
        LookupFailure::Timeout => BridgeLookupFailure::Timeout,
        LookupFailure::ArtworkAnalysis => BridgeLookupFailure::ArtworkAnalysis,
        LookupFailure::Diagnostic { detail } => BridgeLookupFailure::Diagnostic { detail },
    }
}

/// Localization key for a lookup failure's user-facing line, or `None` for
/// `Diagnostic` (which has no translated copy — the UI shows a generic line
/// plus the opaque `detail`). `Provider` resolves to one of two keys: the
/// status-bearing line when a code was observed, or a no-status fallback when
/// not — so the UI never has to decide which message a missing status takes.
/// One source of these keys for every platform; the UI resolves the key
/// through its string catalog.
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn signal_state_to_bridge(s: bae_core::identify::SignalState) -> BridgeSignalState {
    use bae_core::identify::SignalState;
    match s {
        SignalState::LookingUp => BridgeSignalState::LookingUp,
        SignalState::Found { count } => BridgeSignalState::Found { count },
        SignalState::NoMatch => BridgeSignalState::NoMatch,
        SignalState::Skipped => BridgeSignalState::Skipped,
        SignalState::Failed { failure } => BridgeSignalState::Failed {
            failure: lookup_failure_to_bridge(failure),
        },
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
    /// downloaded yet, or an in-core decode failure. The UI renders `reason`
    /// for its locale; playback itself falls back to stopped.
    PlaybackError {
        reason: BridgePlaybackErrorReason,
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
        reason: BridgePlaybackPauseReason,
    },
    /// Position tick — goes to NSView.
    PlaybackProgress {
        track_id: String,
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), so the media-control
        /// update reads it from the event instead of the now-playing slice.
        duration_ms: u64,
        progress: f64,
    },
    /// Position after a seek completes — goes to NSView.
    PlaybackSeeked {
        track_id: String,
        position_ms: u64,
        /// User-facing track duration (pregap-adjusted), so the media-control
        /// update reads it from the event instead of the now-playing slice.
        duration_ms: u64,
        progress: f64,
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
        manual: Vec<BridgeQueueEntry>,
        context: Option<BridgePlaybackContext>,
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
    },
    PreviewPaused {
        path: String,
        duration_ms: u64,
    },
    /// High-frequency tick — goes to NSView, not store.
    PreviewProgress {
        position_ms: u64,
        progress: f64,
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
        step: Option<BridgeImportStep>,
    },
    /// High-frequency loudness-measurement tick — the UI routes it to a native
    /// leaf view (a determinate bar driven by `fraction`, labelled "N / M"), not
    /// the coarse candidate row.
    CandidateImportLoudnessProgress {
        key: String,
        tracks_done: u32,
        tracks_total: u32,
        fraction: f32,
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
        error: BridgeError,
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
    /// Sync loop's current error state. `None` clears a prior failure. When
    /// set, it's a `BridgeError::Diagnostic` whose category keys the generic
    /// line and whose detail is the opaque, log-only error chain offered in a
    /// copyable disclosure.
    SyncError {
        error: Option<BridgeError>,
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
    /// A pin/unpin/manage/unmanage transition started. The UI composes the
    /// localized line and shows an in-flight indicator on the release row until
    /// `ReleaseTransferEnded`.
    ReleaseTransferProgress {
        release_id: String,
        action: BridgeReleaseStorageAction,
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
    /// The in-memory export queue changed — the Storage Manager re-renders its
    /// Exporting pane from this.
    ExportQueueChanged {
        snapshot: BridgeExportSnapshot,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: BridgeError,
    },
    ErrorCleared,
}

/// The dominant activity of a slice of the upload queue, for the storage-row
/// badge. Mirror of bae-core's `UploadActivity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadActivity {
    Uploading,
    Retrying,
    Queued,
}

/// One pending cloud delete.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDeleteOp {
    pub id: i64,
    pub cloud_key: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// Per-state counts, byte progress, and a derived badge `activity`. Used
/// per-release (the storage-row badge reads `activity`; storage-action gates
/// read the counts) and as the overall total (queue counts, ETA, summary band).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The badge activity for this slice; `None` when idle. Per-release entries
    /// are never idle, so theirs is always set; the overall total's is `None`
    /// only when the whole queue is empty.
    pub activity: Option<BridgeUploadActivity>,
}

/// A queued download's state.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeDownloadState {
    Queued,
    Active {
        progress: BridgeDownloadTransferProgress,
    },
    Failed {
        error: String,
    },
}

/// Byte progress for the active download. Mirrors the payload emitted by the
/// transfer reading the release's blobs.
#[derive(Debug, Clone, Default, PartialEq, uniffi::Record)]
pub struct BridgeDownloadTransferProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
}

/// One queued download — a whole release being pinned. Mirror of bae-core's
/// `DownloadOp`; carries raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDownloadOp {
    pub release_id: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeDownloadState,
}

/// Per-state counts for the download queue. Used per-release (the storage-row
/// "Downloading" badge) and as the overall total (the pane header).
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
}

/// A queued export's state. Mirror of bae-core's `ExportState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeExportState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// One queued export — a whole release being copied out verbatim to a folder.
/// Mirror of bae-core's `ExportOp`; carries raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeExportOp {
    pub release_id: String,
    /// The chosen destination directory; the release's source folder is
    /// reconstructed under it.
    pub target_dir: String,
    /// Album title for display.
    pub title: String,
    pub file_count: i64,
    /// Total size in bytes across the release's files. The UI formats it.
    pub total_size: i64,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
    pub state: BridgeExportState,
}

/// Per-state counts for the export queue, driving the pane header. No bytes:
/// exports track an overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeExportProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory export queue snapshot the Storage Manager's Exporting pane
/// renders. Mirror of bae-core's `ExportSnapshot`; the UI renders it verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeExportSnapshot {
    pub exports: Vec<BridgeExportOp>,
    pub total: BridgeExportProgress,
    /// True when the user paused the export queue. Drives the pause/resume
    /// toggle in the Exporting pane.
    pub paused: bool,
}

/// Where release exports write — the browser download-folder model. Mirror of
/// bae-core's `ExportLocation`. `Fixed` carries the configured directory as a
/// path string.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeExportLocation {
    AskEachTime,
    Fixed { dir: String },
}

/// Which metadata tags a single-track export embeds. Mirror of bae-core's
/// `ExportMetadata` — the FFI boundary requires a `uniffi::Record`, so the
/// fields are restated here rather than reusing the core type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeExportMetadata {
    pub title: bool,
    pub artist: bool,
    pub album: bool,
    pub year: bool,
    pub track_number: bool,
    pub disc_number: bool,
    pub cover_art: bool,
}

/// A release's pending uploads, grouped for the queue pane's per-release rows.
/// Mirror of bae-core's `UploadReleaseGroup`. `release_id` is `None` for the
/// orphaned-files bucket; `display_title` is the row's label, resolved by core.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReleaseGroup {
    pub release_id: Option<String>,
    pub display_title: String,
    pub file_count: u32,
    pub progress: BridgeUploadProgress,
}

/// The cloud-outbox processing snapshot the Storage Manager renders. The
/// counts, per-release aggregates, one-line `summary`, throughput, and ETA
/// are computed from bae-core's grouped snapshot; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    /// Pending uploads grouped by release for the queue pane's per-release rows.
    pub upload_groups: Vec<BridgeUploadReleaseGroup>,
    pub deletes: Vec<BridgeDeleteOp>,
    /// Per-release aggregate derived from `upload_groups`, keyed by release id.
    /// Releases with no pending work are absent from the map.
    pub per_release: std::collections::HashMap<String, BridgeUploadProgress>,
    pub total: BridgeUploadProgress,
    /// Total bytes of the files uploading right now. The master progress bar
    /// shows `total.bytes_done` of this — the live transfer — rather than
    /// progress against the whole backlog.
    pub active_bytes_total: u64,
    /// Derived from `deletes.len()`.
    pub pending_deletes: u32,
    /// True when the user has paused the upload pipeline. Drives the
    /// pause/resume toggle and suppresses throughput/ETA in the UI.
    pub paused: bool,
    /// Rolling-window upload throughput in bytes per second. The UI formats it.
    pub throughput_bps: u64,
    /// Estimated seconds remaining at the current rate. The UI formats it.
    pub eta_seconds: Option<u64>,
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
    /// Raw position string as the metadata source reports it ("A1", "1",
    /// "1-2", or arbitrary prose). Shown verbatim in the import preview.
    pub position: String,
    pub side: u32,
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

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    pub library_id: String,
    pub library_name: String,
    pub library_path: String,
    pub encryption_key_stored: bool,
    pub encryption_key_fingerprint: Option<String>,
    pub pause_between_sides: bool,
    /// Where release exports write: prompt each time, or a fixed folder.
    pub export_location: BridgeExportLocation,
    /// Template rendering a single-track export's suggested filename.
    pub export_filename_template: String,
    /// Which metadata tags a single-track export embeds.
    pub export_metadata: BridgeExportMetadata,
    pub mcp: BridgeMcpConfig,
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

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMcpConfig {
    pub enabled: bool,
    pub port: u16,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMcpServerStatus {
    Disabled,
    Running { url: String },
    Error { error: BridgeMcpServerError },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMcpServerError {
    InvalidConfig { detail: String },
    TokenUnavailable { detail: String },
    BindFailed { detail: String },
    ServerFailed { detail: String },
}

/// Cloud sync settings for a connected provider. `provider` carries the
/// provider-specific fields; the rest are shared across providers. Whether
/// sync is actually running is `BridgeConfig.sync_ready`, kept orthogonal.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncConfig {
    pub provider: BridgeSyncProvider,
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
    pub composers: Vec<BridgeComposerSummary>,
    pub works: Vec<BridgeWorkSummary>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumSearchResult {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub artist_name: String,
    /// Reference to the album's cover (the primary release's cover), or `None`.
    /// The UI fetches the bytes by id and caches under `(id, version)`.
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackSearchResult {
    pub id: String,
    pub title: String,
    pub duration_ms: Option<i64>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSummary {
    pub artist_id: String,
    pub name: String,
    pub sort_name: Option<String>,
    pub work_count: i64,
    pub linked_release_count: i64,
    pub unlinked_credit_count: i64,
    pub image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkSummary {
    pub work_id: String,
    pub title: String,
    pub disambiguation: Option<String>,
    pub work_type: Option<String>,
    pub parent_work_id: Option<String>,
    pub composer_names: Option<String>,
    pub linked_release_count: i64,
    pub representative_release_id: Option<String>,
    pub representative_cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleaseRoleSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub source: BridgeMetadataSource,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackRoleSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub artist_id: String,
    pub artist_name: String,
    pub source: BridgeMetadataSource,
    pub source_credit: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkTrackSummary {
    pub track_id: String,
    pub track_title: String,
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkReleaseSummary {
    pub release_id: String,
    pub album_id: String,
    pub album_title: String,
    pub display_name: String,
    pub format: Option<String>,
    pub cover: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerDetail {
    pub composer: BridgeComposerSummary,
    pub work_groups: Vec<BridgeComposerWorkGroup>,
    pub unlinked_release_roles: Vec<BridgeReleaseRoleSummary>,
    pub unlinked_track_roles: Vec<BridgeTrackRoleSummary>,
    pub default_work_id: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerWorkGroup {
    pub id: String,
    pub parent: Option<BridgeWorkSummary>,
    pub works: Vec<BridgeWorkSummary>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeWorkDetail {
    pub work: BridgeWorkSummary,
    pub child_works: Vec<BridgeWorkSummary>,
    pub releases: Vec<BridgeWorkReleaseSummary>,
    pub tracks: Vec<BridgeWorkTrackSummary>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeComposerSortField {
    Name,
    WorkCount,
    LinkedReleaseCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeComposerSortCriterion {
    pub field: BridgeComposerSortField,
    pub direction: BridgeSortDirection,
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
    Remote,
    Local,
    Uploading,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueEntry {
    /// Per-instance id: the same track queued twice yields two entries with two
    /// ids, so the UI keys each row on a stable unique identity and targets
    /// remove/reorder/skip at one instance.
    pub entry_id: String,
    pub track_id: String,
    pub title: String,
    pub artist_names: String,
    pub duration_ms: Option<i64>,
    pub album_title: String,
    pub cover_image_id: Option<String>,
}

/// Which kind of source the context plays from, so the UI labels the section
/// (a release's "Playing From" vs the whole library). The discriminant of
/// `bae_core::playback::ContextSource`; the release id stays in core (the UI
/// labels by kind, not by id here). FFI mirror of the core enum's variants.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackSourceKind {
    Release,
    Library,
}

impl BridgePlaybackSourceKind {
    pub(crate) fn from_core(source: &bae_core::playback::ContextSource) -> Self {
        match source {
            bae_core::playback::ContextSource::Release(_) => Self::Release,
            bae_core::playback::ContextSource::Library => Self::Library,
        }
    }
}

/// The context lane (what the queue is playing from), carried by `QueueUpdated`
/// alongside the manual lane so each UI renders the two as distinct sections:
/// its kind (release vs library, for the section label), its not-yet-played tail,
/// plus whether it was ordered by shuffle (the UI shows a shuffle indicator when
/// so).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgePlaybackContext {
    pub kind: BridgePlaybackSourceKind,
    pub shuffled: bool,
    pub upcoming: Vec<BridgeQueueEntry>,
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

/// The storage state the user picks for an import — Local (keep the files in
/// place) or Remote (upload to the cloud). Mirrors
/// `bae_core::import::StorageMode`. Whether a remote import is kept offline is the
/// ORTHOGONAL `pin` argument on `start_import`, never folded into this enum.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeStorageMode {
    Local,
    Remote,
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
    pub needs_oauth: bool,
}

/// A device in the library's membership chain.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMember {
    /// Hex-encoded Ed25519 public key — the device's stable identity.
    pub pubkey: String,
    pub role: BridgeMemberRole,
    /// True for the device this app is running on.
    pub is_self: bool,
    /// Short display identity — the first 8 characters of the pubkey.
    pub fingerprint: String,
    /// Whether the running device may remove this one (owner-only, never self).
    pub can_remove: bool,
}

/// The library's membership: its devices and whether the running device is an
/// owner (the gate the UI uses to show inviting and removal controls).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMembership {
    pub members: Vec<BridgeMember>,
    pub self_is_owner: bool,
}

/// A member's role. Mirrors coven's `MemberRole`. bae adds devices as `Member`;
/// the founding device is `Owner`. `Follower` (read-only) exists in coven's model
/// but bae does not create it — it is mapped through so the conversion stays total.
#[derive(Debug, Clone, Copy, PartialEq, uniffi::Enum)]
pub enum BridgeMemberRole {
    Owner,
    Member,
    Follower,
}

/// Decoded join-request code: the joining device's public key and its
/// fingerprint, shown to an existing member for approval before inviting it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeJoinRequestInfo {
    pub pubkey: String,
    /// Short display identity — the first 8 characters of the pubkey.
    pub fingerprint: String,
    pub email: Option<String>,
}

/// This device's join-request code and the fingerprint it encodes, so the
/// joining device shows its own identity without decoding the code it generated.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeJoinRequest {
    pub code: String,
    /// Short display identity — the first 8 characters of this device's pubkey.
    pub fingerprint: String,
}

/// Decoded invite code info for UI preview (before joining).
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeInviteCodeInfo {
    pub library_id: String,
    pub library_name: String,
    pub owner_pubkey: String,
    /// Short display identity of the library owner — the first 8 characters of
    /// the owner pubkey.
    pub owner_fingerprint: String,
    pub cloud_provider: BridgeCloudProvider,
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

/// The kind of diagnostic failure. The UI shows one generic localized line per
/// category; `detail` is the underlying Rust error chain — logged and offered in
/// a copyable disclosure, never translated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeErrorCategory {
    Database,
    Config,
    Internal,
    Import,
    Export,
}

/// What a `NotFound` was looking for, so the UI can localize "… not found".
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeEntityKind {
    Library,
    Album,
    Release,
    Track,
    File,
}

#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum BridgeError {
    /// The user cancelled — the UI shows nothing.
    #[error("cancelled")]
    Cancelled,
    /// A specific entity was missing. User-facing and keyed; the UI localizes it.
    #[error("not found: {entity:?} {id}")]
    NotFound {
        entity: BridgeEntityKind,
        id: String,
    },
    /// A diagnostic failure. The UI shows a generic per-category line; `detail`
    /// is the opaque Rust error chain for logs / a copyable disclosure, never
    /// translated.
    #[error("{category:?}: {detail}")]
    Diagnostic {
        category: BridgeErrorCategory,
        detail: String,
    },
}

impl BridgeError {
    pub(crate) fn diagnostic(
        category: BridgeErrorCategory,
        detail: impl std::fmt::Display,
    ) -> Self {
        BridgeError::Diagnostic {
            category,
            detail: detail.to_string(),
        }
    }
    pub(crate) fn database(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Database, detail)
    }
    pub(crate) fn config(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Config, detail)
    }
    pub(crate) fn internal(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Internal, detail)
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn import(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Import, detail)
    }
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn export(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Export, detail)
    }
}

/// Why playback couldn't start or continue. The two cloud-only "not playable
/// yet" cases are user-actionable and keyed (`bridge_playback_error_reason_key`);
/// every in-core failure is un-enumerable and rides in `Diagnostic` — the UI
/// renders it through the same `BridgeError` path (generic per-category line +
/// copyable, log-only detail).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackErrorReason {
    /// A remote cloud-only track isn't downloaded and sync is disconnected —
    /// the user reconnects to play it.
    SyncDisconnected,
    /// A remote track's cloud upload is still queued — the user waits for it
    /// to finish.
    UploadPending,
    /// Any other failure. Carries the underlying `BridgeError`; the UI renders
    /// its generic per-category line plus the opaque, log-only detail.
    Diagnostic { error: BridgeError },
}

/// Localization key for a `BridgeErrorCategory`'s generic user-facing line. The
/// UI resolves this through its string catalog; `detail` is never translated.
/// One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_error_category_key(category: BridgeErrorCategory) -> String {
    match category {
        BridgeErrorCategory::Database => "core.error.category.database",
        BridgeErrorCategory::Config => "core.error.category.config",
        BridgeErrorCategory::Internal => "core.error.category.internal",
        BridgeErrorCategory::Import => "core.error.category.import",
        BridgeErrorCategory::Export => "core.error.category.export",
    }
    .to_string()
}

/// Localization key for a `BridgeEntityKind`'s "… not found" line. One source
/// of these keys for every platform.
#[uniffi::export]
pub fn bridge_entity_not_found_key(entity: BridgeEntityKind) -> String {
    match entity {
        BridgeEntityKind::Library => "core.error.not_found.library",
        BridgeEntityKind::Album => "core.error.not_found.album",
        BridgeEntityKind::Release => "core.error.not_found.release",
        BridgeEntityKind::Track => "core.error.not_found.track",
        BridgeEntityKind::File => "core.error.not_found.file",
    }
    .to_string()
}

/// Localization key for the actionable `BridgePlaybackErrorReason` variants, or
/// `None` for `Diagnostic` (which the UI renders through the `BridgeError`
/// category path instead). One source of these keys for every platform.
#[uniffi::export]
pub fn bridge_playback_error_reason_key(reason: &BridgePlaybackErrorReason) -> Option<String> {
    match reason {
        BridgePlaybackErrorReason::SyncDisconnected => {
            Some("core.playback.error.sync_disconnected".to_string())
        }
        BridgePlaybackErrorReason::UploadPending => {
            Some("core.playback.error.upload_pending".to_string())
        }
        BridgePlaybackErrorReason::Diagnostic { .. } => None,
    }
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

pub(crate) fn bridge_error_category(
    category: bae_core::ui::UiErrorCategory,
) -> BridgeErrorCategory {
    use bae_core::ui::UiErrorCategory;
    match category {
        UiErrorCategory::Database => BridgeErrorCategory::Database,
        UiErrorCategory::Config => BridgeErrorCategory::Config,
        UiErrorCategory::Internal => BridgeErrorCategory::Internal,
        UiErrorCategory::Import => BridgeErrorCategory::Import,
        UiErrorCategory::Export => BridgeErrorCategory::Export,
    }
}

fn bridge_entity_kind(entity: bae_core::ui::UiEntityKind) -> BridgeEntityKind {
    use bae_core::ui::UiEntityKind;
    match entity {
        UiEntityKind::Library => BridgeEntityKind::Library,
        UiEntityKind::Album => BridgeEntityKind::Album,
        UiEntityKind::Release => BridgeEntityKind::Release,
        UiEntityKind::Track => BridgeEntityKind::Track,
        UiEntityKind::File => BridgeEntityKind::File,
    }
}

pub(crate) fn bridge_error(error: bae_core::ui::UiError) -> BridgeError {
    use bae_core::ui::UiError;
    match error {
        UiError::NotFound { entity, id } => BridgeError::NotFound {
            entity: bridge_entity_kind(entity),
            id,
        },
        UiError::Diagnostic { category, detail } => BridgeError::Diagnostic {
            category: bridge_error_category(category),
            detail,
        },
    }
}

pub(crate) fn bridge_playback_error_reason(
    reason: bae_core::ui::PlaybackErrorReason,
) -> BridgePlaybackErrorReason {
    use bae_core::ui::PlaybackErrorReason;
    match reason {
        PlaybackErrorReason::SyncDisconnected => BridgePlaybackErrorReason::SyncDisconnected,
        PlaybackErrorReason::UploadPending => BridgePlaybackErrorReason::UploadPending,
        PlaybackErrorReason::Diagnostic { error } => BridgePlaybackErrorReason::Diagnostic {
            error: bridge_error(error),
        },
    }
}

/// Map the UI's storage-state choice to the core's `StorageMode`. Pinned-ness is
/// orthogonal — the caller passes the import's `pin` choice separately.
#[cfg(feature = "desktop")]
pub(crate) fn bridge_storage_mode_to_core(
    mode: BridgeStorageMode,
) -> bae_core::import::StorageMode {
    use bae_core::import::StorageMode;
    match mode {
        BridgeStorageMode::Local => StorageMode::Local,
        BridgeStorageMode::Remote => StorageMode::Remote,
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

pub(crate) fn bridge_member_role(
    role: bae_core::sync::sync_manager::MemberRole,
) -> BridgeMemberRole {
    use bae_core::sync::sync_manager::MemberRole;
    match role {
        MemberRole::Owner => BridgeMemberRole::Owner,
        MemberRole::Member => BridgeMemberRole::Member,
        MemberRole::Follower => BridgeMemberRole::Follower,
    }
}

fn bridge_member(m: bae_core::sync::sync_manager::MembershipMember) -> BridgeMember {
    BridgeMember {
        pubkey: m.pubkey,
        role: bridge_member_role(m.role),
        is_self: m.is_self,
        fingerprint: m.fingerprint,
        can_remove: m.can_remove,
    }
}

pub(crate) fn bridge_membership(
    membership: bae_core::sync::sync_manager::Membership,
) -> BridgeMembership {
    BridgeMembership {
        members: membership.members.into_iter().map(bridge_member).collect(),
        self_is_owner: membership.self_is_owner,
    }
}

/// Only the OAuth sign-in and authorize flows still map a bare bridge provider
/// back to core; gated so non-OAuth builds don't carry a dead mapping.
#[cfg(feature = "oauth-providers")]
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
        BridgeStorageFilter::Remote => bae_core::album_detail::StorageFilter::Remote,
        BridgeStorageFilter::Local => bae_core::album_detail::StorageFilter::Local,
        BridgeStorageFilter::Uploading => bae_core::album_detail::StorageFilter::Uploading,
    }
}

pub(crate) fn bridge_composer_sort_to_core(
    c: &BridgeComposerSortCriterion,
) -> bae_core::db::ComposerSortCriterion {
    bae_core::db::ComposerSortCriterion {
        field: match c.field {
            BridgeComposerSortField::Name => bae_core::db::ComposerSortField::Name,
            BridgeComposerSortField::WorkCount => bae_core::db::ComposerSortField::WorkCount,
            BridgeComposerSortField::LinkedReleaseCount => {
                bae_core::db::ComposerSortField::LinkedReleaseCount
            }
        },
        direction: bridge_sort_direction_to_core(&c.direction),
    }
}

fn bridge_sort_direction_to_core(direction: &BridgeSortDirection) -> bae_core::db::SortDirection {
    match direction {
        BridgeSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
        BridgeSortDirection::Descending => bae_core::db::SortDirection::Descending,
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
        direction: bridge_sort_direction_to_core(&c.direction),
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
                duration_ms: t.duration_ms,
                position: t.position,
                side: t.side,
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
                position: t.position,
                side: t.side,
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
        DiscidProgress::Failed { failure, .. } => BridgeDiscidProgress::Failed {
            failure: lookup_failure_to_bridge(failure),
        },
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
        BarcodeProgress::Failed { failure } => BridgeBarcodeProgress::Failed {
            failure: lookup_failure_to_bridge(failure),
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
        year_min: g.year_min,
        year_max: g.year_max,
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
            failure,
            track_count,
        } => BridgeDiscIdSignal::Failed {
            failure: lookup_failure_to_bridge(failure),
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
        BarcodeSignal::Failed { failure, codes } => BridgeBarcodeSignal::Failed {
            failure: lookup_failure_to_bridge(failure),
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
        TextSignal::Failed {
            failure,
            catalogs,
            free_text,
        } => BridgeTextSignal::Failed {
            failure: lookup_failure_to_bridge(failure),
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn scanned_file_to_bridge(f: bae_core::import::folder_scanner::ScannedFile) -> BridgeFileInfo {
    BridgeFileInfo {
        name: f.relative_path,
        size: f.size,
        dir_prefix: f.dir_prefix,
        file_name: f.file_name,
        local_path: f.path.to_string_lossy().to_string(),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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

#[cfg(not(any(target_os = "ios", target_os = "android")))]
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
                    cue_size: p.cue_file.size,
                    cue_local_path: p.cue_file.path.to_string_lossy().to_string(),
                    flac_name: p.audio_file.relative_path,
                    flac_local_path: p.audio_file.path.to_string_lossy().to_string(),
                    total_size: p.total_size,
                    track_count: p.cue_sheet.tracks.len() as u32,
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

/// Airtight cross-check that the `core.*` localization catalog stays in sync
/// with the keys the `bridge_*_key` functions produce — in both directions:
///
/// - `every_produced_key_exists_in_catalog`: every key a key fn can emit (plus
///   every direct-reference key the UI uses) has a catalog entry. A renamed or
///   dropped catalog key fails the build instead of rendering a raw key.
/// - `no_orphan_core_keys`: every `core.*` catalog entry is produced by a key
///   fn or listed in `DIRECT_KEYS`. A catalog key no producer references is
///   dead and must be deleted (or, if a real UI direct-reference, added to
///   `DIRECT_KEYS`).
///
/// Each keyed enum is covered by an explicit array of every variant AND an
/// inline exhaustive `match` with no `_` arm, so adding a variant is a compile
/// error here that forces updating the coverage.
#[cfg(test)]
mod loc_key_coverage {
    use super::*;

    /// `core.*` keys the UI references directly with its own args — not emitted
    /// by any `bridge_*_key` fn. Kept in sync with the catalog by
    /// `no_orphan_core_keys`.
    const DIRECT_KEYS: &[&str] = &[
        // Storage queue summary (UI composes counts).
        "core.queue.uploading",
        "core.queue.downloading",
        "core.queue.exporting",
        "core.queue.failed",
        "core.queue.queued",
        "core.download.bytes_progress",
        "core.outbox.pending_deletes",
        "core.outbox.bytes_progress",
        "core.outbox.throughput",
        "core.outbox.eta",
        // Release-group card pressing count.
        "core.import.pressings",
        // Generic lookup-failure line for the keyless `Diagnostic` variant:
        // `bridge_lookup_failure_key` returns `None`, the UI shows this line.
        "core.lookup.failure.diagnostic",
    ];

    /// Every key the `bridge_*_key` fns can emit. For each keyed enum an
    /// explicit array of all variants feeds an inline exhaustive `match` that
    /// re-derives the key, asserted equal to the production fn's output — so a
    /// new variant fails to compile here.
    fn produced_keys() -> Vec<String> {
        let mut keys = Vec::new();

        // bridge_transfer_action_key — every variant carries a key.
        for a in [
            BridgeReleaseStorageAction::MakeRemote,
            BridgeReleaseStorageAction::Pin,
            BridgeReleaseStorageAction::Unpin,
            BridgeReleaseStorageAction::MakeLocal,
        ] {
            let expected = match a {
                BridgeReleaseStorageAction::MakeRemote => "core.transfer.action.manage",
                BridgeReleaseStorageAction::Pin => "core.transfer.action.pin",
                BridgeReleaseStorageAction::Unpin => "core.transfer.action.unpin",
                BridgeReleaseStorageAction::MakeLocal => "core.transfer.action.unmanage",
            };
            assert_eq!(bridge_transfer_action_key(a), expected);
            keys.push(expected.to_string());
        }

        // bridge_track_header_key — Flat carries no key (None).
        for s in [
            BridgeTrackSide::Sided {
                side_letter: "A".to_string(),
            },
            BridgeTrackSide::Disc { disc: 1 },
            BridgeTrackSide::Flat,
        ] {
            let expected: Option<&str> = match s {
                BridgeTrackSide::Sided { .. } => Some("core.track.side"),
                BridgeTrackSide::Disc { .. } => Some("core.track.disc"),
                BridgeTrackSide::Flat => None,
            };
            assert_eq!(bridge_track_header_key(s).as_deref(), expected);
            if let Some(k) = expected {
                keys.push(k.to_string());
            }
        }

        // bridge_audio_channels_key — only 1 and 2 carry words.
        for (channels, expected) in [
            (1_i64, Some("core.audio.channels.mono")),
            (2, Some("core.audio.channels.stereo")),
        ] {
            assert_eq!(bridge_audio_channels_key(channels).as_deref(), expected);
            if let Some(k) = expected {
                keys.push(k.to_string());
            }
        }

        // bridge_cloud_provider_label_key — None (local-only) and S3 carry
        // keys; the brand-name providers pass through (None).
        for p in [
            None,
            Some(BridgeCloudProvider::S3),
            Some(BridgeCloudProvider::GoogleDrive),
            Some(BridgeCloudProvider::Dropbox),
            Some(BridgeCloudProvider::OneDrive),
            Some(BridgeCloudProvider::CloudKit),
        ] {
            let expected: Option<&str> = match p {
                None => Some("core.cloud.local_only"),
                Some(BridgeCloudProvider::S3) => Some("core.cloud.s3_compatible"),
                Some(
                    BridgeCloudProvider::GoogleDrive
                    | BridgeCloudProvider::Dropbox
                    | BridgeCloudProvider::OneDrive
                    | BridgeCloudProvider::CloudKit,
                ) => None,
            };
            assert_eq!(bridge_cloud_provider_label_key(p).as_deref(), expected);
            if let Some(k) = expected {
                keys.push(k.to_string());
            }
        }

        // bridge_invalid_reason_key — every variant carries a key.
        for r in [
            BridgeInvalidReason::CorruptAudioFile {
                path: String::new(),
            },
            BridgeInvalidReason::CorruptImage {
                path: String::new(),
            },
            BridgeInvalidReason::CueMissingAudio,
            BridgeInvalidReason::NoValidAudio,
        ] {
            let expected = match r {
                BridgeInvalidReason::CorruptAudioFile { .. } => "core.import.invalid.corrupt_audio",
                BridgeInvalidReason::CorruptImage { .. } => "core.import.invalid.corrupt_image",
                BridgeInvalidReason::CueMissingAudio => "core.import.invalid.cue_missing_audio",
                BridgeInvalidReason::NoValidAudio => "core.import.invalid.no_valid_audio",
            };
            assert_eq!(bridge_invalid_reason_key(r.clone()), expected);
            keys.push(expected.to_string());
        }

        // bridge_prepare_step_key — every variant carries a key.
        for step in [
            BridgePrepareStep::ParsingMetadata,
            BridgePrepareStep::WritingCoverArt,
            BridgePrepareStep::DiscoveringFiles,
            BridgePrepareStep::ValidatingTracks,
            BridgePrepareStep::SavingToDatabase,
        ] {
            let expected = match step {
                BridgePrepareStep::ParsingMetadata => "core.import.prepare.parsing_metadata",
                BridgePrepareStep::WritingCoverArt => "core.import.prepare.writing_cover_art",
                BridgePrepareStep::DiscoveringFiles => "core.import.prepare.discovering_files",
                BridgePrepareStep::ValidatingTracks => "core.import.prepare.validating_tracks",
                BridgePrepareStep::SavingToDatabase => "core.import.prepare.saving_to_database",
            };
            assert_eq!(bridge_prepare_step_key(step), expected);
            keys.push(expected.to_string());
        }

        // bridge_import_phase_key — every variant carries a key.
        for phase in [
            BridgeImportPhase::ReferencingFiles,
            BridgeImportPhase::MeasuringLoudness,
            BridgeImportPhase::Finalizing,
        ] {
            let expected = match phase {
                BridgeImportPhase::ReferencingFiles => "core.import.phase.referencing_files",
                BridgeImportPhase::MeasuringLoudness => "core.import.phase.measuring_loudness",
                BridgeImportPhase::Finalizing => "core.import.phase.finalizing",
            };
            assert_eq!(bridge_import_phase_key(phase), expected);
            keys.push(expected.to_string());
        }

        // BridgeValidationReason::loc_key — every variant carries a key.
        for reason in [
            BridgeValidationReason::EmptyAlbumTitle,
            BridgeValidationReason::NoAlbumArtist,
            BridgeValidationReason::InvalidYear,
        ] {
            let expected = match reason {
                BridgeValidationReason::EmptyAlbumTitle => {
                    "core.import.validation.empty_album_title"
                }
                BridgeValidationReason::NoAlbumArtist => "core.import.validation.no_album_artist",
                BridgeValidationReason::InvalidYear => "core.import.validation.invalid_year",
            };
            assert_eq!(reason.loc_key(), expected);
            keys.push(expected.to_string());
        }

        // bridge_lookup_failure_key — all keyed variants must produce catalog
        // keys; Diagnostic carries no key.
        for f in [
            BridgeLookupFailure::Network,
            BridgeLookupFailure::Provider { status: Some(503) },
            BridgeLookupFailure::Provider { status: None },
            BridgeLookupFailure::Timeout,
            BridgeLookupFailure::ArtworkAnalysis,
        ] {
            keys.push(
                bridge_lookup_failure_key(f)
                    .expect("typed lookup failure is keyed")
                    .to_string(),
            );
        }
        assert!(bridge_lookup_failure_key(BridgeLookupFailure::Diagnostic {
            detail: String::new(),
        })
        .is_none());

        // bridge_error_category_key — every variant carries a key.
        for c in [
            BridgeErrorCategory::Database,
            BridgeErrorCategory::Config,
            BridgeErrorCategory::Internal,
            BridgeErrorCategory::Import,
            BridgeErrorCategory::Export,
        ] {
            let expected = match c {
                BridgeErrorCategory::Database => "core.error.category.database",
                BridgeErrorCategory::Config => "core.error.category.config",
                BridgeErrorCategory::Internal => "core.error.category.internal",
                BridgeErrorCategory::Import => "core.error.category.import",
                BridgeErrorCategory::Export => "core.error.category.export",
            };
            assert_eq!(bridge_error_category_key(c), expected);
            keys.push(expected.to_string());
        }

        // bridge_entity_not_found_key — every variant carries a key.
        for e in [
            BridgeEntityKind::Library,
            BridgeEntityKind::Album,
            BridgeEntityKind::Release,
            BridgeEntityKind::Track,
            BridgeEntityKind::File,
        ] {
            let expected = match e {
                BridgeEntityKind::Library => "core.error.not_found.library",
                BridgeEntityKind::Album => "core.error.not_found.album",
                BridgeEntityKind::Release => "core.error.not_found.release",
                BridgeEntityKind::Track => "core.error.not_found.track",
                BridgeEntityKind::File => "core.error.not_found.file",
            };
            assert_eq!(bridge_entity_not_found_key(e), expected);
            keys.push(expected.to_string());
        }

        // bridge_playback_error_reason_key — Diagnostic carries no key (None).
        for r in [
            BridgePlaybackErrorReason::SyncDisconnected,
            BridgePlaybackErrorReason::UploadPending,
            BridgePlaybackErrorReason::Diagnostic {
                error: BridgeError::internal(""),
            },
        ] {
            let expected: Option<&str> = match r {
                BridgePlaybackErrorReason::SyncDisconnected => {
                    Some("core.playback.error.sync_disconnected")
                }
                BridgePlaybackErrorReason::UploadPending => {
                    Some("core.playback.error.upload_pending")
                }
                BridgePlaybackErrorReason::Diagnostic { .. } => None,
            };
            assert_eq!(bridge_playback_error_reason_key(&r).as_deref(), expected);
            if let Some(k) = expected {
                keys.push(k.to_string());
            }
        }

        keys.extend(
            [
                bae_core::playback::SIDE_PAUSE_TITLE_KEY,
                bae_core::playback::SIDE_PAUSE_VINYL_MESSAGE_KEY,
                bae_core::playback::SIDE_PAUSE_CASSETTE_MESSAGE_KEY,
            ]
            .into_iter()
            .map(str::to_string),
        );

        keys
    }

    fn catalog() -> bae_loc::Catalog {
        bae_loc::Catalog::from_toml(include_str!("../loc/catalog.toml")).expect("catalog parses")
    }

    /// Missing-key direction: every produced key and every direct-reference key
    /// has a catalog entry.
    #[test]
    fn every_produced_key_exists_in_catalog() {
        let cat = catalog();
        for key in produced_keys()
            .iter()
            .map(String::as_str)
            .chain(DIRECT_KEYS.iter().copied())
        {
            assert!(
                cat.messages.contains_key(key),
                "catalog missing `{key}` — a key fn or DIRECT_KEYS produces it but the entry is gone"
            );
        }
    }

    /// Orphan direction: every `core.*` catalog entry is produced by a key fn
    /// or listed in `DIRECT_KEYS`.
    #[test]
    fn no_orphan_core_keys() {
        let cat = catalog();
        let mut accounted: std::collections::HashSet<String> =
            produced_keys().into_iter().collect();
        accounted.extend(DIRECT_KEYS.iter().map(|k| k.to_string()));

        for key in cat.messages.keys() {
            if !key.starts_with("core.") {
                continue;
            }
            assert!(
                accounted.contains(key),
                "catalog key `{key}` has no producer — delete it or add a producer \
                 (a bridge_*_key fn) or list it in DIRECT_KEYS"
            );
        }
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod identify_progress_tests {
    use super::*;

    #[test]
    fn barcode_progress_failure_crosses_bridge() {
        let progress = bae_core::identify::BarcodeProgress::Failed {
            failure: bae_core::signals::LookupFailure::Diagnostic {
                detail: "provider lookup failed".to_string(),
            },
        };

        match barcode_progress_to_bridge(progress) {
            BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::Diagnostic { detail },
            } => assert_eq!(detail, "provider lookup failed"),
            other => panic!("expected failed barcode progress, got {other:?}"),
        }
    }
}
