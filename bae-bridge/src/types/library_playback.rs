use super::{BridgeMediaCount, BridgePressingFacts, BridgeReleaseName, BridgeReleasePlayability};

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
#[cfg(desktop)]
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDiscogsSaveOutcome {
    Valid,
    /// The key is stored but Discogs could not be reached to check it.
    Unvalidated,
    Rejected,
}

#[cfg(desktop)]
impl BridgeDiscogsSaveOutcome {
    pub(crate) fn from_core(check: bae_catalogs::providers::DiscogsCheck) -> Self {
        use bae_catalogs::providers::DiscogsCheck;
        match check {
            DiscogsCheck::Valid => Self::Valid,
            DiscogsCheck::Offline(_) => Self::Unvalidated,
            DiscogsCheck::Rejected => Self::Rejected,
        }
    }
}

/// A service that publishes descriptions of releases. Mirrors
/// `bae_catalogs::catalog::Catalog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCatalog {
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

impl BridgeCatalog {
    /// The catalog's own name, for a surface that says where a description
    /// came from.
    ///
    /// A brand, so it is the same in every language and carries no catalog
    /// key — and the service's full name rather than a code, because the name
    /// is what the person recognises.
    pub fn name(self) -> &'static str {
        self.into_core().display_name()
    }
}

mirror_enum! {
    BridgeCatalog = bae_catalogs::catalog::Catalog,
    from_core: pub fn,
    into_core: pub fn,
    variants: {
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
    },
}

/// A catalog that answers questions — a search, a disc ID, a barcode — and
/// so the one a switch, a run's provider, or a search's part names. Mirrors
/// `bae_catalogs::catalog::LookupCatalog`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeLookupCatalog {
    MusicBrainz,
    Discogs,
}

mirror_enum! {
    BridgeLookupCatalog = bae_catalogs::catalog::LookupCatalog,
    from_core: pub fn,
    into_core: pub fn,
    variants: { MusicBrainz, Discogs },
}

/// One catalog's key for an entity, whose kind is stated by its containing field. Mirrors
/// `bae_catalogs::records::MetadataRef`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeMetadataRef {
    pub catalog: BridgeCatalog,
    pub key: String,
}

/// A release on a catalog that answers lookups: one bae can fetch and pick.
/// Mirrors `bae_catalogs::records::LookupRef`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLookupRef {
    pub catalog: BridgeLookupCatalog,
    pub key: String,
}

#[cfg(desktop)]
impl BridgeLookupRef {
    pub(crate) fn from_core(release: bae_catalogs::records::LookupRef) -> Self {
        Self { catalog: BridgeLookupCatalog::from_core(release.catalog), key: release.key }
    }

    pub(crate) fn into_core(self) -> bae_catalogs::records::LookupRef {
        bae_catalogs::records::LookupRef::new(self.catalog.into_core(), self.key)
    }
}

/// A catalog link displayed by the native UI. Core resolves whether its page
/// names a pressing or an album before projecting the URL.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseRecord {
    pub catalog: BridgeCatalog,
    pub url: String,
}

impl BridgeReleaseRecord {
    /// Native surfaces render catalog links; identity matching remains in core.
    pub(crate) fn from_core(record: bae_catalogs::records::ReleaseRecord) -> Self {
        Self { catalog: BridgeCatalog::from_core(record.catalog()), url: record.url() }
    }
}

#[cfg(desktop)]
impl BridgeMetadataRef {
    pub(crate) fn from_core(release_ref: bae_catalogs::records::MetadataRef) -> Self {
        Self { catalog: BridgeCatalog::from_core(release_ref.catalog), key: release_ref.key }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCloudProvider {
    S3,
    Dropbox,
    OneDrive,
    CloudKit,
}

/// A coven store registered on this device, with its current committed name.
/// Mirrors `LibraryInfo`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeLibrary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub state: BridgeLibraryState,
    pub is_active: bool,
}

/// Whether a library on this device opens, and where it is stored. Mirrors
/// `LibraryState`.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLibraryState {
    /// It opens: stored in its storage, or on this device only (`None`).
    Openable { storage: Option<super::BridgeStorageLocation> },
    /// Its storage settings can't be read, which keeps it from opening.
    Unreadable { error: super::BridgeError },
    /// A reset or a restore of this device's copy stopped partway; it opens
    /// once the call `by` names runs again and finishes it.
    ReplacementUnfinished { by: BridgeReplacedBy },
    /// This device is removing it, and the removal stopped partway; it never
    /// opens again, and finishing the removal ends it.
    DeletionUnfinished,
}

