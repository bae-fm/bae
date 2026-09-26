use serde::{Deserialize, Deserializer, Serialize};
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

mod app_dir;
mod handle;
mod identification;
mod import_storage;
mod keyring;
mod save;
mod server;

pub use app_dir::AppDir;
pub use handle::ConfigHandle;
pub use identification::{
    IdentificationPreferences, IdentificationStep, IdentificationSteps, LookupCatalogPreferences,
};
#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub use import_storage::ImportDestination;
pub use import_storage::ImportStoragePreferences;
pub use keyring::init_keyring;
#[cfg(any(test, feature = "test-utils", debug_assertions))]
pub use keyring::install_test_keyring;
pub use save::{SaveBitDepth, SaveCodec, SaveFilenameToken, SavePregapPlacement, SavePreset};
pub use server::{
    McpConfig, SubsonicConfig, SubsonicCredential, MCP_DEFAULT_PORT, SUBSONIC_DEFAULT_PORT,
};

use coven::{write_atomic, StoreDir, WriteError};
use save::default_save_presets;

/// Blob transfers bae runs at once, per direction, on a fresh library. Serial
/// (1) is safe but slow; a small burst keeps a single stalled transfer from
/// holding up the rest.
pub const DEFAULT_CONCURRENT_TRANSFERS: u32 = 3;

/// The largest transfer concurrency the UI offers and the setters accept.
pub const MAX_CONCURRENT_TRANSFERS: u32 = 8;

fn default_transfer_concurrency() -> NonZeroU32 {
    NonZeroU32::new(DEFAULT_CONCURRENT_TRANSFERS).expect("DEFAULT_CONCURRENT_TRANSFERS is non-zero")
}

pub(crate) fn default_transfer_limits() -> coven::TransferLimits {
    let limit = usize_bound(default_transfer_concurrency());
    coven::TransferLimits {
        uploads: limit,
        downloads: limit,
    }
}

/// A blob-transfer concurrency setting must be at least 1 — coven's drain admits
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

/// Cloud home provider selection.
pub use coven::CloudProvider;

/// Cloud home settings (provider + per-provider fields).
pub use coven::CloudHomeConfig;

