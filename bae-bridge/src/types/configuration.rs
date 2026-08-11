use super::*;

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