/// The call that began replacing this device's copy of a library, and the
/// only one of the two that finishes it: `Reset` is `reset_device`,
/// `Restore` is `restore_replacing_device` with the person's current restore
/// code. `resume_replacement` finishes either without the code. Mirrors
/// coven's `ReplacedBy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReplacedBy {
    Reset,
    Restore,
}

mirror_enum! {
    BridgeReplacedBy = coven::ReplacedBy,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Reset, Restore },
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeLibraryImageType {
    Cover,
    Artist,
}

mirror_enum! {
    BridgeLibraryImageType = bae_records::LibraryImageType,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Cover, Artist },
}

/// A reference to a curated library image (a release cover or an artist
/// portrait): the image kind, subject id, and content version. The UI passes the
/// whole ref to `fetch_library_image`, so core dispatches to the known
/// image namespace.
/// `version` is the coven file the image's row names, with the device that has
/// the file while it is on one device only, so it moves whenever the bytes do
/// and again when such a file is stored. Mirrors
/// `bae_records::ImageRef`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeImageRef {
    pub id: String,
    pub version: String,
    pub image_type: BridgeLibraryImageType,
}

mirror_struct! {
    BridgeImageRef = bae_records::ImageRef,
    from_core: pub fn,
    into_core: pub fn,
    fields: { id, version, image_type: (BridgeLibraryImageType) },
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbum {
    pub id: String,
    pub title: String,
    pub year: Option<i32>,
    pub is_compilation: bool,
    /// The album's credit line for display: each credit's printed name, then
    /// the join its source prints after it (", " where it states none)
    pub artist_names: String,
    /// All release IDs for this album, in album order
    pub release_ids: Vec<String>,
    /// The album's primary release, its lowest-ordered one: it provides the
    /// album's cover art and is the default playback target.
    /// `set_album_primary_release` orders another release first.
    pub primary_release_id: String,
    /// Reference to the album's cover (the primary release's cover), or `None`
    /// when it has no cover. The UI fetches the bytes by id and caches under
    /// `(id, version)`; the version moves when the cover changes.
    pub cover: Option<BridgeImageRef>,
}

/// One row of the album grid: an album, listed once when the grid is flat and
/// under each artist its credits show when grouped by artist.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAlbumAppearance {
    pub album: BridgeAlbum,
    /// Grouped by artist: the album's credits other than the artist whose
    /// group lists it, printed as a line; `None` when that artist is its only
    /// credit, or when the grid is flat.
    pub credited_with: Option<String>,
}

/// Where a release's files are: on the one device holding its originals, or
/// in storage, with whether this device keeps them offline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageState {
    OnOneDevice,
    InStorage { pinned: bool },
}

mirror_enum! {
    BridgeReleaseStorageState = bae_views::album_detail::ReleaseStorageState,
    from_core: pub fn,
    variants: { OnOneDevice, InStorage { pinned } },
}

/// A storage transition available from the release "Storage…" sheet.
/// Mirrors `bae_types::release_storage::ReleaseStorageAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeReleaseStorageAction {
    Upload,
    Pin,
    Unpin,
    DetachAudio,
}

impl BridgeReleaseStorageAction {
    pub fn from_core(value: bae_types::release_storage::ReleaseStorageAction) -> Self {
        use bae_types::release_storage::ReleaseStorageAction as A;
        match value {
            A::Upload => Self::Upload,
            A::DetachAudio => Self::DetachAudio,
            A::Pin => Self::Pin,
            A::Unpin => Self::Unpin,
        }
    }
}

