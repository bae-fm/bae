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
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiscogsSaveOutcome {
    Valid,
    Unvalidated,
    Rejected,
}

#[cfg(feature = "desktop")]
impl BridgeDiscogsSaveOutcome {
    pub(crate) fn from_core(outcome: bae_core::import::DiscogsSaveOutcome) -> Self {
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
    pub fn into_core(self) -> bae_core::import::MetadataSource {
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
    /// Why this library cannot be opened, or `None` when it is fine.
    ///
    /// A library whose config.yaml will not parse is still listed — it used to
    /// vanish from the picker instead, which lost it. The UI shows the row as
    /// unavailable with this as the reason, and refuses to open it. Its `name` is
    /// the directory id, because the name is exactly what could not be read.
    pub error: Option<String>,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLibraryImageType {
    Cover,
    Artist,
}

impl BridgeLibraryImageType {
    pub(crate) fn from_core(value: bae_core::db::LibraryImageType) -> Self {
        match value {
            bae_core::db::LibraryImageType::Cover => Self::Cover,
            bae_core::db::LibraryImageType::Artist => Self::Artist,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::db::LibraryImageType {
        match self {
            BridgeLibraryImageType::Cover => bae_core::db::LibraryImageType::Cover,
            BridgeLibraryImageType::Artist => bae_core::db::LibraryImageType::Artist,
        }
    }
}

/// A reference to a curated library image (a release cover or an artist
/// portrait): the image kind, subject id, and content version. The UI passes the
/// whole ref to `fetch_library_image_bytes`, so core dispatches to the known
/// image namespace.
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
        let bae_core::album_detail::ImageRef {
            id,
            version,
            image_type,
        } = r;
        Self {
            id,
            version,
            image_type: BridgeLibraryImageType::from_core(image_type),
        }
    }

    pub fn into_core(self) -> bae_core::album_detail::ImageRef {
        let BridgeImageRef {
            id,
            version,
            image_type,
        } = self;
        bae_core::album_detail::ImageRef {
            id,
            version,
            image_type: image_type.into_core(),
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

    fn into_core(self) -> bae_core::album_detail::ReleaseStorageAction {
        use bae_core::album_detail::ReleaseStorageAction;
        match self {
            Self::MakeRemote => ReleaseStorageAction::MakeRemote,
            Self::Pin => ReleaseStorageAction::Pin,
            Self::Unpin => ReleaseStorageAction::Unpin,
            Self::MakeLocal => ReleaseStorageAction::MakeLocal,
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

/// Slim per-release row for list views (storage manager, release pickers). The
/// fat sibling is `BridgeRelease`; the bridge mirrors each half as its own type
/// so UI consumers can populate separate summary and detail stores.
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
    pub transfer_action: Option<BridgeReleaseStorageAction>,
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
    pub transfer_action: Option<BridgeReleaseStorageAction>,
    pub tracks: Vec<BridgeTrack>,
    pub track_groups: Vec<BridgeTrackGroup>,
    pub files: Vec<BridgeFile>,
    pub image_files: Vec<BridgeFile>,
    /// Cover slot first (if the release has one), then every image file the
    /// release has. Each item's bytes are read through
    /// `fetch_release_image_bytes`, which takes the item's `source` and
    /// dispatches the read. Consumers render this as-is.
    pub gallery_items: Vec<BridgeGalleryItem>,
    /// Total playing time across all tracks, as the words it reads in, or `None`
    /// when no track reports a length. The raw sum does not cross: with the
    /// milliseconds in hand a UI could name the total its own way, which is how
    /// the three platforms came to disagree about it.
    pub total_duration: Option<BridgeDurationUnits>,
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
    pub fn into_core(self) -> bae_core::import::IdentityChoice {
        match self {
            Self::Exact { release_id, source } => bae_core::import::IdentityChoice::Exact {
                release_ref: bae_core::import::MetadataRef::new(release_id, source.into_core()),
            },
            Self::Approximate { release_id, source } => {
                bae_core::import::IdentityChoice::Approximate {
                    release_ref: bae_core::import::MetadataRef::new(release_id, source.into_core()),
                }
            }
            Self::Unknown => bae_core::import::IdentityChoice::Unknown,
        }
    }

    /// The claim core recorded for a candidate — the direction `into_core`
    /// doesn't cover, since a pick's claim is settled in core and travels
    /// outward.
    pub fn from_core(choice: bae_core::import::IdentityChoice) -> Self {
        match choice {
            bae_core::import::IdentityChoice::Exact { release_ref } => Self::Exact {
                release_id: release_ref.id,
                source: BridgeMetadataSource::from_core(release_ref.source),
            },
            bae_core::import::IdentityChoice::Approximate { release_ref } => Self::Approximate {
                release_id: release_ref.id,
                source: BridgeMetadataSource::from_core(release_ref.source),
            },
            bae_core::import::IdentityChoice::Unknown => Self::Unknown,
        }
    }
}

/// A track group's side discriminant — the header the UI renders ("Side A" /
/// "Disc 2"). `Flat` means no header (single-disc digital).
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

    /// Localization key for this side's header word ("Side" / "Disc"), or `None`
    /// for `Flat` (single-disc digital has no header). Pre-computed onto
    /// [`BridgeTrackGroup::header_key`] at conversion; the UI resolves the key
    /// and substitutes the side letter / disc number `side` carries.
    pub(crate) fn header_key(&self) -> Option<&'static str> {
        match self {
            Self::Sided { .. } => Some("core.track.side"),
            Self::Disc { .. } => Some("core.track.disc"),
            Self::Flat => None,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrackGroup {
    /// The group's side discriminant; the UI substitutes the letter / disc
    /// number into the header format (`Flat` means no header).
    pub side: BridgeTrackSide,
    /// Localization key for the group header word ("core.track.side" /
    /// "core.track.disc"), or `None` for `Flat`. Core-rendered at conversion so
    /// a track list reads a field per group instead of an FFI call per group;
    /// the UI resolves the key against the `Core` table and interpolates the
    /// letter / number `side` carries.
    pub header_key: Option<String>,
    pub tracks: Vec<BridgeTrack>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeTrack {
    pub id: String,
    pub title: String,
    pub side: i32,
    pub track_number: Option<i32>,
    /// Raw track length in milliseconds; `None` when core reports none. Retained
    /// alongside `duration_clock` because a consumer needs the number itself —
    /// Android sets it on the media-session `MediaMetadata` for a queued track.
    pub duration_ms: Option<i64>,
    /// The track length as a clock label's fields ("3:07"), or `None` when there
    /// is nothing to label. Core-rendered at conversion so a track row reads a
    /// field instead of an FFI call; the UI formats the fields for its locale.
    pub duration_clock: Option<BridgeDurationClock>,
    /// Effective comma-joined artist names for display (the track's own
    /// artists when it has per-track artist rows, otherwise the album
    /// artists). Always populated.
    pub artist_names: String,
    /// The artist to show on the track row, or `None` for none. Core sets it
    /// only for a compilation, where the album header names no single artist;
    /// on a single-artist album the row would only repeat the header. The UI
    /// renders it when present rather than deciding for itself.
    pub display_artist: Option<String>,
    /// Core-rendered position string: "A1"/"2-3"/"5", or the stable prefix
    /// when the source has no track number.
    pub position_text: String,
}

/// Mirror of bae-core's `DurationClock` — the fields a clock label shows, for
/// every place a duration reads as "3:07" / "1:12:34" / "-0:42": a track's
/// length, the elapsed position, the remaining countdown.
///
/// The UI renders these numbers and nothing else: `:` between the fields, every
/// field after the first zero-padded to two digits, a leading `-` when
/// `negative`. It never decides which fields to show — `hours` being `Some` is
/// core's answer, as is `bridge_clock` returning `None` (nothing to label).
/// Digits are the UI's because the locale never crosses the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDurationClock {
    pub negative: bool,
    /// Set from one hour up, and only then.
    pub hours: Option<u64>,
    pub minutes: u32,
    pub seconds: u32,
}

impl BridgeDurationClock {
    fn from_core(clock: bae_core::util::duration::DurationClock) -> Self {
        let bae_core::util::duration::DurationClock {
            negative,
            hours,
            minutes,
            seconds,
        } = clock;
        Self {
            negative,
            hours,
            minutes,
            seconds,
        }
    }

    /// The clock for a duration in milliseconds, or `None` when there is nothing
    /// to label (an absent duration, or a negative one — a gap in the data, not
    /// a short track). Pre-computed onto the static row types at conversion.
    pub(crate) fn from_millis(ms: Option<i64>) -> Option<Self> {
        bae_core::util::duration::DurationClock::from_millis(ms).map(Self::from_core)
    }
}

/// Mirror of bae-core's `DurationUnits` — a duration named in words ("39 min",
/// "3 hr", "3 hr, 42 min"), which is what a release's total playing time reads
/// as. Distinct from [`BridgeDurationClock`], which counts time rather than
/// naming an amount of it, and which always has minutes and seconds.
///
/// The variants are the label's shape: a component that would be zero does not
/// exist, so no UI has to filter one out. Each maps to the `core.duration.*`
/// catalog messages — the words and the join between them belong to the catalog.
///
/// No variant carries a field whose name is the variant's own: that generates a
/// C# member named the same as its enclosing type, which does not compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDurationUnits {
    HoursOnly { hours: u64 },
    MinutesOnly { minutes: u64 },
    HoursAndMinutes { hours: u64, minutes: u64 },
}

impl BridgeDurationUnits {
    pub(crate) fn from_core(units: bae_core::util::duration::DurationUnits) -> Self {
        use bae_core::util::duration::DurationUnits;
        match units {
            DurationUnits::HoursOnly { hours } => Self::HoursOnly { hours },
            DurationUnits::MinutesOnly { minutes } => Self::MinutesOnly { minutes },
            DurationUnits::HoursAndMinutes { hours, minutes } => {
                Self::HoursAndMinutes { hours, minutes }
            }
        }
    }
}

/// The clock for a duration, or `None` when there is nothing to label — an
/// absent duration, or a negative one (a gap in the data, not a short track).
#[uniffi::export]
pub fn bridge_clock(ms: Option<i64>) -> Option<BridgeDurationClock> {
    BridgeDurationClock::from_millis(ms)
}

/// The two clocks a seek bar shows. Mirror of bae-core's `SeekBarClocks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSeekBarClocks {
    /// Elapsed, or the countdown, per `show_remaining_time`.
    pub leading: BridgeDurationClock,
    /// The track's total length; `None` when it is not known.
    pub trailing: Option<BridgeDurationClock>,
}

/// The seek bar's two labels, for a position within a track.
///
/// Named apart from the record it returns: uniffi-bindgen-cs would otherwise emit
/// a C# method whose name is the type's, and Windows has no compiler here.
///
/// `show_remaining` is the user's `show_remaining_time` config, which the UI
/// reads off the config mirror and never stores itself. A `duration_ms` of zero
/// is playback reporting an unknown length: no total, and no countdown.
#[uniffi::export]
pub fn bridge_seek_bar(
    position_ms: u64,
    duration_ms: u64,
    show_remaining: bool,
) -> BridgeSeekBarClocks {
    let bae_core::util::duration::SeekBarClocks { leading, trailing } =
        bae_core::util::duration::SeekBarClocks::new(position_ms, duration_ms, show_remaining);
    BridgeSeekBarClocks {
        leading: BridgeDurationClock::from_core(leading),
        trailing: trailing.map(BridgeDurationClock::from_core),
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

impl BridgeAudioFormat {
    pub(crate) fn from_core(f: bae_core::album_detail::AudioFormat) -> Self {
        let bae_core::album_detail::AudioFormat {
            codec,
            sample_rate_hz,
            bits_per_sample,
            bitrate_kbps,
            channels,
        } = f;
        Self {
            codec,
            sample_rate_hz,
            bits_per_sample,
            bitrate_kbps,
            channels,
        }
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
/// carries its file id. The UI passes the whole value to
/// `fetch_release_image_bytes` and never inspects it to pick a fetch.
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
    pub fn into_core(self) -> bae_core::playback::RepeatMode {
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

/// The mode a repeat button steps to next. Playback only accepts an absolute
/// mode, so the button computes its target from the one it renders — but which
/// mode follows which is core's answer, not each app's.
#[uniffi::export]
pub fn bridge_next_repeat_mode(mode: BridgeRepeatMode) -> BridgeRepeatMode {
    BridgeRepeatMode::from_core(mode.into_core().next())
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
    /// The track's own release's cover, or `None` when it has none. Versioned,
    /// so the UI's art cache key moves when the cover bytes change.
    pub cover_image: Option<BridgeImageRef>,
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
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
    Paused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
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
        let bae_core::playback::PlaybackSidePausePrompt {
            id,
            title_key,
            side_letter,
            message_key,
        } = prompt;
        Self {
            id,
            title_key: title_key.to_string(),
            side_letter,
            message_key: message_key.to_string(),
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

/// Bytes of provider art fetched from a URL, plus the token that identifies this
/// exact content: the response's `ETag`, or a hash of the bytes when it carries
/// none. A UI stores the validator alongside its decoded copy and replaces that
/// copy when a later fetch answers with a different one.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRemoteImage {
    pub bytes: Vec<u8>,
    pub validator: String,
}

impl BridgeRemoteImage {
    #[cfg(feature = "desktop")]
    pub(crate) fn from_core(image: bae_core::import::cover_art::RemoteImage) -> Self {
        let bae_core::import::cover_art::RemoteImage {
            bytes,
            validator,
            // Describes the download, not what the UI renders: every platform
            // decoder sniffs the bytes themselves.
            content_type: _,
        } = image;
        Self { bytes, validator }
    }
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
    fn from_core(o: bae_core::signals::SignalOrigin) -> Self {
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
    fn from_core(s: bae_core::signals::SourcedValue) -> Self {
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
    fn from_core(f: bae_core::signals::LookupFailure) -> Self {
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
    fn from_core(s: bae_core::identify::SignalState) -> Self {
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
    fn from_core(s: bae_core::identify::ToolbarSignal) -> Self {
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

/// Callback for the unified UI event stream.
#[uniffi::export(callback_interface)]
pub trait UiEventCallback: Send + Sync {
    fn on_event(&self, event: BridgeUiEvent);
}

/// Everything one Vision pass over an image surfaces — barcode payloads and
/// recognized text lines from a single image decode. Mirrors
/// `bae_core::signals::ArtworkAnalysis`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtworkAnalysis {
    pub barcodes: Vec<String>,
    pub text_lines: Vec<String>,
}

/// Platform-provided artwork analyzer. One `analyze` pass over an image yields
/// both barcodes and text, so the signal-extraction pass decodes each image
/// exactly once.
///
/// Sync by design: `VNImageRequestHandler.perform` is synchronous, and the Rust
/// side calls this from `tokio::task::spawn_blocking` so the async runtime isn't
/// parked while Vision churns.
///
/// Unlike `UiEventCallback` (fire-and-forget), this one returns a value.
#[uniffi::export(callback_interface)]
pub trait ArtworkAnalyzerCallback: Send + Sync {
    /// Detect barcodes and recognize text in one image decode. Empty
    /// payloads/lines on failure or when absent.
    fn analyze(&self, path: String) -> BridgeArtworkAnalysis;
}

/// A scoped state key the UI should requery from core. Mirrors
/// `bae_core::ui::Invalidation`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeInvalidation {
    AlbumList,
    Album { album_id: String },
    Release { release_id: String },
    ComposerList,
    Composer { composer_id: String },
    ArtistList,
    Queue,
    Config,
    SyncStatus,
    Outbox,
    DownloadQueue,
    OutputQueue,
    ImportCandidateList,
    ImportCandidate { key: String },
    WatchedFolders,
    CastDevices,
}

impl BridgeInvalidation {
    pub(crate) fn from_core(invalidation: bae_core::ui::Invalidation) -> Self {
        use bae_core::ui::Invalidation as CoreInvalidation;

        match invalidation {
            CoreInvalidation::AlbumList => Self::AlbumList,
            CoreInvalidation::Album { album_id } => Self::Album { album_id },
            CoreInvalidation::Release { release_id } => Self::Release { release_id },
            CoreInvalidation::ComposerList => Self::ComposerList,
            CoreInvalidation::Composer { composer_id } => Self::Composer { composer_id },
            CoreInvalidation::ArtistList => Self::ArtistList,
            CoreInvalidation::Queue => Self::Queue,
            CoreInvalidation::Config => Self::Config,
            CoreInvalidation::SyncStatus => Self::SyncStatus,
            CoreInvalidation::Outbox => Self::Outbox,
            CoreInvalidation::DownloadQueue => Self::DownloadQueue,
            CoreInvalidation::OutputQueue => Self::OutputQueue,
            CoreInvalidation::ImportCandidateList => Self::ImportCandidateList,
            CoreInvalidation::ImportCandidate { key } => Self::ImportCandidate { key },
            CoreInvalidation::WatchedFolders => Self::WatchedFolders,
            CoreInvalidation::CastDevices => Self::CastDevices,
        }
    }
}

/// Top-level UI event. Every distinct state is a top-level variant with
/// fields inlined, except query-backed state changes which carry a scoped
/// invalidation key.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeUiEvent {
    Invalidated {
        invalidation: BridgeInvalidation,
    },

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
        cover_image: Option<BridgeImageRef>,
        duration_ms: u64,
    },
    PlaybackPaused {
        track_id: String,
        track_title: String,
        artist_names: String,
        artist_id: String,
        album_id: String,
        album_title: String,
        cover_image: Option<BridgeImageRef>,
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
        snapshot: BridgeQueueSnapshot,
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

    // ── Import live progress ───────────────────────────────────────
    /// High-frequency loudness-measurement tick — the UI routes it to a native
    /// leaf view (a determinate bar driven by `fraction`, labelled "N / M"), not
    /// the coarse candidate row.
    CandidateImportLoudnessProgress {
        key: String,
        tracks_done: u32,
        tracks_total: u32,
        fraction: f32,
    },
    /// How much of the import queue the background sweep has answered — the
    /// sidebar header's line and bar. Both numbers are the queue's; a view
    /// must not derive `total` from the rows it holds, which are filtered.
    ImportQueueIdentifyProgress {
        identified: u32,
        total: u32,
    },

    // ── Release transfer ───────────────────────────────────────────
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

    // ── Cast ───────────────────────────────────────────────────────
    /// The active renderer changed: `Some(name)` when playback moved to a Cast
    /// device, `None` when it returned to local output. Drives the cast button's
    /// active state and the "Casting to `<name>`" row.
    CastStatusChanged {
        device_name: Option<String>,
    },

    // ── Errors ─────────────────────────────────────────────────────
    Error {
        error: BridgeError,
    },
}

/// The dominant activity of a slice of the upload queue (a release's uploads,
/// or the whole queue), for the storage-row badge. Mirror of bae-core's
/// `UploadActivity`. No terminal variant: a release with nothing left to ship
/// stops being rendered at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadActivity {
    Uploading,
    Retrying,
    Queued,
}

/// One file's state in the queue pane's per-file rows. Unlike the slice badge,
/// a file inside a still-uploading release does render as `Done` — the row
/// shows which files already shipped while the rest transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeUploadFileState {
    Queued,
    Uploading,
    Retrying,
    Done,
}

/// One cloud object still owed a removal.
///
/// The row that named the object is gone — that is what makes the removal
/// outstanding — so there is no filename or album to show, and no cancel: the
/// object exists in the cloud and abandoning the tombstone would strand it.
/// `namespace` and `blob_id` together identify it and serve as the row's
/// identity for list diffing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeDeleteOp {
    pub namespace: String,
    pub blob_id: String,
    /// Enqueue time as Unix epoch milliseconds, for the queued relative label.
    pub created_at: i64,
}

/// Per-state counts, byte progress, and a derived badge `activity`. Used
/// per-release (the storage-row badge reads `activity`; storage-action gates
/// read the counts) and as the overall total (queue counts, ETA, summary band).
/// Files completed during the current queue burst count in `bytes_done` and
/// `bytes_total`, so the fractions are cumulative over the burst.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeUploadProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The badge activity for this slice; `None` when idle. Per-release entries
    /// always have pending work (finished releases aren't rendered), so theirs
    /// is always set.
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

impl BridgeDownloadTransferProgress {
    fn into_core(self) -> bae_core::library::DownloadTransferProgress {
        let Self {
            bytes_done,
            bytes_total,
            fraction,
        } = self;
        bae_core::library::DownloadTransferProgress {
            bytes_done,
            bytes_total,
            fraction,
        }
    }
}

impl BridgeDownloadState {
    fn into_core(self) -> bae_core::library::DownloadState {
        use bae_core::library::DownloadState;
        match self {
            Self::Queued => DownloadState::Queued,
            Self::Active { progress } => DownloadState::Active {
                progress: progress.into_core(),
            },
            Self::Failed { error } => DownloadState::Failed { error },
        }
    }
}

impl BridgeDownloadOp {
    fn into_core(self) -> bae_core::library::DownloadOp {
        let Self {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            state,
        } = self;
        bae_core::library::release_queue::ReleaseQueueOp {
            release_id,
            title,
            file_count,
            total_size,
            created_at,
            // Downloads carry no operation-specific payload.
            payload: (),
            state: state.into_core(),
        }
    }
}

/// What the album-detail download control shows for one release. Mirror of
/// bae-core's `ReleaseDownloadStatus`.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum BridgeReleaseDownloadStatus {
    Downloaded,
    Queued,
    Downloading {
        progress: BridgeDownloadTransferProgress,
    },
    Failed {
        error: String,
    },
    Available,
}

impl BridgeReleaseDownloadStatus {
    fn from_core(status: bae_core::album_detail::ReleaseDownloadStatus) -> Self {
        use bae_core::album_detail::ReleaseDownloadStatus;
        match status {
            ReleaseDownloadStatus::Downloaded => Self::Downloaded,
            ReleaseDownloadStatus::Queued => Self::Queued,
            ReleaseDownloadStatus::Downloading { progress } => Self::Downloading {
                progress: BridgeDownloadTransferProgress::from_core(progress),
            },
            ReleaseDownloadStatus::Failed { error } => Self::Failed { error },
            ReleaseDownloadStatus::Available => Self::Available,
        }
    }
}

/// The download control's state for one release, or `None` when there is no
/// control to show (no cloud home, or a release whose audio is already local).
///
/// The whole join is core's — including finding this release's entry in the
/// queue. A live entry outranks `pinned`, and `Available` means exactly "core
/// offers Pin"; both are properties of core's own storage-action gate, so an app
/// that re-derived either would drift from it.
#[uniffi::export]
pub fn bridge_release_download_status(
    pinned: bool,
    storage_actions: Vec<BridgeReleaseStorageAction>,
    downloads: BridgeDownloadSnapshot,
    release_id: String,
) -> Option<BridgeReleaseDownloadStatus> {
    let actions: Vec<_> = storage_actions
        .into_iter()
        .map(BridgeReleaseStorageAction::into_core)
        .collect();
    let ops: Vec<_> = downloads
        .downloads
        .into_iter()
        .map(BridgeDownloadOp::into_core)
        .collect();
    bae_core::album_detail::release_download_status(pinned, &actions, &ops, &release_id)
        .map(BridgeReleaseDownloadStatus::from_core)
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
    /// The one-line queue summary's parts (downloading/failed/queued, each
    /// dropped when zero), decided by core. The UI resolves each key with its
    /// count and joins — it does not choose which counts appear or their order.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// True when the user paused the download queue. Drives the pause/resume
    /// toggle in the Downloads pane.
    pub paused: bool,
}

/// One part of a queue summary line — a catalog key and its count. Mirror of
/// bae-core's `CountLabel`. Which parts appear, in what order, and that a zero
/// drops out is core's decision; the UI resolves the key and joins the parts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCountLabel {
    pub key: String,
    pub count: u32,
}

impl BridgeCountLabel {
    pub(crate) fn from_core(label: bae_core::library::CountLabel) -> Self {
        let bae_core::library::CountLabel { key, count } = label;
        Self { key, count }
    }
}

/// A queued export's state. Mirror of bae-core's `OutputState`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputState {
    Queued,
    Active { percent: u8 },
    Failed { error: String },
}

/// What a queued release-level output produces, for display in the queue row.
/// Mirror of bae-core's `OutputKind`; a save carries its preset's display name
/// (resolved at enqueue, not an id — the row never dereferences a preset).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeOutputKind {
    Export,
    Save { preset_name: String },
}

/// One queued release output — a whole release being written out to a folder,
/// either a verbatim export or a preset save. Mirror of bae-core's `OutputOp`;
/// carries raw fields the UI renders directly.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputOp {
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
    pub state: BridgeOutputState,
    /// Whether this row is a verbatim export or a preset save; drives the row's
    /// state text and (for saves) the preset name in the detail line.
    pub kind: BridgeOutputKind,
}

/// Per-state counts for the export queue, driving the pane header. No bytes:
/// outputs track an overall percent per release, not aggregate bytes.
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct BridgeOutputProgress {
    pub queued: u32,
    pub active: u32,
    pub failed: u32,
}

/// The in-memory export queue snapshot the Storage Manager's Exporting pane
/// renders. Mirror of bae-core's `OutputSnapshot`; the UI renders it verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutputSnapshot {
    pub outputs: Vec<BridgeOutputOp>,
    pub total: BridgeOutputProgress,
    /// The one-line queue summary's parts (exporting/failed/queued), decided by
    /// core. The UI resolves each key and joins.
    pub summary_parts: Vec<BridgeCountLabel>,
    /// True when the user paused the export queue. Drives the pause/resume
    /// toggle in the Exporting pane.
    pub paused: bool,
}

/// One file in a release's upload group: what the queue pane's per-file rows
/// render. Mirror of bae-core's `UploadFileOp`, with the state flattened into
/// `state` + `bytes_done` + `last_error` so the UI doesn't switch on
/// associated data: `bytes_done` is the live count while `Uploading`, equal to
/// `bytes_total` when `Done`, and 0 otherwise; `last_error` is set only when
/// `Retrying`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadFileOp {
    pub file_id: String,
    /// The file's original name, resolved by core (its cloud key / file id
    /// when no release-file row backs it).
    pub display_name: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub state: BridgeUploadFileState,
    pub last_error: Option<String>,
}

/// A release's uploads, grouped for the queue pane's expandable per-release
/// rows. Mirror of bae-core's `UploadReleaseGroup`. `release_id` is `None` for
/// the orphaned-files bucket; `display_title` is the row's label, resolved by
/// core. `files` runs completed files first (in completion order), then the
/// remaining queue in order.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUploadReleaseGroup {
    pub release_id: Option<String>,
    pub display_title: String,
    pub files: Vec<BridgeUploadFileOp>,
    pub progress: BridgeUploadProgress,
}

/// The cloud-outbox processing snapshot the Storage Manager renders. The
/// counts, per-release aggregates, one-line `summary`, throughput, and ETA
/// are computed from bae-core's grouped snapshot; the UI renders them verbatim.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeOutboxSnapshot {
    /// Uploads grouped by release for the queue pane's rows. Groups whose
    /// files all completed stay listed (as done rows) until the whole queue
    /// drains.
    pub upload_groups: Vec<BridgeUploadReleaseGroup>,
    pub deletes: Vec<BridgeDeleteOp>,
    /// Per-release aggregate derived from `upload_groups`, keyed by release id.
    /// Releases with no work this burst are absent from the map.
    pub per_release: std::collections::HashMap<String, BridgeUploadProgress>,
    /// Sum across all uploads. The master progress bar shows
    /// `total.bytes_done` of `total.bytes_total` — cumulative over the burst,
    /// completed files included.
    pub total: BridgeUploadProgress,
    /// Derived from `deletes.len()`.
    pub pending_deletes: u32,
    /// The one-line queue summary's parts (uploading/failed/queued/pending
    /// deletes, each dropped when zero), decided by core. The UI resolves each
    /// key and joins.
    pub summary_parts: Vec<BridgeCountLabel>,
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

/// What picking a release in the import search gives the confirmation pane.
///
/// `detail` is display: covers, source positions, the track count to reconcile
/// against the folder. `seed` is the metadata editor's starting value, projected
/// from the release exactly as the commit worker maps it — so the UI must seed
/// the editor from `seed`, never from `detail`, or an untouched artist list reads
/// as an edit at commit and the release loses its secondary album artists.
///
/// `seed` arrives already masked for `claim.choice` (an album-level claim blanks
/// the pressing block), so the UI binds it straight to the editor. The claim
/// itself came in on the pick, and lowering it is another pick — so there is no
/// re-shaping for the UI to do either way.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReleasePrefetch {
    pub detail: BridgeReleaseDetail,
    pub seed: BridgeReleaseUserEdit,
    pub claim: BridgeClaimLine,
    /// The file↔release pairing this pick produces: every source unit the
    /// folder offers with the track committing makes of it, the editable row
    /// inside the row that produces it. Empty for a key that names no scanned
    /// folder.
    pub mapping: BridgeMappingTable,
}

/// What identified the picked release. Mirrors
/// `bae_core::import::ClaimEvidence`. It explains the pick and decides nothing:
/// the UI renders it as the claim sentence's trailing clause, and the claim
/// itself is the user's, carried on the pick.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeClaimEvidence {
    /// The disc's table of contents matched this release and no other.
    DiscIdAlone,
    /// The disc's table of contents matched, and `match_count` releases came
    /// back for it.
    DiscIdShared { match_count: u32 },
    /// A barcode read off the packaging matched.
    Barcode,
    /// A catalog number, or a search the user typed, found it.
    Search,
}

/// The release header's claim line. Mirrors `bae_core::import::ClaimLine`.
///
/// Two facts: `choice` is what the import claims you physically hold, and
/// `release` is the release the metadata was read from. They coincide only for
/// a pressing claim; `level` says which of the two sentences the line reads as,
/// which side of the header's claim control is in force, and — since only an
/// album claim leaves the metadata's release unsaid — whether the second line
/// naming it is drawn.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeClaimLine {
    /// The claim this import will record, and what commit writes.
    pub choice: BridgeIdentityChoice,
    /// How far the claim reaches, as the user set it.
    pub level: BridgeClaimLevel,
    pub evidence: BridgeClaimEvidence,
    /// The picked release by its pressing facts — format, year, country and
    /// catalog number, `·`-joined. `None` when the source states none of them,
    /// and the sentence then reads without a description.
    pub release: Option<String>,
    /// The picked release's track count, where the source stated one. Rendered
    /// on the metadata-from line.
    pub track_count: Option<u32>,
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
    /// Which of the folder's audio holds this track's samples. An import's rows
    /// are its track slots, so this is the pairing the user left and the one the
    /// commit writes; a row with no audio is a slot nobody answered and does not
    /// become a track. The library's metadata editor never re-binds files, so
    /// its rows carry `None`.
    pub file: Option<BridgeAudioFile>,
}

/// The audio a track's samples come from. Mirrors
/// `bae_core::import::AudioFile`. `file_id` is the file's identity within the
/// release (its relative path), the same id the file-roles table and the sheet
/// bindings use.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeAudioFile {
    /// The whole file holds this one track.
    Standalone { file_id: String },
    /// One of several tracks the track sheet `sheet_id` carves out of
    /// `file_id`. `index` counts that sheet's playable tracks from zero.
    SheetSlice {
        file_id: String,
        sheet_id: String,
        index: u32,
    },
}

