use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use tracing::info;

mod app_dir;
mod handle;
mod identification;
mod import_storage;
mod keyring;
mod presentation;
mod save;
mod server;

pub use app_dir::AppDir;
pub use handle::ConfigHandle;
pub use identification::{IdentificationPreferences, LookupCatalogPreferences};
pub use import_storage::ImportStoragePreferences;
pub use keyring::init_keyring;
pub use presentation::LookupCatalogSetting;
pub use save::{SaveBitDepth, SaveCodec, SaveFilenameToken, SavePregapPlacement, SavePreset};
pub use server::{
    McpConfig, SubsonicConfig, SubsonicCredential, MCP_DEFAULT_PORT, SUBSONIC_DEFAULT_PORT,
};

use coven::write_atomic;
use save::default_save_presets;

/// File transfers bae runs at once, per direction, on a fresh library. Serial
/// (1) is safe but slow; a small burst keeps a single stalled transfer from
/// holding up the rest.
pub const DEFAULT_CONCURRENT_TRANSFERS: u32 = 3;

/// The largest transfer concurrency the UI offers and the setters accept.
pub const MAX_CONCURRENT_TRANSFERS: u32 = 8;

fn default_transfer_concurrency() -> NonZeroU32 {
    NonZeroU32::new(DEFAULT_CONCURRENT_TRANSFERS).expect("DEFAULT_CONCURRENT_TRANSFERS is non-zero")
}

/// A file-transfer concurrency setting must be at least 1 — coven's drain admits
/// nothing at 0 and never completes — and at most [`MAX_CONCURRENT_TRANSFERS`],
/// the ceiling the UI offers. Returns the validated value for storage.
pub(crate) fn validate_concurrency(n: u32) -> Result<NonZeroU32, ConfigError> {
    NonZeroU32::new(n)
        .filter(|n| n.get() <= MAX_CONCURRENT_TRANSFERS)
        .ok_or_else(|| {
            ConfigError::Config(format!(
                "transfer concurrency must be between 1 and {MAX_CONCURRENT_TRANSFERS}"
            ))
        })
}

/// Widen a stored concurrency setting to the [`NonZeroUsize`] coven's builder
/// takes. Non-zero is preserved: `usize` is at least 32 bits on every platform
/// bae targets, so a `NonZeroU32` never widens to zero.
pub(crate) fn usize_bound(n: NonZeroU32) -> NonZeroUsize {
    NonZeroUsize::new(n.get() as usize).expect("a NonZeroU32 widened to usize stays non-zero")
}

pub use coven::{CloudProvider, StorageConfig};

/// The validation state of a stored Discogs API key. Only carried when a key
/// exists (`Config::discogs` is `Some`). Distinct from `DiscogsTokenStatus`:
/// this only describes a key that exists, whereas the status folds in the
/// no-key case for the UI.
///
/// - `Unvalidated` — a key is stored but Discogs hasn't confirmed it yet
///   (saved while offline or rate-limited). Used optimistically; re-checked
///   when possible.
/// - `Valid` — Discogs accepted the key.
/// - `Rejected` — Discogs returned 401 for the key. Not used until re-saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscogsValidation {
    Unvalidated,
    Valid,
    Rejected,
}

/// How loudness normalization is applied at playback.
///
/// - `Off` — no normalization; tracks play at their stored level (unity gain).
/// - `Track` — normalize each track to the target using its own loudness.
/// - `Album` — normalize whole albums to the target using album loudness, so
///   the loudness relationship between an album's tracks is preserved.
///
/// The gain is derived at playback from the stored loudness measurements and a
/// constant target; this only selects which measurement (track vs album) drives
/// it. Defaults to `Off`. Set by editing `preferences.toml`; there is no UI
/// picker yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
}