impl BridgeReleaseStorageAction {
    fn transfer_loc_key(self) -> &'static str {
        match self {
            Self::Pin => "core.transfer.action.pin",
            Self::Unpin => "core.transfer.action.unpin",
            Self::Upload => "core.transfer.action.upload",
            Self::DetachAudio => "core.transfer.action.make_local",
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
    /// What the release is made of, each carrier with its count.
    pub media: Vec<BridgeMediaCount>,
    /// The same media worded, as [`crate::types::bridge_media_terms`] words
    /// them, so a surface draws them with no call into the bridge.
    pub media_terms: Vec<crate::types::BridgeFactTerm>,
    /// Where its files are, and whether this device keeps them offline.
    pub storage_state: BridgeReleaseStorageState,
    /// Storage transitions available right now, gated by the core on whether the library
    /// syncs, and empty while the release is `transitioning`. The Storage
    /// Manager row context menu renders these.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    /// Whether coven is still copying the release's files to this device
    /// because it was moved to the cloud to be kept here. Its Unpin action
    /// then reads Stop keeping here.
    pub keeping_here: bool,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<BridgeReleaseUpload>,
    /// Whether an upload or a transfer is under way, which offers only
    /// cancelling.
    pub transitioning: bool,
    pub file_count: i64,
    pub total_size: u64,
    /// Reference to this release's own cover (image id + version), or `None` when
    /// it has no cover. Keyed on the release id so each release renders its own
    /// art; the UI caches the bytes under `(id, version)`.
    pub cover: Option<BridgeImageRef>,
}

/// A release's files in this device's upload queue: how many wait and how
/// many are being sent now. Mirrors `ReleaseUpload`; how far and how fast
/// they go is the release's upload transfer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeReleaseUpload {
    pub progress: crate::types::BridgeUploadProgress,
    /// How many of the release's files are uploading, queued and stored,
    /// each left out at zero.
    pub summary: Vec<crate::types::BridgeTransferCount>,
    /// The name of the file whose missing original its move waits for,
    /// while none of its files is being sent.
    pub missing: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeRelease {
    pub id: String,
    pub album_id: String,
    /// What a list of the album's releases calls this one.
    pub name: BridgeReleaseName,
    pub year: Option<i32>,
    /// The labels the release is on, as a surface shows them: each name
    /// once, each catalog number once.
    pub labels: Vec<crate::types::BridgeLabelLine>,
    /// What the pressing is: where it was released, its media, status,
    /// packaging and Discogs details.
    pub facts: BridgePressingFacts,
    /// The facts' summary line — where, and on what — as
    /// [`crate::types::bridge_pressing_summary`] words it. Carried rather than
    /// asked for, so a surface draws the release with no call into the
    /// bridge, as Android's preview renderer has to.
    pub pressing_summary: Vec<crate::types::BridgeFactTerm>,
    /// The facts' media alone, worded, as the release's summary carries
    /// them.
    pub media_terms: Vec<crate::types::BridgeFactTerm>,
    /// The facts' details line, as [`crate::types::bridge_pressing_details`]
    /// words it. Carried for the same reason.
    pub pressing_details: Vec<crate::types::BridgeFactTerm>,
    /// Where its files are, and whether this device keeps them offline.
    pub storage_state: BridgeReleaseStorageState,
    /// Storage transitions available right now, gated by the core on whether the library
    /// syncs, and empty while the release is `transitioning`.
    pub storage_actions: Vec<BridgeReleaseStorageAction>,
    /// Whether coven is still copying the release's files to this device
    /// because it was moved to the cloud to be kept here. Its Unpin action
    /// then reads Stop keeping here.
    pub keeping_here: bool,
    /// What of the release waits in this device's upload queue; `None` when
    /// nothing does.
    pub upload: Option<BridgeReleaseUpload>,
    /// Whether an upload or a transfer is under way, which offers only
    /// cancelling.
    pub transitioning: bool,
    pub tracks: Vec<BridgeTrack>,
    pub track_groups: Vec<BridgeTrackGroup>,
    pub source_audio: Option<BridgeSourceAudioSummary>,
    pub image_files: Vec<BridgeFile>,
    /// Core-selected cover inputs; the gallery still contains every image.
    pub cover_files: Vec<BridgeFile>,
    /// Cover slot first (if the release has one), then every image file the
    /// release has. Each item's bytes are read through
    /// `fetch_release_image`, which takes the item's `source` and
    /// dispatches the read. Consumers render this as-is.
    pub gallery_items: Vec<BridgeGalleryItem>,
    /// Whether this device can play the release now, and why not.
    pub playability: BridgeReleasePlayability,
    /// Where the release stands in being made local, which core decides from
    /// its entry in the download queue; `None` when it is not being made
    /// local.
    pub make_local: Option<crate::types::BridgeMakeLocalStatus>,
    /// The download control's state for the release, which core decides from
    /// its entry in the download queue; `None` when there is no control to
    /// show.
    pub download_status: Option<crate::types::BridgeReleaseDownloadStatus>,
    /// Every catalog's description of this release, in the order surfaces list
    /// catalogs. Empty when no catalog describes it.
    pub records: Vec<BridgeReleaseRecord>,
    /// Total playing time across all tracks, as the words it reads in, or `None`
    /// when no track reports a length. The raw sum does not cross: with the
    /// milliseconds in hand a UI could name the total its own way, which is how
    /// the three platforms came to disagree about it.
    pub total_duration: Option<BridgeDurationUnits>,
    pub file_count: i64,
    pub total_size: u64,
    /// Reference to this release's own cover (image id + version), or `None` when
    /// it has no cover. Mirrors `BridgeReleaseSummary.cover` so a summary rebuilt
    /// from this fat payload keeps its per-release art.
    pub cover: Option<BridgeImageRef>,
}

/// A track group's side discriminant — the header the UI renders ("Side A" /
/// "Disc 2"). `Flat` means no header (single-disc digital).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeTrackSide {
    Sided { side_letter: String },
    Disc { disc: u32 },
    Flat,
}