/// The tally above the slot table: how many files the folder offers against how
/// many tracks the source names, and which way they disagree. Mirror of
/// bae-core's `SlotReconciliation`.
///
/// Arrives computed rather than left to each UI to subtract, and it is stated
/// rather than enforced — a disagreement is something to read, never something
/// that disables the commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSlotReconciliation {
    Agrees { count: u32 },
    MoreFiles { files: u32, tracks: u32 },
    MoreTracks { files: u32, tracks: u32 },
}

/// Whether a slot row's two lengths — the file's own, probed off disk, and the
/// one the source states — are far enough apart that the row should say so.
///
/// Core's judgement, not each surface's: how much two rips of one track may
/// legitimately differ is one question, and two UIs each picking a number is
/// two answers to it. `false` when either side has no number, because there is
/// nothing to compare, which is not the same as agreeing.
///
/// Asked per row as it renders rather than carried on the slot, so a row the
/// user re-points at a different file is answered about the pairing it has now.
/// It marks a row; it disables nothing.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_lengths_disagree(probed_ms: Option<u64>, source_ms: Option<u64>) -> bool {
    bae_core::import::lengths_disagree(probed_ms, source_ms)
}

/// The catalog key naming the reconciliation line.
#[cfg_attr(feature = "desktop", uniffi::export)]
pub fn bridge_slot_reconciliation_key(reconciliation: BridgeSlotReconciliation) -> String {
    match reconciliation {
        BridgeSlotReconciliation::Agrees { .. } => "core.import.reconciliation.agrees",
        BridgeSlotReconciliation::MoreFiles { .. } => "core.import.reconciliation.more_files",
        BridgeSlotReconciliation::MoreTracks { .. } => "core.import.reconciliation.more_tracks",
    }
    .to_string()
}