/// How a cloud home stores its objects: opaque (encrypted, obfuscated blob paths)
/// or browsable (in the clear, at readable paths). Chosen when the home is
/// created; drives both encryption-at-rest and the blob-path scheme. Not access
/// control — the provider's credentials gate the bucket either way; this only
/// decides whether what's stored is legible.
pub use coven::HomeStorage;

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
/// it. Defaults to `Off`. Set by editing `preferences.yaml`; there is no UI
/// picker yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
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
    /// [`Catalog`](crate::import::Catalog).
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
        crate::import::Catalog::LOOKUP
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
    fn source_is_configured(&self, catalog: crate::import::Catalog) -> bool {
        match catalog {
            crate::import::Catalog::MusicBrainz => true,
            crate::import::Catalog::Discogs => self.discogs_token_status().is_usable(),
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }

    /// The coven sync/cloud config bae embeds. Handed to the `CovenHandle` (via
    /// its config provider) and read fresh by coven for the cloud-home selection,
    /// the blob-path scheme, sync, and restore-code generation.
    pub fn to_coven(&self) -> coven::Config {
        self.inner.clone()
    }

    /// Display string for the connected cloud account, derived from config
    /// alone. For OAuth providers, a set provider implies stored credentials
    /// (sign-in saves the keyring entry, then sets the provider), so we report
    /// "Connected" without reading the keyring — rendering settings never
    /// triggers a keychain prompt. `None` when no provider is configured.
    pub fn cloud_account_display(&self) -> Option<String> {
        match self.cloud_home.provider.as_ref()? {
            CloudProvider::S3 => self
                .cloud_home
                .s3_bucket
                .as_ref()
                .map(|b| format!("s3://{b}")),
            CloudProvider::CloudKit => Some("iCloud".to_string()),
            CloudProvider::GoogleDrive | CloudProvider::Dropbox | CloudProvider::OneDrive => {
                Some("Connected".to_string())
            }
        }
    }

    /// The running config for a store coven just restored or joined. coven
    /// wrote that store's `config.yaml` itself; bae has recorded no preferences
    /// for it yet, so they start at their defaults, as they do on any library
    /// whose `preferences.yaml` has not been written.
    pub fn from_coven(c: coven::Config, library_path: PathBuf) -> Self {
        Self {
            inner: c,
            library_path,
            prefs: Preferences::default(),
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
    /// coven could not read or write the library's `config.yaml`, which it
    /// owns.
    #[error("store configuration: {0}")]
    Store(#[from] coven::ConfigError),
}

/// Deserialize an `Option<T>` whose key must be present, even when its value is
/// `null`. A plain `Option<T>` reads a missing key as `None`; this fails the load
/// instead, so a config file that omits the key is loud rather than silently
/// defaulted. An explicit `null` still reads as `None`.
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::deserialize(deserializer)
}

/// The file in a library directory that holds bae's [`Preferences`], next to
/// coven's `config.yaml`.
///
/// Two files because two owners: coven writes `config.yaml` (store identity,
/// device id, snapshot policy, cloud home) on create, restore, and join, and
/// reads it back as the mark of a finished join; bae writes only this one.
/// A library directory whose `preferences.yaml` has not been written yet — a
/// new, restored, or joined library before any setting changes — has every
/// preference at its default.
pub const PREFERENCES_FILENAME: &str = "preferences.yaml";

/// bae's own per-library settings: `preferences.yaml`, in field order.
/// Device-local, like the rest of the library directory; nothing here syncs.
///
/// No field carries a `serde` default — serialization always emits every key,
/// so a file missing a key fails the load rather than silently taking an
/// implicit value. Only a missing file means defaults (see
/// [`PREFERENCES_FILENAME`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some` doubles as the hint that a key is in the keyring, so
    /// settings render without a keyring read.
    #[serde(deserialize_with = "deserialize_some")]
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
    /// How many blob uploads coven's upload drain runs at once. Device-local: a
    /// concurrency limit reflects one machine's link and CPU, so unlike most
    /// preferences it does not follow the user across devices.
    pub max_concurrent_uploads: NonZeroU32,
    /// How many blob downloads a pin fetches at once. Device-local, like uploads.
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
    /// on to do, the steps every run takes, and the catalogs it asks.
    pub identification: IdentificationPreferences,
    /// Whether a candidate's draft is created from the folder's own metadata.
    /// Defaults to `true`; off means the draft starts blank.
    pub prefill_with_file_metadata: bool,
    /// Whether casting to a network receiver (Cast, UPnP, AirPlay) is available.
    /// Defaults to `false`: casting browses the local network and serves audio
    /// off this machine, so it stays off until the user asks for it. While off,
    /// no discovery runs and no cast session can be started.
    pub cast_enabled: bool,
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
            prefill_with_file_metadata: true,
            cast_enabled: false,
            mcp: McpConfig::disabled_default(),
            subsonic: SubsonicConfig::disabled_default(),
        }
    }
}

/// Metadata about a discovered library (for the library switcher UI)
/// A library registered under the app directory, whether or not it can be opened.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub is_active: bool,
    pub cloud_provider: Option<CloudProvider>,
    /// Why this library cannot be opened, or `None` when its config loaded.
    ///
    /// A library whose config.yaml will not parse is still listed — it must not
    /// silently vanish from the picker. Its `id` and `name` use the directory
    /// name because the configured values could not be read.
    pub error: Option<String>,
}

/// Application configuration.
///
/// Three parts: coven's sync/cloud config (`inner`), bae's own settings
/// (`prefs`), and where this library lives on this machine.
/// `Deref`/`DerefMut` expose coven's fields directly, so `config.store_id`,
/// `config.device_id`, and `config.cloud_home.provider = …` read and write
/// through to `inner`.
#[derive(Clone, Debug)]
pub struct Config {
    /// Sync/cloud config coven owns — embedded, not re-declared.
    pub inner: coven::Config,
    /// Runtime location of this library. It is host context rather than synced
    /// configuration, so it stays outside `coven::Config` and off the wire.
    library_path: PathBuf,
    /// bae's own settings, exactly as `preferences.yaml` carries them.
    pub prefs: Preferences,
}