mirror_enum! {
    BridgeTrackSide = bae_types::mediums::TrackSide,
    from_core: pub(crate) fn,
    #[cfg(desktop)]
    into_core: pub(crate) fn,
    variants: { Sided { side_letter }, Disc { disc }, Flat },
}

impl BridgeTrackSide {
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
    /// Raw track length in milliseconds. Retained alongside `duration_clock`
    /// because a consumer needs the number itself — the media session's
    /// metadata states it for a queued track.
    pub duration_ms: u64,
    /// The track length as a clock label's fields ("3:07"). Core-rendered at
    /// conversion so a track row reads a field instead of an FFI call; the UI
    /// formats the fields for its locale.
    pub duration_clock: BridgeDurationClock,
    /// Effective credit line for display, joined as for `BridgeAlbum` (the track's own
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

/// Mirror of bae-library's `DurationClock` — the fields a clock label shows, for
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

mirror_struct! {
    BridgeDurationClock = bae_util::duration::DurationClock,
    from_core: pub(super) fn,
    fields: { negative, hours, minutes, seconds },
}

impl BridgeDurationClock {
    /// The clock for a duration in milliseconds, or `None` when there is nothing
    /// to label (an absent duration, or a negative one — a gap in the data, not
    /// a short track).
    pub(crate) fn from_millis(ms: Option<i64>) -> Option<Self> {
        bae_util::duration::DurationClock::from_millis(ms).map(Self::from_core)
    }

    /// The clock for a track's length, which every track has. Pre-computed onto
    /// the static row types at conversion.
    pub(crate) fn of_length(ms: u64) -> Self {
        Self::from_core(bae_util::duration::DurationClock::of_length(ms))
    }
}

/// Mirror of bae-library's `DurationUnits` — a duration named in words ("39 min",
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

mirror_enum! {
    BridgeDurationUnits = bae_util::duration::DurationUnits,
    from_core: pub(crate) fn,
    variants: {
        HoursOnly { hours },
        MinutesOnly { minutes },
        HoursAndMinutes { hours, minutes },
    },
}

/// The clock for a duration, or `None` when there is nothing to label — an
/// absent duration, or a negative one (a gap in the data, not a short track).
#[uniffi::export]
pub fn bridge_clock(ms: Option<i64>) -> Option<BridgeDurationClock> {
    BridgeDurationClock::from_millis(ms)
}

/// The clock for a track's length ("3:07"), which every track has.
#[uniffi::export]
pub fn bridge_length_clock(ms: u64) -> BridgeDurationClock {
    BridgeDurationClock::of_length(ms)
}

/// The two clocks a seek bar shows. Mirror of bae-util's `SeekBarClocks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeSeekBarClocks {
    /// Pregap countdown before track start; elapsed or remaining afterward,
    /// per `show_remaining_time`.
    pub leading: BridgeDurationClock,
    /// The track's total length; `None` when it is not known.
    pub trailing: Option<BridgeDurationClock>,
}

mirror_struct! {
    BridgeSeekBarClocks = bae_util::duration::SeekBarClocks,
    from_core: fn,
    fields: { leading: (BridgeDurationClock), trailing: (opt BridgeDurationClock) },
}