/// Which disc of the release one track sheet's entries become. Mirror of
/// bae-core's `SheetDisc`.
///
/// Cue filenames are arbitrary — `CD1.cue` may hold disc two — so this is a
/// decision, set through `AppHandle::set_sheet_disc`, and never something a UI
/// reads off a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSheetDisc {
    /// The sheet's entries are the release's disc `number`, counting from one.
    Disc { number: u32 },
    /// The sheet contributes nothing to the tracklist. Its container is loose
    /// audio again.
    Ignored,
}

/// What one of the folder's files is, as a row of the mapping table. Mirror of
/// bae-core's `MappingRole`.
///
/// Narrower than the role the scan proposes: a track sheet is not a row here —
/// it heads a group of rows — and an image is not one either, because the
/// images are one gallery row rather than a row each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeMappingRole {
    Audio,
    Document,
    Other,
}

/// A file of the folder, as the mapping table's left half shows it. Mirror of
/// bae-core's `MappingFile`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingFile {
    /// The file's identity within the release (its relative path) — the id
    /// `AppHandle::set_file_role` and the sheet bindings take.
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    pub size: u64,
    /// Absolute path — what auditioning this row plays.
    pub local_path: String,
    /// Probed playing time in milliseconds, where the folder's audio has been
    /// read. `None` for anything that is not audio, for audio nothing could be
    /// read from, and while no release is picked.
    pub probed_duration_ms: Option<u64>,
    pub role: BridgeMappingRole,
    /// The roles this file can be put in, the one in force first. Empty when
    /// its role is nobody's decision to make.
    pub alternatives: Vec<BridgeFileRoleChoice>,
    /// The role in force as a choice — what a picker shows selected. `None`
    /// exactly when `alternatives` is empty.
    pub role_choice: Option<BridgeFileRoleChoice>,
}

/// One entry of a track sheet, as the mapping table's left half shows it.
/// Mirror of bae-core's `MappingEntry`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingEntry {
    pub sheet_id: String,
    /// Counts this sheet's playable entries from zero — the index the audio
    /// binding carries.
    pub index: u32,
    /// The number the sheet prints for this entry.
    pub number: u32,
    pub title: Option<String>,
    /// How long the sheet says this entry runs, in milliseconds.
    pub duration_ms: Option<u64>,
    /// The container this entry's samples come from — what auditioning plays.
    pub container_id: String,
    pub container_name: String,
    pub container_local_path: String,
}

/// The left half of a mapping row: what the folder offers for it. Mirror of
/// bae-core's `MappingSource`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingSource {
    /// A file the folder holds, whole.
    File { file: BridgeMappingFile },
    /// One entry of a track sheet, carved out of the container it is bound to.
    SheetEntry { entry: BridgeMappingEntry },
    /// The source names a track this folder has nothing for: the left half is
    /// empty, and the row is offered the folder's audio to point it at.
    Missing,
}

/// The right half of a mapping row: what committing makes of the source unit.
/// Mirror of bae-core's `MappingBecomes`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingBecomes {
    /// A track of the release being committed. The row edits it in place, and
    /// `bridge_mapping_tracks` reads the edited rows back out in commit order.
    Track {
        track: BridgeRawTrackEdit,
        /// The source's own position string — `A1`, `1`, `1-2`, or prose —
        /// where the picked release names one for this track.
        source_position: Option<String>,
        source_duration_ms: Option<u64>,
    },
    /// Carried with the release, not one of its tracks.
    Kept,
    /// No release is picked yet, so what this becomes is the open question.
    AwaitingPick,
}

/// One source unit and the track committing makes of it. Mirror of bae-core's
/// `MappingUnit`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingUnit {
    pub source: BridgeMappingSource,
    pub becomes: BridgeMappingBecomes,
}

/// The audio a track sheet describes. Mirror of bae-core's `MappingContainer`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingContainer {
    pub file_id: String,
    pub name: String,
    pub size: u64,
}

/// A track sheet, as the header of the group of rows it carves. Mirror of
/// bae-core's `SheetGroup`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSheetGroup {
    /// The sheet's `file_id` — the id `AppHandle::set_sheet_binding` and
    /// `AppHandle::set_sheet_disc` take.
    pub sheet_id: String,
    pub name: String,
    /// Absolute path — what opening the sheet to read it reaches.
    pub local_path: String,
    pub bound: BridgeSheetBound,
    pub assignment: BridgeSheetDisc,
    /// The discs this sheet may be assigned to, counting from one.
    pub disc_options: Vec<u32>,
}

/// What a track sheet describes, with the facts its header shows about it.
/// Mirror of bae-core's `SheetBound`.
///
/// `BridgeSheetBinding` enriched by the container's name and size: a header
/// states both which audio a sheet is on and why it is on none, and carrying
/// the binding separately would be a second way to say the first.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSheetBound {
    /// The sheet describes this audio.
    Describes { container: BridgeMappingContainer },
    /// It describes nothing: the directive named audio that is not in the
    /// folder, named several and only some are here, or the user cleared the
    /// binding. `requested` is what the directive asked for, so the header can
    /// say what the sheet was looking for while it offers the folder's own
    /// audio instead.
    Unresolved { requested: Vec<String> },
    /// The directive resolved, but bae cannot carve tracks out of that codec.
    /// The UI localizes `codec` through `bridge_sheet_refused_codec_key`.
    RefusedCodec {
        container: BridgeMappingContainer,
        codec: String,
    },
}

/// One of the folder's images, as the gallery shows it. Mirror of bae-core's
/// `MappingImage`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingImage {
    /// The file's identity within the release (its relative path).
    pub file_id: String,
    /// The file's own name, without its directory prefix.
    pub name: String,
    pub size: u64,
    /// Absolute path — what a thumbnail and the lightbox read.
    pub local_path: String,
    /// Whether this is the image that leads the release.
    pub is_cover: bool,
}

/// One row of the mapping table. Mirror of bae-core's `MappingRow`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeMappingRow {
    /// One source unit and what it becomes.
    Unit { unit: BridgeMappingUnit },
    /// A track sheet and the entries it carves, which are its child rows.
    Sheet {
        sheet: BridgeSheetGroup,
        entries: Vec<BridgeMappingUnit>,
    },
    /// Every image the folder holds, shown as one gallery.
    Images { images: Vec<BridgeMappingImage> },
    /// A directory whose files all do the same job, shown as one row.
    Directory { directory: BridgeCollapsedDirectory },
}

/// The mapping table: every source unit the folder offers, alongside the track
/// committing makes of it. Mirror of bae-core's `MappingTable`.
///
/// One structure, not two lists to keep aligned: the editable track row lives
/// *inside* the row that produces it, so removing a row removes both halves and
/// no index addresses anything.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeMappingTable {
    pub rows: Vec<BridgeMappingRow>,
    /// The tally over the rows that become tracks. `None` when there is nothing
    /// to reconcile the folder against — no release is picked, or the tracklist
    /// was read off the folder's own files and so cannot disagree with it.
    pub reconciliation: Option<BridgeSlotReconciliation>,
}

/// What committing a folder as Unknown produces: the release its own files
/// describe, and the mapping table that lands each of its audio units on one of
/// those tracks.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeUnknownMapping {
    pub seed: BridgeReleaseUserEdit,
    pub mapping: BridgeMappingTable,
}

/// The table's track rows in commit order — what the editor shapes into the
/// release it writes. Core decides the order, so the two desktop surfaces
/// cannot commit two different tracklists from one table.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_tracks(table: BridgeMappingTable) -> Vec<BridgeRawTrackEdit> {
    bae_core::import::mapping_tracks(&table.into_core())
        .into_iter()
        .map(BridgeRawTrackEdit::from_core)
        .collect()
}

/// Write an edited track row back onto the row that commits it, found by the
/// track's own id. A row nothing matches leaves the table alone.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_with_track(
    table: BridgeMappingTable,
    track: BridgeRawTrackEdit,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_with_track(
        table.into_core(),
        track.into_core(),
    ))
}

/// Drop the row committing the track with `track_id` — the Drop action on a
/// track the release names that this folder has nothing for. Nothing is
/// persisted: the folder is unchanged, the release is committed without it.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_without_track(
    table: BridgeMappingTable,
    track_id: String,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_without_track(
        table.into_core(),
        &track_id,
    ))
}

/// Drop every row the file `file_id` backs — the Exclude action, once the role
/// change that persists it has landed. One container backs every entry of the
/// sheet bound to it, so that sheet's whole group leaves with it.
#[cfg(feature = "desktop")]
#[uniffi::export]
pub fn bridge_mapping_without_file(
    table: BridgeMappingTable,
    file_id: String,
) -> BridgeMappingTable {
    BridgeMappingTable::from_core(bae_core::import::mapping_without_file(
        table.into_core(),
        &file_id,
    ))
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
    /// The audio bound to this row. An editor must carry it through untouched:
    /// dropping it when rebuilding a row from its text fields is what unpairs a
    /// track the user had already paired.
    pub file: Option<BridgeAudioFile>,
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
    /// How many blob uploads run at once. Device-local; range 1..=8. Desktop
    /// exposes a control for it, mobile does not (mobile makes no uploads).
    pub max_concurrent_uploads: u32,
    /// How many blob downloads a pin fetches at once. Device-local; range 1..=8.
    pub max_concurrent_downloads: u32,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. A synced preference, not a
    /// per-device one — the seek bar reads it and never stores a copy.
    pub show_remaining_time: bool,
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. A synced preference;
    /// the library page reads it and never stores a copy.
    pub library_full_width: bool,
    /// Configured export presets offered by release and track export.
    pub save_presets: Vec<BridgeSavePreset>,
    /// Id of the preset a track save defaults to (a valid, track-applicable
    /// preset id; core keeps it non-dangling).
    pub default_track_save_preset: String,
    /// Id of the preset a release save defaults to (a valid, release-applicable
    /// preset id; core keeps it non-dangling).
    pub default_release_save_preset: String,
    /// Whether casting to a network receiver is available. Off unless the user
    /// turns it on; while off, core runs no discovery and refuses to start a
    /// session, and the UI hides its Cast control.
    pub cast_enabled: bool,
    pub mcp: BridgeMcpConfig,
    pub subsonic: BridgeSubsonicConfig,
    pub discogs_token_status: BridgeDiscogsTokenStatus,
    /// Whether Discogs can be used as a metadata source (a stored key that
    /// isn't rejected). Core decides the policy via `DiscogsTokenStatus::
    /// is_usable`; the UI reads this flag rather than re-deriving it from the
    /// status.
    pub discogs_usable: bool,
    /// The configured cloud provider, present whenever YAML carries one — so
    /// the settings tab can render the previous selection even when sync is
    /// broken. Does not imply sync is working: runtime status lives in
    /// `BridgeSyncStatusSnapshot`, not config.
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

/// On-disk Subsonic server settings surfaced to the UI. The password is
/// keyring-only and is set through `set_subsonic_password`, so it is not here.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSubsonicConfig {
    pub enabled: bool,
    pub port: u16,
    pub username: String,
    /// The IP the server binds. `127.0.0.1` keeps it on this machine; `0.0.0.0`
    /// opens it to other devices on the network. The UI presents this as a
    /// network-access toggle rather than a raw address field.
    pub bind_address: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSubsonicServerStatus {
    Disabled,
    Running { url: String },
    Error { error: BridgeSubsonicServerError },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSubsonicServerError {
    InvalidConfig { detail: String },
    CredentialUnavailable { detail: String },
    BindFailed { detail: String },
    ServerFailed { detail: String },
}

/// A discovered remote-renderer device (Cast or UPnP), for the device picker.
/// One list, tagged by `kind` — the picker doesn't segregate by protocol.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCastDevice {
    /// Opaque id passed back to `cast_to`.
    pub id: String,
    /// Display name shown in the picker.
    pub name: String,
    /// Which protocol the device speaks, so the row can carry a flavor hint.
    pub kind: BridgeRendererKind,
}

/// The protocol flavor of a discovered device.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeRendererKind {
    Cast,
    Dlna,
    AirPlay,
}