impl std::ops::Deref for Config {
    type Target = coven::Config;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Config {
    /// The local library path for host UI and host-owned files. Callers receive
    /// the path value, not Coven's store owner.
    pub fn library_path(&self) -> &std::path::Path {
        &self.library_path
    }

    /// Open the library registered as `library_id`: coven's `config.yaml` and
    /// bae's `preferences.yaml` from its directory.
    pub fn load_registered_library(
        app_dir: &AppDir,
        library_id: &str,
    ) -> Result<Self, ConfigError> {
        let library_dir = app_dir.registered_library(library_id);
        let inner = coven::Config::load_from_config_yaml(&StoreDir::new(&library_dir))?;
        if inner.store_id != library_id {
            return Err(ConfigError::Config(format!(
                "registered library directory {} contains store_id {}",
                library_dir.display(),
                inner.store_id
            )));
        }
        let prefs = read_preferences(&library_dir)?;
        Ok(Self {
            inner,
            library_path: library_dir,
            prefs,
        })
    }

    /// Record this library as the one this device last opened, in the app
    /// directory's active-library pointer.
    pub fn save_active_library(&self, app_dir: &AppDir) -> Result<(), ConfigError> {
        app_dir.create()?;
        write_atomic(&app_dir.active_library_pointer(), self.store_id.as_bytes())
            .map_err(WriteError::into_inner)?;
        Ok(())
    }

    /// Write coven's part of this config to the library's `config.yaml`,
    /// through coven.
    pub fn save_store_config(&self) -> Result<(), ConfigError> {
        self.write_store_config().map_err(WriteError::into_inner)
    }

    /// Write bae's preferences to the library's `preferences.yaml`.
    pub fn save_preferences(&self) -> Result<(), ConfigError> {
        self.write_preferences().map_err(WriteError::into_inner)
    }

    /// [`Self::save_store_config`], reporting whether the new file was
    /// installed when the write failed.
    fn write_store_config(&self) -> Result<(), WriteError<ConfigError>> {
        self.inner
            .save_to_config_yaml(&StoreDir::new(&self.library_path))
            .map_err(|error| {
                if error.installed_new_file() {
                    WriteError::AfterCommit(error.into())
                } else {
                    WriteError::BeforeCommit(error.into())
                }
            })
    }

    fn write_preferences(&self) -> Result<(), WriteError<ConfigError>> {
        std::fs::create_dir_all(&self.library_path)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::from(e)))?;
        let serialized = serde_yaml::to_string(&self.prefs)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::Serialization(e.to_string())))?;
        write_atomic(
            &self.library_path.join(PREFERENCES_FILENAME),
            serialized.as_bytes(),
        )
        .map_err(|e| e.map(ConfigError::from))
    }

    /// Construct a Config with defaults for a new library.
    pub fn with_defaults(
        library_id: String,
        device_id: String,
        library_path: impl AsRef<std::path::Path>,
        library_name: String,
    ) -> Self {
        Self {
            inner: coven::Config::with_defaults(library_id, device_id, library_name),
            library_path: library_path.as_ref().to_path_buf(),
            prefs: Preferences::default(),
        }
    }

    /// Discover every library registered under the app directory.
    pub fn discover_libraries(app_dir: &AppDir) -> Result<Vec<LibraryInfo>, ConfigError> {
        let active_id = Self::active_library_id(app_dir)?;

        let mut libraries: Vec<LibraryInfo> = discover_all_library_paths(app_dir)
            .into_iter()
            .map(|(path, store)| match store {
                Ok(store) => LibraryInfo {
                    is_active: active_id.as_deref() == Some(&store.store_id),
                    id: store.store_id,
                    name: store.store_name,
                    path,
                    cloud_provider: store.cloud_home.provider,
                    error: None,
                },
                // The config is the only thing that knows the library's id and name, and
                // it is what failed — so the directory name stands in for both. It is a
                // UUID, which is what the id would have been anyway.
                Err(e) => {
                    let dir_name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or_default()
                        .to_string();
                    LibraryInfo {
                        is_active: active_id.as_deref() == Some(dir_name.as_str()),
                        id: dir_name.clone(),
                        name: dir_name,
                        path,
                        cloud_provider: None,
                        error: Some(e.to_string()),
                    }
                }
            })
            .collect();

        // Broken libraries sort last: they are visible, but they are not what the user
        // is looking for.
        libraries.sort_by(|a, b| {
            a.error
                .is_some()
                .cmp(&b.error.is_some())
                .then_with(|| b.is_active.cmp(&a.is_active))
                .then_with(|| a.name.cmp(&b.name))
        });

        Ok(libraries)
    }

    /// The library this device last opened, from the app directory's
    /// active-library pointer; `None` when no library has been opened yet.
    pub fn active_library_id(app_dir: &AppDir) -> Result<Option<String>, ConfigError> {
        let pointer_path = app_dir.active_library_pointer();
        let Some(content) = read_optional_file(&pointer_path)? else {
            return Ok(None);
        };
        let id = content.trim().to_string();
        if id.is_empty() {
            return Err(ConfigError::Config(format!(
                "active-library pointer at {} is empty",
                pointer_path.display()
            )));
        }
        Ok(Some(id))
    }
}