/// The seek bar's two labels, for a position within a track.
///
/// Named apart from the record it returns: uniffi-bindgen-cs would otherwise emit
/// a C# method whose name is the type's, and Windows has no compiler here.
///
/// `show_remaining` is the user's `show_remaining_time` config, which the UI
/// reads off the config mirror and never stores itself. The pregap countdown
/// overrides it while `position_ms` is negative. A `duration_ms` of `None` is a
/// track whose length is unknown: no total, and no end countdown.
#[uniffi::export]
pub fn bridge_seek_bar(
    position_ms: i64,
    duration_ms: Option<u64>,
    show_remaining: bool,
) -> BridgeSeekBarClocks {
    BridgeSeekBarClocks::from_core(bae_util::duration::SeekBarClocks::new(
        position_ms,
        duration_ms,
        show_remaining,
    ))
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeFile {
    pub id: String,
    pub original_filename: String,
    /// The file's own name, past its folders.
    pub name: String,
    /// The folders above it in the release's folder ("Disc 1"); `None` at
    /// the folder's top.
    pub folder: Option<String>,
    pub kind: BridgeFileKind,
    pub file_size: u64,
    pub content_type: String,
    pub is_image: bool,
    /// Structured audio format; `None` for non-audio files. The UI composes the
    /// one-line descriptor from it.
    pub audio_format: Option<BridgeAudioFormat>,
}

/// What kind of file a release file is. Mirrors `FileKind`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeFileKind {
    /// Audio, with its format's name ("FLAC").
    Audio {
        format: String,
    },
    /// An image, with its format's name ("JPEG").
    Image {
        format: String,
    },
    DamagedImage,
    CueSheet,
    RipLog,
    AccurateRipReport,
    Playlist,
    Text,
    Pdf,
    Other,
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

/// A release's audio: one format — a lossy one at the release's average
/// bitrate — or the facts its files disagree on.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSourceAudioSummary {
    Uniform { descriptor: BridgeSourceAudioDescriptor },
    Mixed { differences: Vec<BridgeSourceAudioDifference> },
}

/// One fact a release's files disagree on, and its values, each once. A
/// surface lists the values with its locale's list formatter.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeSourceAudioDifference {
    Layout { layouts: Vec<BridgeSourceAudioLayout> },
    Codec { codecs: Vec<String> },
    SampleRate { sample_rates_hz: Vec<i64> },
    BitDepth { bits_per_sample: Vec<i64> },
    Channels { channels: Vec<i64> },
}

mirror_enum! {
    BridgeSourceAudioLayout = bae_records::source_audio::SourceAudioLayout,
    from_core: pub(crate) fn,
    variants: { File, Cue },
}

mirror_struct! {
    BridgeSourceAudioDescriptor = bae_records::source_audio::SourceAudioDescriptor,
    from_core: pub(crate) fn,
    fields: { layout: (BridgeSourceAudioLayout), format: (BridgeAudioFormat) },
}

mirror_enum! {
    BridgeSourceAudioSummary = bae_records::source_audio::SourceAudioSummary,
    from_core: pub(crate) fn,
    variants: {
        Uniform { descriptor: (BridgeSourceAudioDescriptor) },
        Mixed { differences: (each BridgeSourceAudioDifference) },
    },
}

mirror_enum! {
    BridgeSourceAudioDifference = bae_records::source_audio::SourceAudioDifference,
    from_core: pub(crate) fn,
    variants: {
        Layout { layouts: (each BridgeSourceAudioLayout) },
        Codec { codecs },
        SampleRate { sample_rates_hz },
        BitDepth { bits_per_sample },
        Channels { channels },
    },
}

#[cfg(test)]
mod source_audio_bridge_tests {
    use super::*;

    #[test]
    fn mixed_source_audio_crosses_with_every_difference() {
        let summary = bae_records::source_audio::SourceAudioSummary::Mixed {
            differences: vec![
                bae_records::source_audio::SourceAudioDifference::Layout {
                    layouts: vec![
                        bae_records::source_audio::SourceAudioLayout::Cue,
                        bae_records::source_audio::SourceAudioLayout::File,
                    ],
                },
                bae_records::source_audio::SourceAudioDifference::Codec {
                    codecs: vec!["FLAC".to_string(), "MP3".to_string()],
                },
            ],
        };

        let BridgeSourceAudioSummary::Mixed { differences } =
            BridgeSourceAudioSummary::from_core(summary)
        else {
            panic!("mixed source audio became uniform");
        };
        assert!(matches!(
            differences.as_slice(),
            [
                BridgeSourceAudioDifference::Layout { layouts },
                BridgeSourceAudioDifference::Codec { codecs },
            ] if matches!(
                layouts.as_slice(),
                [BridgeSourceAudioLayout::Cue, BridgeSourceAudioLayout::File]
            ) && codecs == &["FLAC", "MP3"]
        ));
    }
}

