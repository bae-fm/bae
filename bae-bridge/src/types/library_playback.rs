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
    /// The service's own name, for a surface that says where a pick came from.
    ///
    /// A brand, so it is the same in every language and carries no catalog
    /// key — and the service's full name rather than a code, because the name
    /// is what the person recognises.
    pub fn name(self) -> &'static str {
        self.into_core().display_name()
    }

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

    pub(super) fn into_core(self) -> bae_core::album_detail::ReleaseStorageAction {
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
            Self::MakeRemote => "core.transfer.action.make_remote",
            Self::MakeLocal => "core.transfer.action.make_local",
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
    pub source_audio: Option<BridgeSourceAudioSummary>,
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

/// A new metadata source for a release already in the library. Mirrors
/// `bae_core::import::ReleaseReseed`.
#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseReseed {
    ExternalRelease {
        release_id: String,
        source: BridgeMetadataSource,
    },
    FileTags,
}

#[cfg(feature = "desktop")]
impl BridgeReleaseReseed {
    pub fn into_core(self) -> bae_core::import::ReleaseReseed {
        match self {
            Self::ExternalRelease { release_id, source } => {
                bae_core::import::ReleaseReseed::ExternalRelease {
                    release_ref: bae_core::import::MetadataRef::new(release_id, source.into_core()),
                }
            }
            Self::FileTags => bae_core::import::ReleaseReseed::FileTags,
        }
    }

    /// The claim core recorded for a candidate — the direction `into_core`
    /// doesn't cover, since a pick's claim is settled in core and travels
    /// outward.
    pub fn from_core(choice: bae_core::import::ReleaseReseed) -> Self {
        match choice {
            bae_core::import::ReleaseReseed::ExternalRelease { release_ref } => {
                Self::ExternalRelease {
                    release_id: release_ref.id,
                    source: BridgeMetadataSource::from_core(release_ref.source),
                }
            }
            bae_core::import::ReleaseReseed::FileTags => Self::FileTags,
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

    #[cfg(feature = "desktop")]
    pub(crate) fn into_core(self) -> bae_core::album_detail::TrackSide {
        match self {
            Self::Sided { side_letter } => bae_core::album_detail::TrackSide::Sided { side_letter },
            Self::Disc { disc } => bae_core::album_detail::TrackSide::Disc { disc },
            Self::Flat => bae_core::album_detail::TrackSide::Flat,
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
    /// Playing time of this group's tracks, in the same words as
    /// `BridgeRelease::total_duration`, or `None` when no track reports a
    /// length. Shown under each disc of a multi-disc release.
    pub total_duration: Option<BridgeDurationUnits>,
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
    /// Core-rendered position string: "A1"/"3"/"5", or the stable prefix
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
    pub(super) fn from_core(clock: bae_core::util::duration::DurationClock) -> Self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSourceAudioLayout {
    File,
    Cue,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeSourceAudioDescriptor {
    pub layout: BridgeSourceAudioLayout,
    pub format: BridgeAudioFormat,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSourceAudioSummary {
    Uniform {
        descriptor: BridgeSourceAudioDescriptor,
    },
    Mixed {
        descriptors: Vec<BridgeSourceAudioDescriptor>,
    },
}

impl BridgeSourceAudioLayout {
    pub(crate) fn from_core(layout: bae_core::album_detail::SourceAudioLayout) -> Self {
        match layout {
            bae_core::album_detail::SourceAudioLayout::File => Self::File,
            bae_core::album_detail::SourceAudioLayout::Cue => Self::Cue,
        }
    }
}

impl BridgeSourceAudioDescriptor {
    pub(crate) fn from_core(descriptor: bae_core::album_detail::SourceAudioDescriptor) -> Self {
        Self {
            layout: BridgeSourceAudioLayout::from_core(descriptor.layout),
            format: BridgeAudioFormat::from_core(descriptor.format),
        }
    }
}

impl BridgeSourceAudioSummary {
    pub(crate) fn from_core(summary: bae_core::album_detail::SourceAudioSummary) -> Self {
        match summary {
            bae_core::album_detail::SourceAudioSummary::Uniform { descriptor } => Self::Uniform {
                descriptor: BridgeSourceAudioDescriptor::from_core(descriptor),
            },
            bae_core::album_detail::SourceAudioSummary::Mixed { descriptors } => Self::Mixed {
                descriptors: descriptors
                    .into_iter()
                    .map(BridgeSourceAudioDescriptor::from_core)
                    .collect(),
            },
        }
    }
}

#[cfg(test)]
mod source_audio_bridge_tests {
    use super::*;

    #[test]
    fn mixed_source_audio_crosses_with_every_descriptor() {
        let format = bae_core::album_detail::AudioFormat {
            codec: "FLAC".to_string(),
            sample_rate_hz: 44_100,
            bits_per_sample: Some(16),
            bitrate_kbps: None,
            channels: 2,
        };
        let summary = bae_core::album_detail::SourceAudioSummary::Mixed {
            descriptors: vec![
                bae_core::album_detail::SourceAudioDescriptor {
                    layout: bae_core::album_detail::SourceAudioLayout::Cue,
                    format: format.clone(),
                },
                bae_core::album_detail::SourceAudioDescriptor {
                    layout: bae_core::album_detail::SourceAudioLayout::File,
                    format,
                },
            ],
        };

        let BridgeSourceAudioSummary::Mixed { descriptors } =
            BridgeSourceAudioSummary::from_core(summary)
        else {
            panic!("mixed source audio became uniform");
        };
        assert_eq!(descriptors.len(), 2);
        assert!(matches!(
            descriptors[0].layout,
            BridgeSourceAudioLayout::Cue
        ));
        assert!(matches!(
            descriptors[1].layout,
            BridgeSourceAudioLayout::File
        ));
    }
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
    Bytes { data: Vec<u8> },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverSelection {
    ReleaseImage {
        file_id: String,
    },
    RemoteCover {
        selection: BridgeRemoteCoverSelection,
    },
    EmbeddedCover {
        source_file_id: String,
    },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverChoice {
    pub selection: BridgeCoverSelection,
    pub preview_source: BridgeCoverImageSource,
    pub thumbnail_source: BridgeCoverImageSource,
}
