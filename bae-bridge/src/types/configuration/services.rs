use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    pub library_id: String,
    pub library_path: String,
    pub pause_between_sides: bool,
    /// Whether a side or disc pause ends on its own after a countdown, and how
    /// long. Settings show it only while `pause_between_sides` is on.
    pub side_pause_countdown: BridgeSidePauseCountdown,
    /// How many file uploads run at once. Device-local; range 1..=8. Desktop
    /// exposes a control for it, mobile does not (mobile makes no uploads).
    pub max_concurrent_uploads: u32,
    /// How many file downloads a pin fetches at once. Device-local; range 1..=8.
    pub max_concurrent_downloads: u32,
    /// Whether a candidate found while this is on is identified on its own.
    /// Changing it affects no candidate already found or queued.
    pub identify_automatically: bool,
    /// Whether what an identification run settles on as needing nothing from
    /// anyone is imported straight away, going where `import_storage` says —
    /// whether the run started on its own or a person asked for it. Independent
    /// of `identify_automatically`.
    pub import_when_identified: bool,
    /// The import storage choice, which core reads as each import starts.
    pub import_storage: BridgeImportStorage,
    /// Which catalogs Find online asks, one entry per catalog in core's
    /// order — what the switches on the Find online header and in Settings
    /// render, those being two views of one setting. Core folds the person's
    /// choice and the catalog's credentials into a single answer, so no surface
    /// re-derives "on and reachable" from a flag plus `discogs_usable`.
    pub lookup_catalogs: Vec<BridgeLookupCatalogSetting>,
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
    /// What a track a Cast or UPnP device can't be served as stored is
    /// converted to. A UPnP device gets MP3 whatever this says; the setting's
    /// help says so.
    pub cast_transcode_format: BridgeCastTranscodeFormat,
    pub mcp: BridgeMcpConfig,
    pub subsonic: BridgeSubsonicConfig,
    pub discogs_token_status: BridgeDiscogsTokenStatus,
    /// Whether Discogs can be used as a metadata source (a stored key that
    /// isn't rejected). Core decides the policy via `DiscogsTokenStatus::
    /// is_usable`; the UI reads this flag rather than re-deriving it from the
    /// status.
    pub discogs_usable: bool,
}

/// How long a side or disc pause waits before the next side starts on its
/// own — exactly the choices Settings offer. `Off` waits for Play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSidePauseCountdown {
    Off,
    Seconds5,
    Seconds15,
    Seconds30,
    Seconds45,
    Seconds60,
}

mirror_enum! {
    BridgeSidePauseCountdown = bae_core::config::SidePauseCountdown,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Off, Seconds5, Seconds15, Seconds30, Seconds45, Seconds60 },
}

/// What bae converts a track to for a Cast or UPnP device that can't be
/// served it as stored — exactly the choices Settings offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCastTranscodeFormat {
    /// MP3 at 320 kbps.
    Mp3,
    /// Uncompressed PCM at the track's stored depth: lossless.
    Wav,
}

mirror_enum! {
    BridgeCastTranscodeFormat = bae_core::config::CastTranscodeFormat,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Mp3, Wav },
}

/// The import storage choice. Mirrors `bae_core::config::ImportStoragePreferences`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeImportStorage {
    /// Whether imports go to the cloud home, when the library has one.
    pub cloud: bool,
    /// Whether a release that goes to the cloud stays downloaded here.
    pub pinned: bool,
}

/// Whether Find online asks one catalog, and when it does not, why not. Mirrors
/// `bae_core::import::SourceAvailability`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeSourceAvailability {
    /// Asked. Its switch is on.
    On,
    /// Switched off. Its switch is off, and turning it back on is all it takes.
    Off,
    /// The source needs a credential this library does not hold, so it cannot
    /// be asked whatever the switch says. Its switch renders off and cannot be
    /// moved; supplying the credential brings back the choice last made.
    NotConfigured,
}

/// One catalog bae asks and whether this library asks it — one switch, as a
/// surface draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeLookupCatalogSetting {
    pub catalog: BridgeCatalog,
    pub availability: BridgeSourceAvailability,
    /// Whether this switch can be moved at all. Core's answer, covering both
    /// reasons it cannot: the catalog needs a credential this library does not
    /// hold, which a switch cannot supply, and it is the only catalog still
    /// being asked, which core refuses to leave nothing behind. A surface
    /// greys the switch out on this rather than working the two cases out for
    /// itself, so it is disabled on exactly the writes core would turn down.
    pub can_change: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStoreKeyState {
    Available,
    Locked,
}

impl From<coven::StoreKeyState> for BridgeStoreKeyState {
    fn from(value: coven::StoreKeyState) -> Self {
        match value {
            coven::StoreKeyState::Available => Self::Available,
            coven::StoreKeyState::Locked => Self::Locked,
        }
    }
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

mirror_enum! {
    #[cfg(feature = "cast")]
    BridgeRendererKind = bae_cast::renderer::RendererKind,
    from_core: fn,
    variants: { Cast, Dlna, AirPlay },
}

/// The conversion is only used by the cast-gated `get_cast_devices` handle fn.
/// Not a `mirror_struct`: `kind` is bae-cast's own reading of the device rather
/// than a field it carries.
#[cfg(feature = "cast")]
impl BridgeCastDevice {
    pub(crate) fn from_core(device: bae_cast::renderer::RendererDevice) -> Self {
        let kind = BridgeRendererKind::from_core(device.kind());
        Self {
            id: device.id,
            name: device.name,
            kind,
        }
    }
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
/// decided in bae-cast.
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

mirror_enum! {
    #[cfg(feature = "cast")]
    BridgeRendererServiceType = bae_cast::renderer::RendererServiceType,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { GoogleCast, AirPlay, Raop },
}

#[cfg(feature = "cast")]
impl BridgeRendererService {
    pub(crate) fn from_core(service_type: bae_cast::renderer::RendererServiceType) -> Self {
        Self {
            service_type: BridgeRendererServiceType::from_core(service_type),
            dns_sd_type: service_type.dns_sd_type().to_string(),
        }
    }
}

mirror_struct! {
    #[cfg(feature = "cast")]
    BridgeReportedRenderer = bae_cast::renderer::ReportedRenderer,
    into_core: pub(crate) fn,
    fields: {
        service_type: (BridgeRendererServiceType),
        instance_name,
        addr,
        port,
        txt,
    },
}

/// Fields entered for an S3 store; provider credentials remain in coven custody.
#[derive(Clone, uniffi::Record)]
pub struct BridgeSaveSyncConfig {
    pub bucket: String,
    pub region: String,
    pub endpoint: Option<String>,
    /// The key prefix as the person typed it, empty for none. Slashes at its
    /// ends mean nothing; coven parses it.
    pub key_prefix: String,
    pub access_key: String,
    pub secret_key: String,
    pub profile: Option<BridgeS3Profile>,
}

impl BridgeSaveSyncConfig {
    /// A key prefix coven can't parse fails in its own class, naming the
    /// folder at fault.
    pub(crate) fn into_core(self) -> Result<bae_core::sync::S3ConfigData, BridgeError> {
        Ok(bae_core::sync::S3ConfigData {
            bucket: self.bucket,
            region: self.region,
            endpoint: self.endpoint,
            key_prefix: coven::S3Prefix::parse(&self.key_prefix)?,
            access_key: self.access_key,
            secret_key: self.secret_key,
            profile: self.profile.map(BridgeS3Profile::into_core),
        })
    }
}