/// What bae converts a track to for a Cast or UPnP device that cannot be
/// served the track as stored: a window of a CUE image, a track whose pregap
/// lives in another file or is generated, or a codec the device doesn't
/// decode. A device flavor that doesn't take WAV gets MP3 whatever this says
/// (see `RendererFlavor::stream_format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CastTranscodeFormat {
    /// MP3 at 320 kbps.
    Mp3,
    /// Uncompressed PCM at the depth the track was stored at: lossless, and
    /// several times the data of MP3.
    Wav,
}

/// How long a side or disc pause waits before the next side starts on its
/// own — exactly the choices the settings offer. `Off` waits for Play.
///
/// Read only when `pause_between_sides` is on; the choice is kept while that
/// setting is off, so turning pausing back on brings the countdown back too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidePauseCountdown {
    Off,
    Seconds5,
    Seconds15,
    Seconds30,
    Seconds45,
    Seconds60,
}

impl SidePauseCountdown {
    /// How long the pause lasts before the next side starts, or `None` when it
    /// waits for Play.
    pub fn duration(self) -> Option<std::time::Duration> {
        let seconds = match self {
            Self::Off => return None,
            Self::Seconds5 => 5,
            Self::Seconds15 => 15,
            Self::Seconds30 => 30,
            Self::Seconds45 => 45,
            Self::Seconds60 => 60,
        };
        Some(std::time::Duration::from_secs(seconds))
    }
}

/// Whether a usable Discogs API key is configured. Folds the no-key case and
/// the validation state into the four states a UI shows, so each binding
/// doesn't re-derive the precedence.
#[derive(Serialize, Deserialize)]
pub enum DiscogsTokenStatus {
    NotConfigured,
    Valid,
    Unvalidated,
    Rejected,
}

impl DiscogsTokenStatus {
    /// Whether Discogs can be used as a metadata source. A stored key is usable
    /// optimistically unless Discogs has rejected it. The single source of truth
    /// for this policy — both the macOS and Windows UIs read a flag derived from
    /// it rather than re-deciding which states count as usable.
    pub fn is_usable(&self) -> bool {
        matches!(
            self,
            DiscogsTokenStatus::Valid | DiscogsTokenStatus::Unvalidated
        )
    }
}

impl Config {
    /// The Discogs key's state for display: not configured, or the stored key's
    /// validation state.
    pub fn discogs_token_status(&self) -> DiscogsTokenStatus {
        match self.prefs.discogs {
            None => DiscogsTokenStatus::NotConfigured,
            Some(DiscogsValidation::Valid) => DiscogsTokenStatus::Valid,
            Some(DiscogsValidation::Unvalidated) => DiscogsTokenStatus::Unvalidated,
            Some(DiscogsValidation::Rejected) => DiscogsTokenStatus::Rejected,
        }
    }

    /// Which metadata sources this library asks, one entry per
    /// [`Catalog`](bae_catalogs::catalog::Catalog).
    ///
    /// The one answer every place that asks the sources together reads — a
    /// run's provider list, a typed search's per-source parts, the switches a
    /// surface renders. Two things decide it and they are not the same
    /// question: whether this library holds what the source needs, and whether
    /// the person wants it asked. Unreachable wins, so a source with no
    /// credential reports `NotConfigured` rather than `Off`, and the surface
    /// says which of the two to fix.
    ///
    /// A fact about the stored config and nothing else, which is why it lives
    /// here: the bridge builds the list a surface renders out of the same
    /// value it reads every other setting off.
    pub fn metadata_sources(&self) -> Vec<crate::import::CatalogAvailability> {
        bae_catalogs::catalog::Catalog::LOOKUP
            .into_iter()
            .map(|catalog| crate::import::CatalogAvailability {
                catalog,
                state: if !self.source_is_configured(catalog) {
                    crate::import::SourceAvailability::NotConfigured
                } else if !self.prefs.identification.catalogs.enabled(catalog) {
                    crate::import::SourceAvailability::Off
                } else {
                    crate::import::SourceAvailability::On
                },
            })
            .collect()
    }