/// Rename a library by id without loading it into memory: locate its directory,
/// read its `config.yaml` through coven, replace `store_name`, write it back
/// through coven. Used by
/// `LibraryManager::rename_library` for libraries that aren't the active one —
/// the active one renames through [`ConfigHandle::rename_library`], so its
/// subscribers see the change.
pub fn rename_inactive_library(
    app_dir: &AppDir,
    library_id: &str,
    new_name: &crate::library_name::LibraryName,
) -> Result<(), ConfigError> {
    let library_dir = find_library_by_id(app_dir, library_id)
        .ok_or_else(|| ConfigError::Config(format!("library not found: {library_id}")))?;
    let store_dir = StoreDir::new(library_dir);
    let mut store = coven::Config::load_from_config_yaml(&store_dir)?;
    store.store_name = new_name.as_str().to_string();
    store.save_to_config_yaml(&store_dir)?;
    Ok(())
}

/// Read bae's preferences from a library directory. A directory with no
/// `preferences.yaml` is one bae has recorded no preference in yet, so every
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
    serde_yaml::from_str(&content)
        .map_err(|e| ConfigError::Serialization(format!("{}: {e}", path.display())))
}

/// Find a library's directory by its UUID, scanning the app directory's
/// registered libraries.
fn find_library_by_id(app_dir: &AppDir, uuid: &str) -> Option<PathBuf> {
    for (path, store) in discover_all_library_paths(app_dir) {
        // A library whose config will not parse cannot be addressed by id — its id
        // is precisely what we could not read.
        if store.is_ok_and(|store| store.store_id == uuid) {
            return Some(path);
        }
    }
    None
}

/// Collect every library directory registered under the app directory with the
/// outcome of reading its config — `Err` for one that cannot be read.
///
/// The failure is carried, not dropped, so an unreadable library remains visible
/// in the picker.
fn discover_all_library_paths(
    app_dir: &AppDir,
) -> Vec<(PathBuf, Result<coven::Config, ConfigError>)> {
    let mut results = Vec::new();
    let libraries_dir = app_dir.libraries();

    if libraries_dir.is_dir() {
        let entries = match std::fs::read_dir(&libraries_dir) {
            Ok(entries) => entries,
            Err(e) => {
                warn!("cannot read libraries dir {}: {e}", libraries_dir.display());
                return results;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    warn!("skipping unreadable libraries dir entry: {e}");
                    continue;
                }
            };
            let path = entry.path();
            if !path.is_dir() {
                debug!(
                    "skipping non-directory entry in libraries dir: {}",
                    path.display()
                );
                continue;
            }
            // A libraries dir entry bae created is UTF-8 by construction
            // (the id is a UUID). A non-UTF-8 name is foreign or corrupt: its
            // bytes can't round-trip through the `String` path the rest of the
            // app addresses files by, so skip it rather than lossily mangle
            // the path into one that points at nothing.
            if path.to_str().is_none() {
                warn!(
                    "skipping library dir with non-UTF-8 name: {}",
                    path.display()
                );
                continue;
            }
            match read_store_config(&path) {
                Ok(Some(store)) => results.push((path, Ok(store))),
                // Not a library at all — nothing to show, nothing to report.
                Ok(None) => {
                    debug!(
                        "skipping library dir with no config.yaml: {}",
                        path.display()
                    );
                }
                // A library that exists but will not load. It stays in the list,
                // marked broken, so the user sees it rather than losing it.
                Err(e) => {
                    warn!("library at {} cannot be read: {e}", path.display());
                    results.push((path, Err(e)));
                }
            }
        }
    }

    results
}

/// Read coven's `config.yaml` from a library directory, if it exists.
///
/// Returns `Ok(None)` if the file doesn't exist, `Err` if it exists but can't be
/// read.
fn read_store_config(path: &std::path::Path) -> Result<Option<coven::Config>, ConfigError> {
    let store_dir = StoreDir::new(path);
    let config_path = store_dir.config_path();
    match config_path.try_exists() {
        Ok(false) => Ok(None),
        Ok(true) => Ok(Some(coven::Config::load_from_config_yaml(&store_dir)?)),
        Err(e) => Err(ConfigError::Io(std::io::Error::new(
            e.kind(),
            format!("{}: {e}", config_path.display()),
        ))),
    }
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