// The conversion is only used by the cast-gated `get_cast_devices` handle fn.
#[cfg(feature = "cast")]
impl BridgeCastDevice {
    pub(crate) fn from_core(device: bae_core::renderer::RendererDevice) -> Self {
        let kind = match device.kind() {
            bae_core::renderer::RendererKind::Cast => BridgeRendererKind::Cast,
            bae_core::renderer::RendererKind::Dlna => BridgeRendererKind::Dlna,
            bae_core::renderer::RendererKind::AirPlay => BridgeRendererKind::AirPlay,
        };
        Self {
            id: device.id,
            name: device.name,
            kind,
        }
    }
}

/// Whether playback is on a Cast device and, if so, which. The `from_core`
/// mapping lives in `handle.rs` with the other `bae_cast` conversions (the cast
/// crate is feature-gated).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCastStatus {
    NotCasting,
    Casting { device_name: String },
}

/// A service type a renderer advertises itself on, tagging which mapping a
/// reported service goes through.
#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeRendererServiceType {
    GoogleCast,
    AirPlay,
    Raop,
}

/// One service type a host that browses on bae's behalf must browse for: the
/// DNS-SD type to hand its browser, and the tag to report each result under.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRendererService {
    pub service_type: BridgeRendererServiceType,
    /// The DNS-SD service type, e.g. `_googlecast._tcp`.
    pub dns_sd_type: String,
}

/// A renderer service a host's browser resolved, as it came off the wire. What
/// it means — which device it is, what to call it, what its TXT bits allow — is
/// decided in core.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeReportedRenderer {
    pub service_type: BridgeRendererServiceType,
    /// The service instance name, which is also what a later `renderer_lost`
    /// names.
    pub instance_name: String,
    /// The resolved address, in text form.
    pub addr: String,
    pub port: u16,
    /// The service's TXT record.
    pub txt: std::collections::HashMap<String, String>,
}