    /// Whether this library holds the credentials `catalog` needs.
    /// MusicBrainz's API is open, so it needs none; Discogs needs a key it has
    /// not rejected. Total over the asked catalogs, so a new one has to state
    /// what it needs.
    fn source_is_configured(&self, catalog: bae_catalogs::catalog::Catalog) -> bool {
        match catalog {
            bae_catalogs::catalog::Catalog::MusicBrainz => true,
            bae_catalogs::catalog::Catalog::Discogs => self.discogs_token_status().is_usable(),
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("serialize configuration: {0}")]
    Serialization(String),
    #[error("configuration file: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    File(#[from] coven::FileError),
    #[error(transparent)]
    Stores(#[from] coven::StoreLayoutError),
    #[error(transparent)]
    Storage(#[from] coven::StorageError),
}

/// TOML has no null value. Spell the no-key state explicitly so every
/// preference remains present in the file, including an unset Discogs key.
fn serialize_discogs<S>(value: &Option<DiscogsValidation>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(validation) => validation.serialize(serializer),
        None => DiscogsTokenStatus::NotConfigured.serialize(serializer),
    }
}

/// Require the key even when no Discogs token is configured; a missing key
/// must fail the load instead of implicitly selecting `None`.
fn deserialize_discogs<'de, D>(deserializer: D) -> Result<Option<DiscogsValidation>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match DiscogsTokenStatus::deserialize(deserializer)? {
        DiscogsTokenStatus::NotConfigured => None,
        DiscogsTokenStatus::Unvalidated => Some(DiscogsValidation::Unvalidated),
        DiscogsTokenStatus::Valid => Some(DiscogsValidation::Valid),
        DiscogsTokenStatus::Rejected => Some(DiscogsValidation::Rejected),
    })
}

/// bae's preferences in its own per-library directory. A missing file means
/// the person has not changed any preferences on this device.
pub const PREFERENCES_FILENAME: &str = "preferences.toml";