/// Mirror of bae-library's `AudioFormat`. The UI composes "FLAC · 44.1 kHz ·
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

mirror_struct! {
    BridgeAudioFormat = bae_types::audio_format::AudioFormat,
    from_core: pub(crate) fn,
    #[cfg(desktop)]
    into_core: pub(crate) fn,
    fields: { codec, sample_rate_hz, bits_per_sample, bitrate_kbps, channels },
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
/// brand-name providers the UI passes through verbatim (iCloud, Dropbox,
/// OneDrive). `None` provider means local-only. One source of these
/// keys for every platform; the UI resolves the key through its string catalog.
#[uniffi::export]
pub fn bridge_cloud_provider_label_key(provider: Option<BridgeCloudProvider>) -> Option<String> {
    match provider {
        None => Some("core.cloud.local_only".to_string()),
        Some(BridgeCloudProvider::S3) => Some("core.cloud.s3_compatible".to_string()),
        Some(
            BridgeCloudProvider::CloudKit
            | BridgeCloudProvider::Dropbox
            | BridgeCloudProvider::OneDrive,
        ) => None,
    }
}

/// Which byte source a gallery slot is read from — each variant self-contained.
/// `Cover` carries the cover's `BridgeImageRef` (id + version); `ReleaseFile`
/// carries its file id. The UI passes the whole value to
/// `fetch_release_image` and never inspects it to pick a fetch.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeGallerySource {
    Cover { image: BridgeImageRef },
    ReleaseFile { file_id: String },
}

mirror_enum! {
    BridgeGallerySource = bae_records::GallerySource,
    from_core: pub(crate) fn,
    into_core: pub fn,
    variants: {
        Cover(image: (BridgeImageRef)),
        ReleaseFile { file_id },
    },
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

mirror_enum! {
    BridgeRepeatMode = bae_playback::RepeatMode,
    from_core: pub fn,
    into_core: pub fn,
    variants: { Off, Track, Context },
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
    pub image: BridgeRemoteImageSet,
    pub source: BridgeCatalog,
}

/// A catalog image: the original, and the downscaled copies the catalog serves
/// of it. A slot hands the whole set to the image fetch with its pixel size,
/// and core reads the copy that size needs — the UI never picks one.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeRemoteImageSet {
    pub url: String,
    pub downscaled: Vec<BridgeDownscaledCopy>,
}

/// A copy whose longer side is at most `max_edge` pixels.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeDownscaledCopy {
    pub url: String,
    pub max_edge: u32,
}

mirror_struct! {
    BridgeRemoteImageSet = bae_catalogs::remote_image::RemoteImageSet,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { url, downscaled: (each BridgeDownscaledCopy) },
}

mirror_struct! {
    BridgeDownscaledCopy = bae_catalogs::remote_image::DownscaledCopy,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: { url, max_edge },
}

/// Where a cover's bytes are read from. Every source carries the identity its
/// bytes have, so the app caches what it drew under it without reading the
/// file to learn whether it changed.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverImageSource {
    /// The cover of the library release imported from the folder's files.
    Library {
        image: BridgeImageRef,
    },
    Remote {
        image: BridgeRemoteImageSet,
    },
    /// An image file of the folder, at the version the scan read.
    Local {
        file: BridgeFileVersion,
    },
    /// Tag artwork at its source file's version; `read_embedded_cover` has its bytes.
    Embedded {
        source: BridgeFileVersion,
        candidate_key: Vec<u8>,
        source_file_id: String,
    },
}

/// One version of a file on disk: its absolute path, and the size and
/// modification time (nanoseconds since the Unix epoch) that tell the bytes
/// there from whatever is written at that path later. Mirror of bae-library's
/// `FileVersion`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeFileVersion {
    /// Its path as coven's bytes (`coven::path_bytes`), which the app opens
    /// the file from.
    pub path: Vec<u8>,
    pub size: u64,
    pub modified_at_ns: i64,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeCoverSelection {
    ReleaseImage { file_id: String },
    RemoteCover { selection: BridgeRemoteCoverSelection },
    EmbeddedCover { source_file_id: String },
}

/// A cover and the one image every slot showing it draws; the slot's size
/// picks among a remote image's copies.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeCoverChoice {
    pub selection: BridgeCoverSelection,
    pub image: BridgeCoverImageSource,
}