#[cfg(feature = "cast")]
impl BridgeRendererServiceType {
    pub(crate) fn from_core(service_type: bae_core::renderer::RendererServiceType) -> Self {
        use bae_core::renderer::RendererServiceType;
        match service_type {
            RendererServiceType::GoogleCast => Self::GoogleCast,
            RendererServiceType::AirPlay => Self::AirPlay,
            RendererServiceType::Raop => Self::Raop,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::renderer::RendererServiceType {
        use bae_core::renderer::RendererServiceType;
        match self {
            Self::GoogleCast => RendererServiceType::GoogleCast,
            Self::AirPlay => RendererServiceType::AirPlay,
            Self::Raop => RendererServiceType::Raop,
        }
    }
}

#[cfg(feature = "cast")]
impl BridgeRendererService {
    pub(crate) fn from_core(service_type: bae_core::renderer::RendererServiceType) -> Self {
        Self {
            service_type: BridgeRendererServiceType::from_core(service_type),
            dns_sd_type: service_type.dns_sd_type().to_string(),
        }
    }
}

#[cfg(feature = "cast")]
impl BridgeReportedRenderer {
    pub(crate) fn into_core(self) -> bae_core::renderer::ReportedRenderer {
        bae_core::renderer::ReportedRenderer {
            service_type: self.service_type.into_core(),
            instance_name: self.instance_name,
            addr: self.addr,
            port: self.port,
            txt: self.txt,
        }
    }
}

/// Cloud sync settings for a connected provider. `provider` carries the
/// provider-specific fields; the rest are shared across providers. Whether
/// sync is actually running is `BridgeSyncStatusSnapshot.sync_ready`, kept
/// orthogonal.
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
    pub artists: Vec<BridgeArtistSummary>,
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
    /// The track length as a clock label's fields ("3:07"), or `None` when there
    /// is nothing to label. The raw milliseconds do not cross — the search row
    /// only ever shows the clock, never the number.
    pub duration_clock: Option<BridgeDurationClock>,
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    /// The cover of the track's own release, or `None`. Same fetch/caching
    /// contract as [`BridgeAlbumSearchResult::cover`].
    pub cover: Option<BridgeImageRef>,
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

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSummary {
    pub artist_id: String,
    pub name: String,
    /// Distinct albums this artist is an album artist of (primary FK or
    /// `album_artists` junction).
    pub album_count: i64,
    pub image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistDetail {
    pub artist: BridgeArtistSummary,
    /// The artist's albums in discography order: year ascending with unknown
    /// years last, then title.
    pub albums: Vec<BridgeAlbum>,
}

#[derive(Debug, Clone, Copy, uniffi::Enum)]
pub enum BridgeArtistSortField {
    Name,
    AlbumCount,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeArtistSortCriterion {
    pub field: BridgeArtistSortField,
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
    /// The track length as a clock label's fields ("3:07"), or `None` when there
    /// is nothing to label. The raw milliseconds do not cross — a queue row only
    /// ever shows the clock, never the number.
    pub duration_clock: Option<BridgeDurationClock>,
    pub album_title: String,
    /// The track's own release's cover, or `None` when it has none. Versioned,
    /// so the UI's art cache key moves when the cover bytes change.
    pub cover_image: Option<BridgeImageRef>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueSnapshot {
    pub manual: Vec<BridgeQueueEntry>,
    pub context: Option<BridgePlaybackContext>,
    pub has_next: bool,
    pub has_previous: bool,
    /// The queue revision this snapshot was resolved from. The UI stamps any
    /// `get_queue_upcoming_page` fetch it makes while showing this snapshot
    /// with this value, and drops the reply if its revision no longer
    /// matches — a `QueueUpdated` for the newer revision has already replaced
    /// the view.
    pub revision: u64,
}

/// One page of the context's upcoming tail, fetched by offset/limit past the
/// initial window `BridgePlaybackContext.upcoming` already carries.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeQueueUpcomingPage {
    pub revision: u64,
    pub entries: Vec<BridgeQueueEntry>,
}

/// Which kind of source the context plays from, so the UI labels the section
/// (a release's "Playing From" vs the whole library). A single- or multi-release
/// source is both `Release` here — the queue pane's "Playing From" title is the
/// same for one album or several. The release ids stay in core (the UI labels by
/// kind, not by id here).
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgePlaybackSourceKind {
    Release,
    Library,
}

impl BridgePlaybackSourceKind {
    pub(crate) fn from_core(source: &bae_core::playback::ContextSource) -> Self {
        match source {
            bae_core::playback::ContextSource::Release(_)
            | bae_core::playback::ContextSource::Releases(_) => Self::Release,
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
    /// The display title of what the context plays from — the album title when
    /// the source is a single release, `None` for a multi-release source or
    /// the whole library. The UI appends it to the section label; the label
    /// prose itself stays UI-side (localized).
    pub source_title: Option<String>,
    pub shuffled: bool,
    /// The first page of the not-yet-played tail — not the whole tail. See
    /// `upcoming_total` for the full length and `get_queue_upcoming_page` for
    /// fetching the rest.
    pub upcoming: Vec<BridgeQueueEntry>,
    /// The full length of the not-yet-played tail, including entries beyond
    /// `upcoming`. The UI renders a placeholder for every index up to this and
    /// pages in the rest as it scrolls.
    pub upcoming_total: u64,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSyncStatusSnapshot {
    pub error: Option<BridgeError>,
    pub last_sync_time: Option<i64>,
    pub syncing: bool,
    pub sync_ready: bool,
}

/// What the sync indicator shows, in precedence order. Mirror of bae-core's
/// `SyncIndicator`. The UI maps a variant to a label and colour and renders the
/// `Synced` time; it never decides which state wins — a stale timestamp used to
/// read as "Synced" on a loop that never came up, on Windows, because each app
/// wrote its own precedence.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSyncIndicator {
    Error,
    Syncing,
    Synced { last_sync_time: Option<i64> },
    Idle,
}

impl BridgeSyncIndicator {
    fn from_core(indicator: bae_core::library::SyncIndicator) -> Self {
        use bae_core::library::SyncIndicator;
        match indicator {
            SyncIndicator::Error => Self::Error,
            SyncIndicator::Syncing => Self::Syncing,
            SyncIndicator::Synced { last_sync_time } => Self::Synced { last_sync_time },
            SyncIndicator::Idle => Self::Idle,
        }
    }
}

/// The sync indicator for a status snapshot — the precedence decided in bae-core.
/// The UI holds the snapshot already; this turns it into the one badge state.
#[uniffi::export]
pub fn bridge_sync_indicator(snapshot: &BridgeSyncStatusSnapshot) -> BridgeSyncIndicator {
    BridgeSyncIndicator::from_core(bae_core::library::SyncIndicator::resolve(
        snapshot.error.is_some(),
        snapshot.syncing,
        snapshot.sync_ready,
        snapshot.last_sync_time,
    ))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveBitDepth {
    Source,
    Bits16,
    Bits24,
    Bits32,
}

/// One piece of an export filename pattern — an ordered token list; rendering
/// substitutes each token's value and joins the non-empty values with single
/// spaces. Mirror of bae-core's `SaveFilenameToken`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveFilenameToken {
    Title,
    Artist,
    Album,
    Year,
    TrackNumber,
    DiscNumber,
    TrackTotal,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSaveCodec {
    Flac { bit_depth: BridgeSaveBitDepth },
    Mp3 { bitrate_kbps: u32 },
    Aac { bitrate_kbps: u32 },
    OpusOgg { bitrate_kbps: u32 },
    Wav { bit_depth: BridgeSaveBitDepth },
    Aiff { bit_depth: BridgeSaveBitDepth },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSavePregapPlacement {
    AppendToPreviousExceptHtoa,
    AppendToPreviousIncludingHtoa,
    Exclude,
    SingleFileWithCue,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSavePreset {
    pub id: String,
    pub name: String,
    pub codec: BridgeSaveCodec,
    pub extension: String,
    pub filename_tokens: Vec<BridgeSaveFilenameToken>,
    pub pregap_placement: BridgeSavePregapPlacement,
    pub applies_to_track: bool,
    pub applies_to_release: bool,
    /// Whether saved files embed the release's cover art.
    pub embed_cover: bool,
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
    Save,
    /// A cloud provider rejected the request or the setup is misconfigured (bad
    /// credentials, denied permission, a bucket/folder that isn't set).
    Credentials,
    /// The cloud backend or the network to it was unreachable — retryable.
    Network,
    /// The device's OS keyring (secure credential store) couldn't be read/written.
    Keyring,
    /// A library-sharing membership operation failed (the membership chain, an
    /// invite, or cross-device key rotation).
    Membership,
    /// An AirPlay receiver can't be driven — it demands a PIN, or offers only
    /// audio encryption the sender doesn't implement.
    AirPlayUnsupported,
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
    /// Desktop-only with the output surface it reports on: iOS/Android compile
    /// out the output queue and the track saver entirely.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn export(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Export, detail)
    }
    /// Desktop-only, like [`Self::export`]: reports a save (rendered-output)
    /// failure.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn save(detail: impl std::fmt::Display) -> Self {
        Self::diagnostic(BridgeErrorCategory::Save, detail)
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
        BridgeErrorCategory::Save => "core.error.category.save",
        BridgeErrorCategory::Credentials => "core.error.category.credentials",
        BridgeErrorCategory::Network => "core.error.category.network",
        BridgeErrorCategory::Keyring => "core.error.category.keyring",
        BridgeErrorCategory::Membership => "core.error.category.membership",
        BridgeErrorCategory::AirPlayUnsupported => "core.error.category.airplay_unsupported",
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

/// The catalog key for an error's user-facing line, or `None` when the error has
/// no line to show.
///
/// A cancellation is the user's own doing and says nothing back to them, so it
/// has no line — and `None` is how that is said. Left to each app, "no line"
/// became `""` on Swift and Kotlin (which then opened a blank error alert,
/// because nothing filters an empty string) and "an internal error occurred" on
/// Windows, which is neither internal nor an error. Whether an error is worth
/// showing is a decision about the error, so it is made once, here.
#[uniffi::export]
pub fn bridge_error_line_key(error: &BridgeError) -> Option<String> {
    match error {
        BridgeError::Cancelled => None,
        BridgeError::NotFound { entity, .. } => Some(bridge_entity_not_found_key(*entity)),
        BridgeError::Diagnostic { category, .. } => Some(bridge_error_category_key(*category)),
    }
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
//
// Convention, crate-wide — conversions also sit above this banner, in
// `handle.rs`, and in `bridge_utils.rs`: a conversion is an associated function
// on the `Bridge*` type, `BridgeX::from_core(core) -> Self` and
// `BridgeX::into_core(self) -> core::X`. Two exceptions: some `from_core`s take
// extra bridge-only arguments (`BridgeReleaseDetail`,
// `BridgeCandidateSearchResults`), and `LibraryError` crosses through a `From`
// impl.
//
// Record converters exhaustively destructure their core input(s) — no `..` in
// any struct or enum-variant pattern — so a new bae-core field fails the build
// here instead of silently never crossing the bridge. A dropped field is named
// explicitly (`field: _`), with a comment when the drop isn't obvious. Fields of
// external-crate types (coven's `Config`/`CloudHomeConfig`, …) are exempt — the
// bridge can't police a pinned crate's field list — and stay dotted reads.
// =========================================================================

impl BridgeErrorCategory {
    pub(crate) fn from_core(category: bae_core::ui::UiErrorCategory) -> Self {
        use bae_core::ui::UiErrorCategory;
        match category {
            UiErrorCategory::Database => BridgeErrorCategory::Database,
            UiErrorCategory::Config => BridgeErrorCategory::Config,
            UiErrorCategory::Internal => BridgeErrorCategory::Internal,
            UiErrorCategory::Import => BridgeErrorCategory::Import,
            UiErrorCategory::Export => BridgeErrorCategory::Export,
            UiErrorCategory::Save => BridgeErrorCategory::Save,
            UiErrorCategory::Credentials => BridgeErrorCategory::Credentials,
            UiErrorCategory::Network => BridgeErrorCategory::Network,
            UiErrorCategory::Keyring => BridgeErrorCategory::Keyring,
            UiErrorCategory::Membership => BridgeErrorCategory::Membership,
        }
    }
}

impl BridgeEntityKind {
    fn from_core(entity: bae_core::ui::UiEntityKind) -> Self {
        use bae_core::ui::UiEntityKind;
        match entity {
            UiEntityKind::Library => BridgeEntityKind::Library,
            UiEntityKind::Album => BridgeEntityKind::Album,
            UiEntityKind::Release => BridgeEntityKind::Release,
            UiEntityKind::Track => BridgeEntityKind::Track,
            UiEntityKind::File => BridgeEntityKind::File,
        }
    }
}

impl BridgeError {
    pub(crate) fn from_core(error: bae_core::ui::UiError) -> Self {
        use bae_core::ui::UiError;
        match error {
            UiError::NotFound { entity, id } => BridgeError::NotFound {
                entity: BridgeEntityKind::from_core(entity),
                id,
            },
            UiError::Diagnostic { category, detail } => BridgeError::Diagnostic {
                category: BridgeErrorCategory::from_core(category),
                detail,
            },
        }
    }
}

/// Carry a core `LibraryError`'s diagnostic class across the bridge: the class
/// (keyring vs cloud credentials vs network vs membership vs …) becomes the
/// `BridgeErrorCategory` the UI renders a localized line for; the error chain
/// rides along as opaque, log-only detail.
impl From<bae_core::library::LibraryError> for BridgeError {
    fn from(error: bae_core::library::LibraryError) -> Self {
        BridgeError::Diagnostic {
            category: BridgeErrorCategory::from_core(error.category()),
            detail: error.to_string(),
        }
    }
}

impl BridgePlaybackErrorReason {
    pub(crate) fn from_core(reason: bae_core::ui::PlaybackErrorReason) -> Self {
        use bae_core::ui::PlaybackErrorReason;
        match reason {
            PlaybackErrorReason::SyncDisconnected => BridgePlaybackErrorReason::SyncDisconnected,
            PlaybackErrorReason::UploadPending => BridgePlaybackErrorReason::UploadPending,
            PlaybackErrorReason::Diagnostic { error } => BridgePlaybackErrorReason::Diagnostic {
                error: BridgeError::from_core(error),
            },
        }
    }
}

/// Map the UI's storage-state choice to the core's `StorageMode`. Pinned-ness is
/// orthogonal — the caller passes the import's `pin` choice separately.
#[cfg(feature = "desktop")]
impl BridgeStorageMode {
    pub(crate) fn into_core(self) -> bae_core::import::StorageMode {
        use bae_core::import::StorageMode;
        match self {
            BridgeStorageMode::Local => StorageMode::Local,
            BridgeStorageMode::Remote => StorageMode::Remote,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCoverSelection {
    pub(crate) fn into_core(self) -> bae_core::import::CoverSelection {
        match self {
            BridgeCoverSelection::ReleaseImage { file_id } => {
                bae_core::import::CoverSelection::Local(file_id)
            }
            BridgeCoverSelection::RemoteCover { selection } => {
                bae_core::import::CoverSelection::Remote(
                    selection.url,
                    selection.source.into_core(),
                )
            }
        }
    }
}

impl BridgeCloudProvider {
    pub(crate) fn from_core(p: &bae_core::config::CloudProvider) -> Self {
        use bae_core::config::CloudProvider;
        match p {
            CloudProvider::S3 => BridgeCloudProvider::S3,
            CloudProvider::GoogleDrive => BridgeCloudProvider::GoogleDrive,
            CloudProvider::Dropbox => BridgeCloudProvider::Dropbox,
            CloudProvider::OneDrive => BridgeCloudProvider::OneDrive,
            CloudProvider::CloudKit => BridgeCloudProvider::CloudKit,
        }
    }
}

impl BridgeMemberRole {
    pub(crate) fn from_core(role: bae_core::sync::membership::MemberRole) -> Self {
        use bae_core::sync::membership::MemberRole;
        match role {
            MemberRole::Owner => BridgeMemberRole::Owner,
            MemberRole::Member => BridgeMemberRole::Member,
            MemberRole::Follower => BridgeMemberRole::Follower,
        }
    }
}

impl BridgeMember {
    fn from_core(m: bae_core::sync::membership::MembershipMember) -> Self {
        let bae_core::sync::membership::MembershipMember {
            pubkey,
            role,
            is_self,
            fingerprint,
            can_remove,
        } = m;
        BridgeMember {
            pubkey,
            role: BridgeMemberRole::from_core(role),
            is_self,
            fingerprint,
            can_remove,
        }
    }
}

impl BridgeMembership {
    pub(crate) fn from_core(membership: bae_core::sync::membership::Membership) -> Self {
        let bae_core::sync::membership::Membership {
            members,
            self_is_owner,
        } = membership;
        BridgeMembership {
            members: members.into_iter().map(BridgeMember::from_core).collect(),
            self_is_owner,
        }
    }
}

/// Only the OAuth sign-in and authorize flows map a bare bridge provider back to
/// core; gated so non-OAuth builds don't carry a dead mapping.
#[cfg(feature = "oauth-providers")]
impl BridgeCloudProvider {
    pub(crate) fn into_core(self) -> bae_core::config::CloudProvider {
        use bae_core::config::CloudProvider;
        match self {
            BridgeCloudProvider::S3 => CloudProvider::S3,
            BridgeCloudProvider::GoogleDrive => CloudProvider::GoogleDrive,
            BridgeCloudProvider::Dropbox => CloudProvider::Dropbox,
            BridgeCloudProvider::OneDrive => CloudProvider::OneDrive,
            BridgeCloudProvider::CloudKit => CloudProvider::CloudKit,
        }
    }
}

impl BridgeHomeStorage {
    pub(crate) fn into_core(self) -> bae_core::config::HomeStorage {
        use bae_core::config::HomeStorage;
        match self {
            BridgeHomeStorage::Opaque => HomeStorage::Opaque,
            BridgeHomeStorage::Browsable => HomeStorage::Browsable,
        }
    }
}

impl BridgeStorageSort {
    pub(crate) fn into_core(self) -> bae_core::db::StorageSortCriterion {
        let BridgeStorageSort { field, direction } = self;
        bae_core::db::StorageSortCriterion {
            field: match field {
                BridgeStorageSortField::AlbumTitle => bae_core::db::StorageSortField::AlbumTitle,
                BridgeStorageSortField::ArtistNames => bae_core::db::StorageSortField::ArtistNames,
                BridgeStorageSortField::Format => bae_core::db::StorageSortField::Format,
                BridgeStorageSortField::FileCount => bae_core::db::StorageSortField::FileCount,
                BridgeStorageSortField::TotalSize => bae_core::db::StorageSortField::TotalSize,
            },
            direction: match direction {
                BridgeStorageSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
                BridgeStorageSortDirection::Descending => bae_core::db::SortDirection::Descending,
            },
        }
    }
}

impl BridgeStorageFilter {
    pub(crate) fn into_core(self) -> bae_core::db::StorageFilter {
        match self {
            BridgeStorageFilter::All => bae_core::db::StorageFilter::All,
            BridgeStorageFilter::Remote => bae_core::db::StorageFilter::Remote,
            BridgeStorageFilter::Local => bae_core::db::StorageFilter::Local,
            BridgeStorageFilter::Uploading => bae_core::db::StorageFilter::Uploading,
        }
    }
}

impl BridgeComposerSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::ComposerSortCriterion {
        let BridgeComposerSortCriterion { field, direction } = self;
        bae_core::db::ComposerSortCriterion {
            field: match field {
                BridgeComposerSortField::Name => bae_core::db::ComposerSortField::Name,
                BridgeComposerSortField::WorkCount => bae_core::db::ComposerSortField::WorkCount,
                BridgeComposerSortField::LinkedReleaseCount => {
                    bae_core::db::ComposerSortField::LinkedReleaseCount
                }
            },
            direction: direction.into_core(),
        }
    }
}

impl BridgeArtistSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::ArtistSortCriterion {
        let BridgeArtistSortCriterion { field, direction } = self;
        bae_core::db::ArtistSortCriterion {
            field: match field {
                BridgeArtistSortField::Name => bae_core::db::ArtistSortField::Name,
                BridgeArtistSortField::AlbumCount => bae_core::db::ArtistSortField::AlbumCount,
            },
            direction: direction.into_core(),
        }
    }
}

impl BridgeSortDirection {
    pub(crate) fn into_core(self) -> bae_core::db::SortDirection {
        match self {
            BridgeSortDirection::Ascending => bae_core::db::SortDirection::Ascending,
            BridgeSortDirection::Descending => bae_core::db::SortDirection::Descending,
        }
    }
}

impl BridgeSortCriterion {
    pub(crate) fn into_core(self) -> bae_core::db::AlbumSortCriterion {
        let BridgeSortCriterion { field, direction } = self;
        bae_core::db::AlbumSortCriterion {
            field: match field {
                BridgeSortField::Title => bae_core::db::AlbumSortField::Title,
                BridgeSortField::Artist => bae_core::db::AlbumSortField::Artist,
                BridgeSortField::Year => bae_core::db::AlbumSortField::Year,
                BridgeSortField::DateAdded => bae_core::db::AlbumSortField::DateAdded,
            },
            direction: direction.into_core(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMetadataResult {
    pub(crate) fn from_core(r: bae_core::import::search::MetadataResult) -> Self {
        let bae_core::import::search::MetadataResult {
            source,
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
            // Dropped: the card carries the album's title/artist/cover, so a
            // pressing projection keeps only pressing-distinguishing fields.
            title: _,
            artist: _,
            cover_art: _,
            source_group_id: _,
            // The source's own tracklist is Ready-rule evidence, not something
            // a pressing row renders; the sidebar reads the classification the
            // rule produced from it.
            source_tracks: _,
        } = r;
        BridgeMetadataResult {
            source: BridgeMetadataSource::from_core(source),
            release_id,
            year,
            format,
            label,
            catalog_number,
            country,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRemoteCover {
    pub(crate) fn from_core(c: bae_core::import::cover_art::RemoteCover) -> Self {
        let bae_core::import::cover_art::RemoteCover {
            url,
            thumbnail_url,
            label,
            source,
        } = c;
        let selection = bridge_remote_cover_selection(url, source);
        let cover_choice = remote_cover_choice_to_bridge(&selection, &thumbnail_url);
        BridgeRemoteCover {
            cover_choice,
            label,
        }
    }
}

#[cfg(feature = "desktop")]
fn bridge_remote_cover_selection(
    url: String,
    source: bae_core::import::MetadataSource,
) -> BridgeRemoteCoverSelection {
    BridgeRemoteCoverSelection {
        url,
        source: BridgeMetadataSource::from_core(source),
    }
}

#[cfg(feature = "desktop")]
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
impl BridgeReleaseDetail {
    pub(crate) fn from_core(d: bae_core::import::search::ImportSearchReleaseDetail) -> Self {
        // Derived values borrow `&d`; compute them before destructuring `d`.
        let default_cover = d
            .default_cover()
            .cloned()
            .map(BridgeRemoteCover::from_core)
            .map(|c| c.cover_choice);
        let bae_core::import::search::ImportSearchReleaseDetail {
            release_id,
            source,
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks,
            cover_art,
        } = d;
        BridgeReleaseDetail {
            release_id,
            source: BridgeMetadataSource::from_core(source),
            source_group_id,
            title,
            artist,
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
            track_count,
            tracks: tracks
                .into_iter()
                .map(BridgeReleaseTrack::from_core)
                .collect(),
            cover_art: cover_art
                .into_iter()
                .map(BridgeRemoteCover::from_core)
                .collect(),
            default_cover,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseTrack {
    pub(crate) fn from_core(t: bae_core::import::search::ReleaseTrack) -> Self {
        let bae_core::import::search::ReleaseTrack {
            title,
            artist,
            duration_ms,
            position,
            side,
        } = t;
        Self {
            title,
            artist,
            duration_ms,
            position,
            side,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleasePrefetch {
    pub(crate) fn from_core(p: bae_core::import::search::ImportReleasePrefetch) -> Self {
        let bae_core::import::search::ImportReleasePrefetch {
            detail,
            seed,
            claim,
            mapping,
        } = p;
        // The seed crosses masked for the claim the pick settled, so the editor
        // binds it directly. Doing it here rather than in the UI is what keeps
        // the two desktop surfaces from each deciding what an album-level claim
        // shows.
        let seed = bae_core::import::shape_user_edit_for_choice(&seed, &claim.choice);
        BridgeReleasePrefetch {
            detail: BridgeReleaseDetail::from_core(detail),
            seed: BridgeReleaseUserEdit::from_core(seed),
            claim: BridgeClaimLine::from_core(claim),
            mapping: BridgeMappingTable::from_core(mapping),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeClaimLine {
    pub(crate) fn from_core(claim: bae_core::import::ClaimLine) -> Self {
        let bae_core::import::ClaimLine {
            choice,
            level,
            evidence,
            release,
            track_count,
        } = claim;
        BridgeClaimLine {
            choice: BridgeIdentityChoice::from_core(choice),
            level: BridgeClaimLevel::from_core(level),
            evidence: BridgeClaimEvidence::from_core(evidence),
            release,
            track_count,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeClaimEvidence {
    fn from_core(evidence: bae_core::import::ClaimEvidence) -> Self {
        use bae_core::import::ClaimEvidence;
        match evidence {
            ClaimEvidence::DiscIdAlone => BridgeClaimEvidence::DiscIdAlone,
            ClaimEvidence::DiscIdShared { match_count } => {
                BridgeClaimEvidence::DiscIdShared { match_count }
            }
            ClaimEvidence::Barcode => BridgeClaimEvidence::Barcode,
            ClaimEvidence::Search => BridgeClaimEvidence::Search,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeDiscidProgress {
    fn from_view(p: bae_core::identify::DiscidProgressView) -> Self {
        use bae_core::identify::DiscidProgressView;
        match p {
            DiscidProgressView::Computing => BridgeDiscidProgress::Computing,
            DiscidProgressView::LookingUp => BridgeDiscidProgress::LookingUp,
            DiscidProgressView::Done { n_results } => BridgeDiscidProgress::Done { n_results },
            DiscidProgressView::Skipped => BridgeDiscidProgress::Skipped,
            DiscidProgressView::Failed { failure } => BridgeDiscidProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeBarcodeProgress {
    fn from_view(p: bae_core::identify::BarcodeProgressView) -> Self {
        use bae_core::identify::BarcodeProgressView;
        match p {
            BarcodeProgressView::Scanning => BridgeBarcodeProgress::Scanning,
            BarcodeProgressView::LookingUp {
                current,
                position,
                total,
            } => BridgeBarcodeProgress::LookingUp {
                current,
                position,
                total,
            },
            BarcodeProgressView::Done { n_results } => BridgeBarcodeProgress::Done { n_results },
            BarcodeProgressView::Failed { failure } => BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::from_core(failure),
            },
            BarcodeProgressView::Skipped => BridgeBarcodeProgress::Skipped,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseGroup {
    pub(crate) fn from_core(g: bae_core::import::release_group::ReleaseGroup) -> Self {
        let bae_core::import::release_group::ReleaseGroup {
            id,
            title,
            artist,
            cover_art,
            source_label,
            group_url,
            year_min,
            year_max,
            pressings,
        } = g;
        BridgeReleaseGroup {
            id,
            title,
            artist,
            cover_art: cover_art.map(BridgeRemoteCover::from_core),
            source_label,
            group_url,
            year_min,
            year_max,
            pressings: pressings
                .into_iter()
                .map(BridgeMetadataResult::from_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSignals {
    pub(crate) fn from_core(s: bae_core::signals::Signals) -> Self {
        use bae_core::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

        fn sourced_values(values: Vec<bae_core::signals::SourcedValue>) -> Vec<BridgeSourcedValue> {
            values
                .into_iter()
                .map(BridgeSourcedValue::from_core)
                .collect()
        }

        let Signals {
            disc_id,
            barcode,
            text,
            // The probed total is a Ready-rule input, not a badge: the sidebar
            // reads a candidate's classification, and the mapping pane will
            // read per-file durations it probes for the one open candidate.
            // Neither wants this number, so it does not cross.
            probed_total_duration_ms: _,
        } = s;

        let disc_id = match disc_id {
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
                failure: BridgeLookupFailure::from_core(failure),
                track_count,
            },
        };

        let barcode = match barcode {
            BarcodeSignal::Scanning { codes } => BridgeBarcodeSignal::Scanning {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Settled { codes } => BridgeBarcodeSignal::Settled {
                codes: sourced_values(codes),
            },
            BarcodeSignal::Failed { failure, codes } => BridgeBarcodeSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                codes: sourced_values(codes),
            },
            BarcodeSignal::Absent => BridgeBarcodeSignal::Absent,
        };

        let text = match text {
            TextSignal::Scanning {
                catalogs,
                free_text,
            } => BridgeTextSignal::Scanning {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Settled {
                catalogs,
                free_text,
            } => BridgeTextSignal::Settled {
                catalogs: sourced_values(catalogs),
                free_text,
            },
            TextSignal::Failed {
                failure,
                catalogs,
                free_text,
            } => BridgeTextSignal::Failed {
                failure: BridgeLookupFailure::from_core(failure),
                catalogs: sourced_values(catalogs),
                free_text,
            },
        };

        BridgeSignals {
            disc_id,
            barcode,
            text,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeResultProvenance {
    fn from_core(p: bae_core::identify::ResultProvenance) -> Self {
        let bae_core::identify::ResultProvenance {
            by_disc_id,
            by_barcode,
            matches_catalog,
        } = p;
        BridgeResultProvenance {
            by_disc_id,
            by_barcode,
            matches_catalog,
        }
    }
}

/// Mirror [`bae_core::identify::IdentifyStateView`] into the uniffi enum. Core has
/// already folded the matches into their group, keyed the provenance, reduced the
/// in-flight payloads to counts, and dropped what must not cross — this is a field
/// copy per variant and nothing else.
#[cfg(feature = "desktop")]
impl BridgeIdentifyState {
    pub(crate) fn from_core(s: bae_core::identify::IdentifyState) -> Self {
        use bae_core::identify::IdentifyStateView;
        match IdentifyStateView::from(s) {
            IdentifyStateView::Idle => BridgeIdentifyState::Idle,
            IdentifyStateView::Triangulating { discid, barcode } => {
                BridgeIdentifyState::Triangulating {
                    discid: BridgeDiscidProgress::from_view(discid),
                    barcode: BridgeBarcodeProgress::from_view(barcode),
                }
            }
            IdentifyStateView::Found {
                group,
                library_statuses,
                track_count,
                provenance,
            } => BridgeIdentifyState::Found {
                group: BridgeReleaseGroup::from_core(group),
                library_statuses: status_map(library_statuses),
                track_count,
                provenance: provenance
                    .into_iter()
                    .map(|(release_id, p)| (release_id, BridgeResultProvenance::from_core(p)))
                    .collect(),
            },
            IdentifyStateView::Conflict {
                discid_results,
                barcode_results,
                matched_barcode,
                track_count,
            } => {
                let (discid_results, discid_library_statuses) =
                    results_and_status_map(discid_results);
                let (barcode_results, barcode_library_statuses) =
                    results_and_status_map(barcode_results);
                BridgeIdentifyState::Conflict {
                    discid_results,
                    discid_library_statuses,
                    barcode_results,
                    barcode_library_statuses,
                    matched_barcode,
                    track_count,
                }
            }
            IdentifyStateView::NotFoundAnywhere => BridgeIdentifyState::NotFoundAnywhere,
            IdentifyStateView::ManualOnly { track_count } => {
                BridgeIdentifyState::ManualOnly { track_count }
            }
        }
    }
}

/// Key library statuses by release id — the UI looks a row's status up by id
/// rather than re-indexing a flat list. Each status carries its own id, so this
/// is a re-container, not a re-pairing.
#[cfg(feature = "desktop")]
fn status_map(
    statuses: Vec<bae_core::db::LibraryStatus>,
) -> std::collections::HashMap<String, BridgeLibraryStatus> {
    statuses
        .into_iter()
        .map(|s| (s.release_id.clone(), BridgeLibraryStatus::from_core(s)))
        .collect()
}

/// Unzip core's paired rows into the two containers the UI reads: the ordered
/// results list (display order matters) and their statuses keyed by release id.
#[cfg(feature = "desktop")]
fn results_and_status_map(
    rows: Vec<bae_core::identify::ResultRow>,
) -> (
    Vec<BridgeMetadataResult>,
    std::collections::HashMap<String, BridgeLibraryStatus>,
) {
    let (results, statuses): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .map(|bae_core::identify::ResultRow { result, status }| {
            (BridgeMetadataResult::from_core(result), status)
        })
        .unzip();
    (results, status_map(statuses))
}

#[cfg(feature = "desktop")]
impl BridgeFileInfo {
    fn from_core(f: bae_core::import::folder_scanner::ScannedFile) -> Self {
        let bae_core::import::folder_scanner::ScannedFile {
            path,
            relative_path,
            size,
            dir_prefix,
            file_name,
        } = f;
        BridgeFileInfo {
            name: relative_path,
            size,
            dir_prefix,
            file_name,
            local_path: path.to_string_lossy().to_string(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFile {
    fn from_core(
        entry: bae_core::import::folder_scanner::CandidateFile,
        becomes: bae_core::import::folder_scanner::FileBecomes,
    ) -> Self {
        use bae_core::import::folder_scanner::{CandidateFile, FileRole, SheetBinding};

        let alternatives = entry
            .role_alternatives()
            .iter()
            .copied()
            .map(BridgeFileRoleChoice::from_core)
            .collect();
        let role_choice = entry.role_choice().map(BridgeFileRoleChoice::from_core);
        let CandidateFile {
            file,
            role,
            proposed_audio: _,
        } = entry;
        // Read the file id (relative path) and disk path back off `BridgeFileInfo`
        // so the exhaustive `ScannedFile` destructure lives only in its `from_core`.
        let file = BridgeFileInfo::from_core(file);
        let image_choice = || BridgeCoverChoice {
            selection: BridgeCoverSelection::ReleaseImage {
                file_id: file.name.clone(),
            },
            preview_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
            thumbnail_source: BridgeCoverImageSource::Local {
                path: file.local_path.clone(),
            },
        };
        let role = match role {
            FileRole::Audio => BridgeFileRole::Audio,
            // The disc assignment is the mapping table's to show, on the group
            // header that carries the picker for it. A roles row states what
            // the sheet's slots are, which already reflects the assignment.
            FileRole::TrackSheet {
                sheet,
                binding,
                disc: _,
            } => BridgeFileRole::TrackSheet {
                binding: match binding {
                    SheetBinding::Describes { file_id } => {
                        BridgeSheetBinding::Describes { file_id }
                    }
                    // Derived from the parsed sheet, like `track_count` below:
                    // the directive's text is what the pane shows a user whose
                    // sheet found nothing, and the bridge doesn't mirror the
                    // whole parse to carry it.
                    SheetBinding::Unresolved => BridgeSheetBinding::Unresolved {
                        requested: sheet
                            .audio_file_references()
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                    SheetBinding::RefusedCodec { file_id, codec } => {
                        BridgeSheetBinding::RefusedCodec { file_id, codec }
                    }
                },
                // A derived count, not a carried field — `CueSheet` is a large
                // parse product the bridge doesn't mirror.
                track_count: sheet.playable_track_count() as u32,
            },
            FileRole::Cover => BridgeFileRole::Cover {
                choice: image_choice(),
            },
            FileRole::Artwork => BridgeFileRole::Artwork {
                choice: image_choice(),
            },
            FileRole::Document => BridgeFileRole::Document,
            FileRole::Other => BridgeFileRole::Other,
        };
        BridgeCandidateFile {
            file,
            role,
            becomes: BridgeFileBecomes::from_core(becomes),
            alternatives,
            role_choice,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeFileRoleChoice {
    pub(crate) fn from_core(choice: bae_core::import::folder_scanner::FileRoleChoice) -> Self {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match choice {
            FileRoleChoice::Audio => Self::Audio,
            FileRoleChoice::NotATrack => Self::NotATrack,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::FileRoleChoice {
        use bae_core::import::folder_scanner::FileRoleChoice;
        match self {
            Self::Audio => FileRoleChoice::Audio,
            Self::NotATrack => FileRoleChoice::NotATrack,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeFileBecomes {
    fn from_core(becomes: bae_core::import::folder_scanner::FileBecomes) -> Self {
        use bae_core::import::folder_scanner::FileBecomes;
        match becomes {
            FileBecomes::Slots { first, last } => Self::Slots { first, last },
            FileBecomes::NoSlots => Self::NoSlots,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCollapsedDirectory {
    fn from_core(directory: bae_core::import::folder_scanner::CollapsedDirectory) -> Self {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let CollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = directory;
        BridgeCollapsedDirectory {
            dir_prefix,
            kind: match kind {
                FileRowKind::Document => BridgeFileRowKind::Document,
                FileRowKind::Other => BridgeFileRowKind::Other,
            },
            count,
            total_size,
        }
    }

    fn into_core(self) -> bae_core::import::folder_scanner::CollapsedDirectory {
        use bae_core::import::folder_scanner::{CollapsedDirectory, FileRowKind};
        let BridgeCollapsedDirectory {
            dir_prefix,
            kind,
            count,
            total_size,
        } = self;
        CollapsedDirectory {
            dir_prefix,
            kind: match kind {
                BridgeFileRowKind::Document => FileRowKind::Document,
                BridgeFileRowKind::Other => FileRowKind::Other,
            },
            count,
            total_size,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBindingOption {
    pub(crate) fn from_core(option: bae_core::import::folder_scanner::SheetBindingOption) -> Self {
        use bae_core::import::folder_scanner::{SheetBindingOffer, SheetBindingOption};

        let SheetBindingOption { file_id, offer } = option;
        BridgeSheetBindingOption {
            file_id,
            offer: match offer {
                SheetBindingOffer::Offered => BridgeSheetBindingOffer::Offered,
                SheetBindingOffer::RefusedCodec { codec } => {
                    BridgeSheetBindingOffer::RefusedCodec { codec }
                }
                SheetBindingOffer::RefusedUnreadable => BridgeSheetBindingOffer::RefusedUnreadable,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeCandidateFiles {
    pub(crate) fn from_core(files: bae_core::import::folder_scanner::CategorizedFiles) -> Self {
        // Both derived from the whole set before it is taken apart: which slots
        // a file backs and which directories collapse are facts about the
        // folder, not about any one file.
        let becomes = files.becomes();
        let collapsed_directories = files
            .collapsed_directories()
            .into_iter()
            .map(BridgeCollapsedDirectory::from_core)
            .collect();
        let bae_core::import::folder_scanner::CategorizedFiles {
            files,
            format_label,
        } = files;
        BridgeCandidateFiles {
            files: files
                .into_iter()
                .zip(becomes)
                .map(|(entry, becomes)| BridgeCandidateFile::from_core(entry, becomes))
                .collect(),
            format_label,
            collapsed_directories,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgePressingEdit {
    fn from_core(p: bae_core::import::PressingEdit) -> Self {
        let bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    fn into_core(self) -> bae_core::import::PressingEdit {
        let BridgePressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::PressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeAudioFile {
    fn from_core(file: bae_core::import::AudioFile) -> Self {
        match file {
            bae_core::import::AudioFile::Standalone { file_id } => Self::Standalone { file_id },
            bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }

    fn into_core(self) -> bae_core::import::AudioFile {
        match self {
            Self::Standalone { file_id } => bae_core::import::AudioFile::Standalone { file_id },
            Self::SheetSlice {
                file_id,
                sheet_id,
                index,
            } => bae_core::import::AudioFile::SheetSlice {
                file_id,
                sheet_id,
                index,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSlotReconciliation {
    fn from_core(reconciliation: bae_core::import::SlotReconciliation) -> Self {
        use bae_core::import::SlotReconciliation;
        match reconciliation {
            SlotReconciliation::Agrees { count } => Self::Agrees { count },
            SlotReconciliation::MoreFiles { files, tracks } => Self::MoreFiles { files, tracks },
            SlotReconciliation::MoreTracks { files, tracks } => Self::MoreTracks { files, tracks },
        }
    }

    fn into_core(self) -> bae_core::import::SlotReconciliation {
        use bae_core::import::SlotReconciliation;
        match self {
            Self::Agrees { count } => SlotReconciliation::Agrees { count },
            Self::MoreFiles { files, tracks } => SlotReconciliation::MoreFiles { files, tracks },
            Self::MoreTracks { files, tracks } => SlotReconciliation::MoreTracks { files, tracks },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetDisc {
    fn from_core(disc: bae_core::import::folder_scanner::SheetDisc) -> Self {
        use bae_core::import::folder_scanner::SheetDisc;
        match disc {
            SheetDisc::Disc { number } => Self::Disc { number },
            SheetDisc::Ignored => Self::Ignored,
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::folder_scanner::SheetDisc {
        use bae_core::import::folder_scanner::SheetDisc;
        match self {
            Self::Disc { number } => SheetDisc::Disc { number },
            Self::Ignored => SheetDisc::Ignored,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingRole {
    fn from_core(role: bae_core::import::MappingRole) -> Self {
        use bae_core::import::MappingRole;
        match role {
            MappingRole::Audio => Self::Audio,
            MappingRole::Document => Self::Document,
            MappingRole::Other => Self::Other,
        }
    }

    fn into_core(self) -> bae_core::import::MappingRole {
        use bae_core::import::MappingRole;
        match self {
            Self::Audio => MappingRole::Audio,
            Self::Document => MappingRole::Document,
            Self::Other => MappingRole::Other,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingFile {
    fn from_core(file: bae_core::import::MappingFile) -> Self {
        let bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path,
            probed_duration_ms,
            role,
            alternatives,
            role_choice,
        } = file;
        BridgeMappingFile {
            role: BridgeMappingRole::from_core(role),
            local_path: path.to_string_lossy().to_string(),
            file_id,
            name,
            size,
            probed_duration_ms,
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::from_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::MappingFile {
        let BridgeMappingFile {
            file_id,
            name,
            size,
            local_path,
            probed_duration_ms,
            role,
            alternatives,
            role_choice,
        } = self;
        bae_core::import::MappingFile {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            probed_duration_ms,
            role: role.into_core(),
            alternatives: alternatives
                .into_iter()
                .map(BridgeFileRoleChoice::into_core)
                .collect(),
            role_choice: role_choice.map(BridgeFileRoleChoice::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingEntry {
    fn from_core(entry: bae_core::import::MappingEntry) -> Self {
        let bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path,
        } = entry;
        BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path: container_path.to_string_lossy().to_string(),
        }
    }

    fn into_core(self) -> bae_core::import::MappingEntry {
        let BridgeMappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_local_path,
        } = self;
        bae_core::import::MappingEntry {
            sheet_id,
            index,
            number,
            title,
            duration_ms,
            container_id,
            container_name,
            container_path: std::path::PathBuf::from(container_local_path),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingSource {
    fn from_core(source: bae_core::import::MappingSource) -> Self {
        use bae_core::import::MappingSource;
        match source {
            MappingSource::File(file) => Self::File {
                file: BridgeMappingFile::from_core(file),
            },
            MappingSource::SheetEntry(entry) => Self::SheetEntry {
                entry: BridgeMappingEntry::from_core(entry),
            },
            MappingSource::Missing => Self::Missing,
        }
    }

    fn into_core(self) -> bae_core::import::MappingSource {
        use bae_core::import::MappingSource;
        match self {
            Self::File { file } => MappingSource::File(file.into_core()),
            Self::SheetEntry { entry } => MappingSource::SheetEntry(entry.into_core()),
            Self::Missing => MappingSource::Missing,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingBecomes {
    fn from_core(becomes: bae_core::import::MappingBecomes) -> Self {
        use bae_core::import::MappingBecomes;
        match becomes {
            MappingBecomes::Track {
                track,
                source_position,
                source_duration_ms,
            } => Self::Track {
                track: BridgeRawTrackEdit::from_core(track),
                source_position,
                source_duration_ms,
            },
            MappingBecomes::Kept => Self::Kept,
            MappingBecomes::AwaitingPick => Self::AwaitingPick,
        }
    }

    fn into_core(self) -> bae_core::import::MappingBecomes {
        use bae_core::import::MappingBecomes;
        match self {
            Self::Track {
                track,
                source_position,
                source_duration_ms,
            } => MappingBecomes::Track {
                track: track.into_core(),
                source_position,
                source_duration_ms,
            },
            Self::Kept => MappingBecomes::Kept,
            Self::AwaitingPick => MappingBecomes::AwaitingPick,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingUnit {
    fn from_core(unit: bae_core::import::MappingUnit) -> Self {
        let bae_core::import::MappingUnit { source, becomes } = unit;
        BridgeMappingUnit {
            source: BridgeMappingSource::from_core(source),
            becomes: BridgeMappingBecomes::from_core(becomes),
        }
    }

    fn into_core(self) -> bae_core::import::MappingUnit {
        let BridgeMappingUnit { source, becomes } = self;
        bae_core::import::MappingUnit {
            source: source.into_core(),
            becomes: becomes.into_core(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingContainer {
    fn from_core(container: bae_core::import::MappingContainer) -> Self {
        let bae_core::import::MappingContainer {
            file_id,
            name,
            size,
        } = container;
        BridgeMappingContainer {
            file_id,
            name,
            size,
        }
    }

    fn into_core(self) -> bae_core::import::MappingContainer {
        let BridgeMappingContainer {
            file_id,
            name,
            size,
        } = self;
        bae_core::import::MappingContainer {
            file_id,
            name,
            size,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetGroup {
    fn from_core(sheet: bae_core::import::SheetGroup) -> Self {
        let bae_core::import::SheetGroup {
            sheet_id,
            name,
            path,
            bound,
            assignment,
            disc_options,
        } = sheet;
        BridgeSheetGroup {
            sheet_id,
            name,
            local_path: path.to_string_lossy().into_owned(),
            bound: BridgeSheetBound::from_core(bound),
            assignment: BridgeSheetDisc::from_core(assignment),
            disc_options,
        }
    }

    fn into_core(self) -> bae_core::import::SheetGroup {
        let BridgeSheetGroup {
            sheet_id,
            name,
            local_path,
            bound,
            assignment,
            disc_options,
        } = self;
        bae_core::import::SheetGroup {
            sheet_id,
            name,
            path: std::path::PathBuf::from(local_path),
            bound: bound.into_core(),
            assignment: assignment.into_core(),
            disc_options,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeSheetBound {
    fn from_core(bound: bae_core::import::SheetBound) -> Self {
        use bae_core::import::SheetBound;
        match bound {
            SheetBound::Describes(container) => Self::Describes {
                container: BridgeMappingContainer::from_core(container),
            },
            SheetBound::Unresolved { requested } => Self::Unresolved { requested },
            SheetBound::RefusedCodec { container, codec } => Self::RefusedCodec {
                container: BridgeMappingContainer::from_core(container),
                codec,
            },
        }
    }

    fn into_core(self) -> bae_core::import::SheetBound {
        use bae_core::import::SheetBound;
        match self {
            Self::Describes { container } => SheetBound::Describes(container.into_core()),
            Self::Unresolved { requested } => SheetBound::Unresolved { requested },
            Self::RefusedCodec { container, codec } => SheetBound::RefusedCodec {
                container: container.into_core(),
                codec,
            },
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingImage {
    fn from_core(image: bae_core::import::MappingImage) -> Self {
        let bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path,
            is_cover,
        } = image;
        BridgeMappingImage {
            file_id,
            name,
            size,
            local_path: path.to_string_lossy().to_string(),
            is_cover,
        }
    }

    fn into_core(self) -> bae_core::import::MappingImage {
        let BridgeMappingImage {
            file_id,
            name,
            size,
            local_path,
            is_cover,
        } = self;
        bae_core::import::MappingImage {
            file_id,
            name,
            size,
            path: std::path::PathBuf::from(local_path),
            is_cover,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingRow {
    fn from_core(row: bae_core::import::MappingRow) -> Self {
        use bae_core::import::MappingRow;
        match row {
            MappingRow::Unit(unit) => Self::Unit {
                unit: BridgeMappingUnit::from_core(unit),
            },
            MappingRow::Sheet { sheet, entries } => Self::Sheet {
                sheet: BridgeSheetGroup::from_core(sheet),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::from_core)
                    .collect(),
            },
            MappingRow::Images(images) => Self::Images {
                images: images
                    .into_iter()
                    .map(BridgeMappingImage::from_core)
                    .collect(),
            },
            MappingRow::Directory(directory) => Self::Directory {
                directory: BridgeCollapsedDirectory::from_core(directory),
            },
        }
    }

    fn into_core(self) -> bae_core::import::MappingRow {
        use bae_core::import::MappingRow;
        match self {
            Self::Unit { unit } => MappingRow::Unit(unit.into_core()),
            Self::Sheet { sheet, entries } => MappingRow::Sheet {
                sheet: sheet.into_core(),
                entries: entries
                    .into_iter()
                    .map(BridgeMappingUnit::into_core)
                    .collect(),
            },
            Self::Images { images } => MappingRow::Images(
                images
                    .into_iter()
                    .map(BridgeMappingImage::into_core)
                    .collect(),
            ),
            Self::Directory { directory } => MappingRow::Directory(directory.into_core()),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeMappingTable {
    pub(crate) fn from_core(table: bae_core::import::MappingTable) -> Self {
        let bae_core::import::MappingTable {
            rows,
            reconciliation,
        } = table;
        BridgeMappingTable {
            rows: rows.into_iter().map(BridgeMappingRow::from_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::MappingTable {
        let BridgeMappingTable {
            rows,
            reconciliation,
        } = self;
        bae_core::import::MappingTable {
            rows: rows.into_iter().map(BridgeMappingRow::into_core).collect(),
            reconciliation: reconciliation.map(BridgeSlotReconciliation::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeTrackUserEdit {
    fn from_core(t: bae_core::import::TrackUserEdit) -> Self {
        let bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file,
        } = t;
        Self {
            title,
            side,
            track_number,
            artist_names,
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::TrackUserEdit {
        let BridgeTrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file,
        } = self;
        bae_core::import::TrackUserEdit {
            title,
            side,
            track_number,
            artist_names,
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeReleaseUserEdit {
    pub(crate) fn from_core(e: bae_core::import::ReleaseUserEdit) -> Self {
        let bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing,
            tracks,
        } = e;
        BridgeReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing: BridgePressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::ReleaseUserEdit {
        let BridgeReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing,
            tracks,
        } = self;
        bae_core::import::ReleaseUserEdit {
            album_title,
            album_artist_names,
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeTrackUserEdit::into_core)
                .collect(),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawPressingEdit {
    fn from_core(p: bae_core::import::RawPressingEdit) -> Self {
        let bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = p;
        Self {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }

    fn into_core(self) -> bae_core::import::RawPressingEdit {
        let BridgeRawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        } = self;
        bae_core::import::RawPressingEdit {
            year,
            format,
            label,
            catalog_number,
            country,
            barcode,
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawTrackEdit {
    fn from_core(t: bae_core::import::RawTrackEdit) -> Self {
        let bae_core::import::RawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file,
        } = t;
        Self {
            id,
            title,
            artist_text,
            side,
            track_number,
            file: file.map(BridgeAudioFile::from_core),
        }
    }

    fn into_core(self) -> bae_core::import::RawTrackEdit {
        let BridgeRawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file,
        } = self;
        bae_core::import::RawTrackEdit {
            id,
            title,
            artist_text,
            side,
            track_number,
            file: file.map(BridgeAudioFile::into_core),
        }
    }
}

#[cfg(feature = "desktop")]
impl BridgeRawReleaseEdit {
    pub(crate) fn from_core(e: bae_core::import::RawReleaseEdit) -> Self {
        let bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_text,
            pressing,
            tracks,
        } = e;
        BridgeRawReleaseEdit {
            album_title,
            album_artist_text,
            pressing: BridgeRawPressingEdit::from_core(pressing),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::from_core)
                .collect(),
        }
    }

    pub(crate) fn into_core(self) -> bae_core::import::RawReleaseEdit {
        let BridgeRawReleaseEdit {
            album_title,
            album_artist_text,
            pressing,
            tracks,
        } = self;
        bae_core::import::RawReleaseEdit {
            album_title,
            album_artist_text,
            pressing: pressing.into_core(),
            tracks: tracks
                .into_iter()
                .map(BridgeRawTrackEdit::into_core)
                .collect(),
        }
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
        "core.queue.output",
        "core.queue.failed",
        "core.queue.queued",
        "core.download.bytes_progress",
        "core.outbox.pending_deletes",
        "core.outbox.bytes_progress",
        "core.outbox.throughput",
        "core.outbox.eta",
        // Album total playing time: the UI switches on `BridgeDurationUnits` and
        // composes the hours and minutes words through the join pattern.
        "core.duration.hours",
        "core.duration.minutes",
        "core.duration.hours_minutes",
        // Release-group card pressing count.
        "core.import.pressings",
        // Disconnect-sync confirmation: releases that live only in the cloud (the
        // UI composes the count into its own base sentence).
        "core.sync.cloud_only_releases",
        // Generic lookup-failure line for the keyless `Diagnostic` variant:
        // `bridge_lookup_failure_key` returns `None`, the UI shows this line.
        "core.lookup.failure.diagnostic",
    ];

    /// A stand-in cover choice for walking the file roles that carry one. The
    /// key a role reads under never looks at it.
    fn loc_cover_choice() -> BridgeCoverChoice {
        BridgeCoverChoice {
            selection: BridgeCoverSelection::ReleaseImage {
                file_id: String::new(),
            },
            preview_source: BridgeCoverImageSource::Local {
                path: String::new(),
            },
            thumbnail_source: BridgeCoverImageSource::Local {
                path: String::new(),
            },
        }
    }

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

        // bridge_sheet_refused_codec_key — one key, no variants to walk.
        keys.push(bridge_sheet_refused_codec_key());
        keys.push(bridge_sheet_refused_unreadable_key());

        // bridge_file_role_key — every role the scan can propose has a name.
        for role in [
            BridgeFileRole::Audio,
            BridgeFileRole::TrackSheet {
                binding: BridgeSheetBinding::Unresolved {
                    requested: Vec::new(),
                },
                track_count: 0,
            },
            BridgeFileRole::Cover {
                choice: loc_cover_choice(),
            },
            BridgeFileRole::Artwork {
                choice: loc_cover_choice(),
            },
            BridgeFileRole::Document,
            BridgeFileRole::Other,
        ] {
            let expected = match role {
                BridgeFileRole::Audio => "core.import.role.audio",
                BridgeFileRole::TrackSheet { .. } => "core.import.role.track_sheet",
                BridgeFileRole::Cover { .. } => "core.import.role.cover",
                BridgeFileRole::Artwork { .. } => "core.import.role.artwork",
                BridgeFileRole::Document => "core.import.role.document",
                BridgeFileRole::Other => "core.import.role.other",
            };
            assert_eq!(bridge_file_role_key(&role), expected);
            keys.push(expected.to_string());
        }

        // bridge_file_role_choice_key — the roles a person can pick between.
        for choice in [BridgeFileRoleChoice::Audio, BridgeFileRoleChoice::NotATrack] {
            let expected = match choice {
                // Deliberately the same key the Audio role reads under: the
                // picker's option and the column's label name one thing.
                BridgeFileRoleChoice::Audio => "core.import.role.audio",
                BridgeFileRoleChoice::NotATrack => "core.import.role.not_a_track",
            };
            assert_eq!(bridge_file_role_choice_key(choice), expected);
            keys.push(expected.to_string());
        }

        // bridge_file_becomes_key — one slot, a run of slots, or none. The
        // single-slot case has its own key because "slot 12" and "slots 1-11"
        // are different sentences, not one sentence with a range in it.
        for becomes in [
            BridgeFileBecomes::Slots { first: 3, last: 3 },
            BridgeFileBecomes::Slots { first: 1, last: 11 },
            BridgeFileBecomes::NoSlots,
        ] {
            let expected = match becomes {
                BridgeFileBecomes::Slots { first, last } if first == last => {
                    "core.import.becomes.slot"
                }
                BridgeFileBecomes::Slots { .. } => "core.import.becomes.slots",
                BridgeFileBecomes::NoSlots => "core.import.becomes.not_a_track",
            };
            assert_eq!(bridge_file_becomes_key(becomes), expected);
            keys.push(expected.to_string());
        }

        // bridge_file_row_kind_key — what a collapsed directory holds.
        for kind in [BridgeFileRowKind::Document, BridgeFileRowKind::Other] {
            let expected = match kind {
                BridgeFileRowKind::Document => "core.import.files.documents",
                BridgeFileRowKind::Other => "core.import.files.other",
            };
            assert_eq!(bridge_file_row_kind_key(kind), expected);
            keys.push(expected.to_string());
        }

        // bridge_slot_reconciliation_key — the tally above the slot table.
        for reconciliation in [
            BridgeSlotReconciliation::Agrees { count: 12 },
            BridgeSlotReconciliation::MoreFiles {
                files: 13,
                tracks: 12,
            },
            BridgeSlotReconciliation::MoreTracks {
                files: 11,
                tracks: 12,
            },
        ] {
            let expected = match reconciliation {
                BridgeSlotReconciliation::Agrees { .. } => "core.import.reconciliation.agrees",
                BridgeSlotReconciliation::MoreFiles { .. } => {
                    "core.import.reconciliation.more_files"
                }
                BridgeSlotReconciliation::MoreTracks { .. } => {
                    "core.import.reconciliation.more_tracks"
                }
            };
            assert_eq!(bridge_slot_reconciliation_key(reconciliation), expected);
            keys.push(expected.to_string());
        }

        // bridge_sheet_binding_offer_key — an offered file needs no reason.
        for o in [
            BridgeSheetBindingOffer::Offered,
            BridgeSheetBindingOffer::RefusedCodec {
                codec: String::new(),
            },
            BridgeSheetBindingOffer::RefusedUnreadable,
        ] {
            let expected: Option<&str> = match o {
                BridgeSheetBindingOffer::Offered => None,
                BridgeSheetBindingOffer::RefusedCodec { .. } => {
                    Some("core.import.sheet.refused_codec")
                }
                BridgeSheetBindingOffer::RefusedUnreadable => {
                    Some("core.import.sheet.refused_unreadable")
                }
            };
            assert_eq!(bridge_sheet_binding_offer_key(o).as_deref(), expected);
            if let Some(k) = expected {
                keys.push(k.to_string());
            }
        }

        // BridgeTrackSide::header_key — Flat carries no key (None). This is what
        // BridgeTrackGroup::header_key is built from at conversion.
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
            assert_eq!(s.header_key(), expected);
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
            BridgeInvalidReason::NoValidAudio,
        ] {
            let expected = match r {
                BridgeInvalidReason::CorruptAudioFile { .. } => "core.import.invalid.corrupt_audio",
                BridgeInvalidReason::CorruptImage { .. } => "core.import.invalid.corrupt_image",
                BridgeInvalidReason::NoValidAudio => "core.import.invalid.no_valid_audio",
            };
            assert_eq!(bridge_invalid_reason_key(r.clone()), expected);
            keys.push(expected.to_string());
        }

        // bridge_needs_you_key — every variant carries a key.
        for needs_you in [
            BridgeNeedsYou::AlreadyInLibrary,
            BridgeNeedsYou::SeveralMatches { count: 0 },
            BridgeNeedsYou::SignalsConflict,
            BridgeNeedsYou::NoMatch,
            BridgeNeedsYou::NothingToLookUp,
            BridgeNeedsYou::TrackCountDisagrees {
                local: 0,
                source: 0,
            },
            BridgeNeedsYou::DurationsDisagree {
                probed_ms: 0,
                source_ms: 0,
                tolerance_ms: 0,
            },
            BridgeNeedsYou::SourceLengthsUnknown,
            BridgeNeedsYou::LocalDurationUnknown,
        ] {
            let expected = match needs_you {
                BridgeNeedsYou::AlreadyInLibrary => "core.import.triage.already_in_library",
                BridgeNeedsYou::SeveralMatches { .. } => "core.import.triage.several_matches",
                BridgeNeedsYou::SignalsConflict => "core.import.triage.signals_conflict",
                BridgeNeedsYou::NoMatch => "core.import.triage.no_match",
                BridgeNeedsYou::NothingToLookUp => "core.import.triage.nothing_to_look_up",
                BridgeNeedsYou::TrackCountDisagrees { .. } => {
                    "core.import.triage.track_count_disagrees"
                }
                BridgeNeedsYou::DurationsDisagree { .. } => "core.import.triage.durations_disagree",
                BridgeNeedsYou::SourceLengthsUnknown => "core.import.triage.source_lengths_unknown",
                BridgeNeedsYou::LocalDurationUnknown => "core.import.triage.local_duration_unknown",
            };
            assert_eq!(bridge_needs_you_key(&needs_you), expected);
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
            BridgeErrorCategory::Save,
            BridgeErrorCategory::Credentials,
            BridgeErrorCategory::Network,
            BridgeErrorCategory::Keyring,
            BridgeErrorCategory::Membership,
            BridgeErrorCategory::AirPlayUnsupported,
        ] {
            let expected = match c {
                BridgeErrorCategory::Database => "core.error.category.database",
                BridgeErrorCategory::Config => "core.error.category.config",
                BridgeErrorCategory::Internal => "core.error.category.internal",
                BridgeErrorCategory::Import => "core.error.category.import",
                BridgeErrorCategory::Export => "core.error.category.export",
                BridgeErrorCategory::Save => "core.error.category.save",
                BridgeErrorCategory::Credentials => "core.error.category.credentials",
                BridgeErrorCategory::Network => "core.error.category.network",
                BridgeErrorCategory::Keyring => "core.error.category.keyring",
                BridgeErrorCategory::Membership => "core.error.category.membership",
                BridgeErrorCategory::AirPlayUnsupported => {
                    "core.error.category.airplay_unsupported"
                }
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

        // bridge_error_line_key — Cancelled carries no line (None); the other two
        // agree with the per-part key fns above, so an error has exactly one line
        // and it is not re-derived anywhere. The keys themselves are already
        // pushed by those loops, so nothing is added here.
        for e in [
            BridgeError::Cancelled,
            BridgeError::NotFound {
                entity: BridgeEntityKind::Album,
                id: "a".to_string(),
            },
            BridgeError::internal(""),
        ] {
            let expected: Option<String> = match &e {
                BridgeError::Cancelled => None,
                BridgeError::NotFound { entity, .. } => Some(bridge_entity_not_found_key(*entity)),
                BridgeError::Diagnostic { category, .. } => {
                    Some(bridge_error_category_key(*category))
                }
            };
            assert_eq!(bridge_error_line_key(&e), expected);
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

#[cfg(all(test, feature = "desktop"))]
mod triage_tests {
    use super::*;

    /// The stacking order the UIs iterate is core's, spelled once on this side
    /// so mobile builds carry it too. This is what keeps the two spellings from
    /// drifting — reorder either and it fails.
    #[test]
    fn group_order_mirrors_core() {
        let core: Vec<BridgeNeedsYouGroup> = bae_core::import::NeedsYouGroup::IN_ORDER
            .iter()
            .map(|group| BridgeNeedsYouGroup::from_core(*group))
            .collect();
        assert_eq!(bridge_needs_you_groups_in_order(), core);
    }

    /// A placement's tab is the one core's own projection gives it, for every
    /// variant — so `bridge_triage_tab` cannot become a second, divergent
    /// rule.
    #[test]
    fn tab_of_placement_mirrors_core() {
        use bae_core::import::{NeedsYouGroup, NeedsYouReason, TriagePlacement, TriageTab};
        for core in [
            TriagePlacement::Ready,
            TriagePlacement::NeedsYou {
                group: NeedsYouGroup::StillIdentifying,
                reason: NeedsYouReason::StillIdentifying {
                    phase: bae_core::import::IdentifyPhase::Queued,
                },
            },
            TriagePlacement::Importing,
            TriagePlacement::Done,
            TriagePlacement::Skipped,
        ] {
            let expected = match core.tab() {
                TriageTab::Ready => BridgeTriageTab::Ready,
                TriageTab::NeedsYou => BridgeTriageTab::NeedsYou,
                TriageTab::Done => BridgeTriageTab::Done,
                TriageTab::Skipped => BridgeTriageTab::Skipped,
            };
            let bridge = BridgeTriagePlacement::from_core(core);
            assert_eq!(bridge_triage_tab(&bridge), expected);
        }
    }
}

#[cfg(all(test, feature = "desktop"))]
mod identify_progress_tests {
    use super::*;

    #[test]
    fn barcode_progress_failure_crosses_bridge() {
        let progress = bae_core::identify::BarcodeProgress::Failed {
            failure: bae_core::signals::LookupFailure::Diagnostic {
                detail: "provider lookup failed".to_string(),
            },
        };

        let view = bae_core::identify::BarcodeProgressView::from(progress);
        match BridgeBarcodeProgress::from_view(view) {
            BridgeBarcodeProgress::Failed {
                failure: BridgeLookupFailure::Diagnostic { detail },
            } => assert_eq!(detail, "provider lookup failed"),
            other => panic!("expected failed barcode progress, got {other:?}"),
        }
    }
}

/// Round-trips a fully-populated sample through `from_core` then `into_core` and
/// asserts equality with the original. The one bug the exhaustive-destructure
/// compile checks can't catch is a transposed same-typed field introduced during
/// a rewrite; these catch it for both directions in one assertion (types without
/// `PartialEq` compare their `Debug` forms). Placeholder names only.
#[cfg(test)]
mod conversion_roundtrip {
    use super::*;

    #[test]
    fn image_ref_round_trips() {
        let core = bae_core::album_detail::ImageRef {
            id: "rel-123".to_string(),
            version: "v1".to_string(),
            image_type: bae_core::db::LibraryImageType::Artist,
        };
        assert_eq!(core, BridgeImageRef::from_core(core.clone()).into_core());
    }

    #[test]
    fn export_preset_round_trips_and_re_derives_extension() {
        let core = bae_core::config::SavePreset {
            id: "preset-1".to_string(),
            name: "Preset One".to_string(),
            codec: bae_core::config::SaveCodec::Flac {
                bit_depth: bae_core::config::SaveBitDepth::Bits24,
            },
            filename_tokens: vec![
                bae_core::config::SaveFilenameToken::Artist,
                bae_core::config::SaveFilenameToken::Title,
            ],
            pregap_placement: bae_core::config::SavePregapPlacement::Exclude,
            applies_to_track: true,
            applies_to_release: false,
            embed_cover: false,
        };
        let bridge = BridgeSavePreset::from_core(&core);
        // `extension` is derived from the codec, not carried in the core preset.
        assert_eq!(bridge.extension, core.codec.extension());
        assert!(!bridge.embed_cover);
        assert_eq!(core, bridge.into_core());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn release_user_edit_round_trips() {
        let core = bae_core::import::ReleaseUserEdit {
            album_title: "Album Title".to_string(),
            album_artist_names: vec!["Artist Name".to_string(), "Second Artist".to_string()],
            pressing: bae_core::import::PressingEdit {
                year: Some(1990),
                format: Some("CD".to_string()),
                label: Some("Label Name".to_string()),
                catalog_number: Some("CAT-1".to_string()),
                country: Some("US".to_string()),
                barcode: Some("012345678905".to_string()),
            },
            tracks: vec![bae_core::import::TrackUserEdit {
                title: "Track Title".to_string(),
                side: 1,
                track_number: Some(1),
                artist_names: vec!["Track Artist".to_string()],
                file: Some(bae_core::import::AudioFile::SheetSlice {
                    file_id: "CDImage.flac".to_string(),
                    sheet_id: "CDImage.cue".to_string(),
                    index: 0,
                }),
            }],
        };
        assert_eq!(
            core,
            BridgeReleaseUserEdit::from_core(core.clone()).into_core()
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn raw_release_edit_round_trips() {
        let core = bae_core::import::RawReleaseEdit {
            album_title: "Album Title".to_string(),
            album_artist_text: "Artist Name, Second Artist".to_string(),
            pressing: bae_core::import::RawPressingEdit {
                year: "1990".to_string(),
                format: "CD".to_string(),
                label: "Label Name".to_string(),
                catalog_number: "CAT-1".to_string(),
                country: "US".to_string(),
                barcode: "012345678905".to_string(),
            },
            tracks: vec![bae_core::import::RawTrackEdit {
                id: "row-1".to_string(),
                title: "Track Title".to_string(),
                artist_text: "Track Artist".to_string(),
                side: 1,
                track_number: Some(1),
                // The audio binding is not a form field, so it has to survive
                // the editor's round trip untouched or a corrected pairing is
                // lost between the slot table and the commit.
                file: Some(bae_core::import::AudioFile::Standalone {
                    file_id: "01.flac".to_string(),
                }),
            }],
        };
        assert_eq!(
            core,
            BridgeRawReleaseEdit::from_core(core.clone()).into_core()
        );
    }

    /// The detail crosses the bridge outbound only — it is the picker's display
    /// shape, never a seed — so this pins the derived fields and the carried ones,
    /// not a round trip.
    #[cfg(feature = "desktop")]
    #[test]
    fn release_detail_derives_default_cover() {
        let core = bae_core::import::search::ImportSearchReleaseDetail {
            release_id: "rel-123".to_string(),
            source: bae_core::import::MetadataSource::MusicBrainz,
            source_group_id: Some("rg-1".to_string()),
            title: "Album Title".to_string(),
            artist: Some("Artist Name".to_string()),
            year: Some(1990),
            format: Some("CD".to_string()),
            label: Some("Label Name".to_string()),
            catalog_number: Some("CAT-1".to_string()),
            country: Some("US".to_string()),
            barcode: Some("012345678905".to_string()),
            track_count: 10,
            tracks: vec![bae_core::import::search::ReleaseTrack {
                title: "Track Title".to_string(),
                artist: Some("Track Artist".to_string()),
                duration_ms: Some(210_000),
                position: "A1".to_string(),
                side: 1,
            }],
            cover_art: vec![bae_core::import::cover_art::RemoteCover {
                url: "https://example.test/cover.jpg".to_string(),
                thumbnail_url: "https://example.test/thumb.jpg".to_string(),
                label: "Front".to_string(),
                source: bae_core::import::MetadataSource::MusicBrainz,
            }],
        };
        // `default_cover` is derived from the first cover.
        let bridge = BridgeReleaseDetail::from_core(core.clone());
        assert!(bridge.default_cover.is_some());
        assert_eq!(bridge.release_id, core.release_id);
        assert_eq!(bridge.track_count, core.track_count);
        assert_eq!(bridge.tracks.len(), core.tracks.len());
        assert_eq!(bridge.tracks[0].title, core.tracks[0].title);
        assert_eq!(bridge.tracks[0].position, core.tracks[0].position);
        assert_eq!(bridge.cover_art.len(), core.cover_art.len());
        assert_eq!(bridge.barcode, core.barcode);
    }
}