/// bae's own per-library settings: `preferences.toml`.
/// Device-local, like the rest of the library directory; nothing here syncs.
///
/// No field carries a `serde` default — serialization always emits every key,
/// so a file missing a key fails the load rather than silently taking an
/// implicit value. Only a missing file means defaults (see
/// [`PREFERENCES_FILENAME`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some` doubles as the hint that a key is in the keyring, so
    /// settings render without a keyring read.
    #[serde(
        serialize_with = "serialize_discogs",
        deserialize_with = "deserialize_discogs"
    )]
    pub discogs: Option<DiscogsValidation>,
    /// How loudness normalization is applied at playback. Defaults to `Off`.
    pub replay_gain_mode: ReplayGainMode,
    /// Configured export presets offered by release and track export.
    pub save_presets: Vec<SavePreset>,
    /// Id of the preset a track save defaults to. A required, valid preset id
    /// that applies to track saves (config validation keeps it non-dangling).
    pub default_track_save_preset: String,
    /// Id of the preset a release save defaults to. A required, valid preset id
    /// that applies to release saves (config validation keeps it non-dangling).
    pub default_release_save_preset: String,
    /// Whether playback pauses between vinyl/cassette sides and CD discs.
    /// Defaults to `true`: a side break is part of how the release was made,
    /// so bae plays it unless the person turns it off.
    pub pause_between_sides: bool,
    /// Whether a side or disc pause ends on its own after a countdown, and how
    /// long it is. Defaults to `Off`: the pause waits for Play.
    pub side_pause_countdown: SidePauseCountdown,
    /// How many file uploads coven's upload drain runs at once. Device-local: a
    /// concurrency limit reflects one machine's link and CPU, so unlike most
    /// preferences it does not follow the user across devices.
    pub max_concurrent_uploads: NonZeroU32,
    /// How many file downloads a pin fetches at once. Device-local, like uploads.
    pub max_concurrent_downloads: NonZeroU32,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. Defaults to `false` (elapsed). Kept
    /// here rather than in each platform's own store, so every app on this
    /// device reads the same choice.
    pub show_remaining_time: bool,
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. Defaults to `false`
    /// (capped).
    pub library_full_width: bool,
    /// Whether import fully decodes each track to verify it (fatal-error / frame
    /// shortfall), failing the import for a broken track rather than importing it
    /// and failing at play time. Rides the loudness decode, so it adds no work.
    /// Defaults to `true`.
    pub verify_decode_on_import: bool,
    /// Where an import puts its release: the cloud or this device, and whether
    /// a cloud release stays downloaded here.
    pub import_storage: ImportStoragePreferences,
    /// How identification runs: on its own or not, what an automatic run goes
    /// on to do, and the catalogs it asks.
    pub identification: IdentificationPreferences,
    /// Whether casting to a network receiver (Cast, UPnP, AirPlay) is available.
    /// Defaults to `false`: casting browses the local network and serves audio
    /// off this machine, so it stays off until the user asks for it. While off,
    /// no discovery runs and no cast session can be started.
    pub cast_enabled: bool,
    /// What a track a Cast or UPnP device can't be served as stored is
    /// converted to. Defaults to `Mp3`.
    pub cast_transcode_format: CastTranscodeFormat,
    /// Local automation server configuration. The bearer token is keyring-only.
    pub mcp: McpConfig,
    /// Subsonic/OpenSubsonic server settings (`enabled`, `port`, `username`).
    /// The password is keyring-only, like the MCP bearer token; the server
    /// controller combines this `username` with it into the runtime credential.
    pub subsonic: SubsonicConfig,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            discogs: None,
            replay_gain_mode: ReplayGainMode::Off,
            save_presets: default_save_presets(),
            default_track_save_preset: "flac".to_string(),
            default_release_save_preset: "flac".to_string(),
            pause_between_sides: true,
            side_pause_countdown: SidePauseCountdown::Off,
            max_concurrent_uploads: default_transfer_concurrency(),
            max_concurrent_downloads: default_transfer_concurrency(),
            show_remaining_time: false,
            library_full_width: false,
            verify_decode_on_import: true,
            import_storage: ImportStoragePreferences::default(),
            identification: IdentificationPreferences::default(),
            cast_enabled: false,
            cast_transcode_format: CastTranscodeFormat::Mp3,
            mcp: McpConfig::disabled_default(),
            subsonic: SubsonicConfig::disabled_default(),
        }
    }
}

/// A store discovered through coven, with its current committed name, or
/// while a replacement of this device's copy is unfinished, the name the new
/// copy loaded.
#[derive(Clone, Debug)]
pub struct LibraryInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub is_active: bool,
    pub state: LibraryState,
}

/// Whether a library on this device opens, and where it is stored.
#[derive(Clone, Debug)]
pub enum LibraryState {
    /// It opens: stored at its cloud home, or on this device only (`None`).
    Openable(Option<StorageConfig>),
    /// Its storage settings can't be read, which keeps it from opening.
    Unreadable(crate::ui::UiError),
    /// A reset or a restore of this device's copy put the new copy in place
    /// of the old one but stopped before it finished. The library opens once
    /// the call named here runs again and finishes it; coven refuses the
    /// other call.
    ReplacementUnfinished(coven::ReplacedBy),
}

/// Device-local preferences and the store they belong to. Store names, device
/// identity and storage settings are owned and read by coven.
#[derive(Clone, Debug)]
pub struct Config {
    pub store_id: coven::StoreId,
    library_path: PathBuf,
    pub prefs: Preferences,
}

impl Config {
    /// bae's directory for preferences and caches, outside coven's store.
    pub fn library_path(&self) -> &std::path::Path {
        &self.library_path
    }

