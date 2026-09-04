use super::super::*;

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeConfig {
    pub library_id: String,
    pub library_name: String,
    pub library_path: String,
    pub pause_between_sides: bool,
    /// How many blob uploads run at once. Device-local; range 1..=8. Desktop
    /// exposes a control for it, mobile does not (mobile makes no uploads).
    pub max_concurrent_uploads: u32,
    /// How many blob downloads a pin fetches at once. Device-local; range 1..=8.
    pub max_concurrent_downloads: u32,
    /// Whether identification starts on its own: newly discovered candidates
    /// are identified as they are found, and opening Find online for a
    /// candidate starts its identification.
    pub identify_automatically: bool,
    /// Which source is applied when an import candidate is first discovered.
    pub default_import_metadata_source: BridgeDefaultImportMetadataSource,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDefaultImportMetadataSource {
    FindOnline,
    FileTags,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCloudHomeKeyState {
    NotRequired,
    Available,
    Locked,
}

impl From<coven::CloudHomeKeyState> for BridgeCloudHomeKeyState {
    fn from(value: coven::CloudHomeKeyState) -> Self {
        match value {
            coven::CloudHomeKeyState::NotRequired => Self::NotRequired,
            coven::CloudHomeKeyState::Available => Self::Available,
            coven::CloudHomeKeyState::Locked => Self::Locked,
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