    pub fn load_registered_library(
        app_dir: &AppDir,
        library_id: &str,
    ) -> Result<Self, ConfigError> {
        let store_id = coven::StoreId(uuid::Uuid::parse_str(library_id).map_err(|error| {
            ConfigError::Config(format!("invalid library id {library_id}: {error}"))
        })?);
        let library_path = app_dir.registered_library(&store_id.to_string());
        let prefs = read_preferences(&library_path)?;
        Ok(Self {
            store_id,
            library_path,
            prefs,
        })
    }

    pub fn save_active_library(&self, app_dir: &AppDir) -> Result<(), ConfigError> {
        write_atomic(
            &app_dir.active_library_pointer(),
            self.store_id.to_string().as_bytes(),
        )?;
        Ok(())
    }

    pub fn save_preferences(&self) -> Result<(), ConfigError> {
        std::fs::create_dir_all(&self.library_path)?;
        let serialized = toml::to_string_pretty(&self.prefs)
            .map_err(|error| ConfigError::Serialization(error.to_string()))?;
        write_atomic(
            &self.library_path.join(PREFERENCES_FILENAME),
            serialized.as_bytes(),
        )?;
        Ok(())
    }

    pub fn with_defaults(store_id: coven::StoreId, library_path: impl Into<PathBuf>) -> Self {
        Self {
            store_id,
            library_path: library_path.into(),
            prefs: Preferences::default(),
        }
    }

    pub async fn discover_libraries(app_dir: &AppDir) -> Result<Vec<LibraryInfo>, ConfigError> {
        let active_id = Self::active_library_id(app_dir)?;
        let layout = app_dir.store_layout();
        let mut libraries = Vec::new();
        for store in layout.stores().await? {
            let state = match store.unfinished_replacement {
                // Whatever its settings say, only finishing the call opens it.
                Some(by) => LibraryState::ReplacementUnfinished(by),
                None => match coven::Coven::storage_config(&layout.store_dir(&store.id)).await {
                    Ok(storage) => LibraryState::Openable(storage),
                    Err(error) => {
                        tracing::warn!(store = %store.id, %error, "a library's settings can't be read");
                        let error = crate::library::LibraryError::CloudHome(error);
                        LibraryState::Unreadable(crate::ui::UiError::diagnostic(
                            error.category(),
                            error,
                        ))
                    }
                },
            };
            let id = store.id.to_string();
            libraries.push(LibraryInfo {
                is_active: active_id.as_deref() == Some(id.as_str()),
                path: app_dir.registered_library(&id),
                id,
                name: store.name,
                state,
            });
        }
        libraries.sort_by(|a, b| {
            b.is_active
                .cmp(&a.is_active)
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(libraries)
    }

    pub fn active_library_id(app_dir: &AppDir) -> Result<Option<String>, ConfigError> {
        let pointer_path = app_dir.active_library_pointer();
        let Some(content) = read_optional_file(&pointer_path)? else {
            return Ok(None);
        };
        let id = content.trim();
        let parsed = uuid::Uuid::parse_str(id)
            .map_err(|error| ConfigError::Config(format!("{}: {error}", pointer_path.display())))?;
        Ok(Some(parsed.to_string()))
    }
}

/// Read bae's preferences from a library directory. A directory with no
/// `preferences.toml` is one bae has recorded no preference in yet, so every
/// preference is at its default.
fn read_preferences(library_dir: &std::path::Path) -> Result<Preferences, ConfigError> {
    let path = library_dir.join(PREFERENCES_FILENAME);
    let Some(content) = read_optional_file(&path)? else {
        info!(
            "no {} yet; preferences start at their defaults",
            path.display()
        );
        return Ok(Preferences::default());
    };
    toml::from_str(&content)
        .map_err(|e| ConfigError::Serialization(format!("{}: {e}", path.display())))
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        // Keep the path in the error: "Access is denied" without the file it
        // was denied on is undiagnosable from a user report.
        Err(e) => Err(ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", path.display()),
        ))),
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
