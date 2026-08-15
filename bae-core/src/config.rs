use serde::{Deserialize, Deserializer, Serialize};
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

mod dev;
mod handle;
mod keyring;
mod save;
mod server;

pub(crate) use dev::dev_secrets;
pub use handle::ConfigHandle;
pub use keyring::init_keyring;
#[cfg(any(test, feature = "test-utils"))]
pub use keyring::install_test_keyring;
pub use save::{SaveBitDepth, SaveCodec, SaveFilenameToken, SavePregapPlacement, SavePreset};
pub use server::{
    McpConfig, SubsonicConfig, SubsonicCredential, MCP_DEFAULT_PORT, SUBSONIC_DEFAULT_PORT,
};

use coven::{write_atomic, WriteError};
use dev::dev_mode_enabled;
use save::default_save_presets;

/// Blob transfers bae runs at once, per direction, on a fresh library. Serial
/// (1) is safe but slow; a small burst keeps a single stalled transfer from
/// holding up the rest.
pub const DEFAULT_CONCURRENT_TRANSFERS: u32 = 3;

/// The largest transfer concurrency the UI offers and the setters accept.
pub const MAX_CONCURRENT_TRANSFERS: u32 = 8;

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
/// it. Defaults to `Off`. Set by editing `config.yaml`; there is no UI picker
/// yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
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
        match self.discogs {
            None => DiscogsTokenStatus::NotConfigured,
            Some(DiscogsValidation::Valid) => DiscogsTokenStatus::Valid,
            Some(DiscogsValidation::Unvalidated) => DiscogsTokenStatus::Unvalidated,
            Some(DiscogsValidation::Rejected) => DiscogsTokenStatus::Rejected,
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

    /// Wrap coven's config, filling bae-only fields with defaults. Used after a
    /// restore where coven produced the synced/cloud config.
    pub fn from_coven(c: coven::Config, library_path: PathBuf) -> Self {
        let mut cfg = Self::with_defaults(
            c.store_id.clone(),
            c.device_id.clone(),
            library_path,
            c.store_name.clone(),
        );
        cfg.inner = c;
        cfg.mcp = McpConfig::disabled_default();
        cfg
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
}

/// bae's application directory (`~/.bae`). The base coven's restore/join build
/// per-library dirs under (`<app_dir>/libraries/<id>`).
pub fn bae_dir() -> Result<std::path::PathBuf, ConfigError> {
    Ok(dirs::home_dir()
        .ok_or_else(|| ConfigError::Config("could not determine home directory".to_string()))?
        .join(".bae"))
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

/// YAML config file structure for non-secret settings (per-library).
///
/// Every field except `device_id` is required, with no `serde` defaults:
/// serialization always emits every key, so a missing key fails the load
/// rather than silently taking an implicit value.
///
/// **Adding a field to `ConfigYaml`: add it to [`Config::with_defaults`] too.**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigYaml {
    pub library_id: String,
    /// Human-readable name for this library
    pub library_name: String,
    /// Unique identifier for this device, used as the namespace key for sync changesets.
    /// Auto-generated on first startup if missing.
    #[serde(default)]
    pub device_id: Option<String>,
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some(v)` doubles as the hint that a key is in the keyring,
    /// so the settings screen renders without a keyring read.
    #[serde(deserialize_with = "deserialize_some")]
    pub discogs: Option<DiscogsValidation>,
    /// How loudness normalization is applied at playback.
    pub replay_gain_mode: ReplayGainMode,
    /// Configured export presets offered by release and track export.
    pub save_presets: Vec<SavePreset>,
    /// Id of the preset a track save defaults to. A required, valid preset id
    /// that applies to track saves (config validation keeps it non-dangling).
    pub default_track_save_preset: String,
    /// Id of the preset a release save defaults to. A required, valid preset id
    /// that applies to release saves (config validation keeps it non-dangling).
    pub default_release_save_preset: String,
    /// Whether playback pauses between vinyl/cassette sides.
    pub pause_between_sides: bool,
    /// How many blob uploads coven's upload drain runs at once. Device-local (a
    /// concurrency limit is a per-machine choice, not a synced preference).
    pub max_concurrent_uploads: NonZeroU32,
    /// How many blob downloads a pin fetches at once. Device-local, like uploads.
    pub max_concurrent_downloads: NonZeroU32,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed.
    pub show_remaining_time: bool,
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column.
    pub library_full_width: bool,
    /// Whether import fully decodes each track to verify it (fatal-error / frame
    /// shortfall), failing the import for a broken track rather than importing it
    /// and failing at play time. Rides the loudness decode, so it adds no work.
    pub verify_decode_on_import: bool,
    /// Whether casting to a network receiver is available at all. Off unless the
    /// user turns it on: while off, nothing browses the local network and no
    /// cast session can be started.
    pub cast_enabled: bool,
    /// Local automation server configuration.
    pub mcp: McpConfig,
    /// Subsonic/OpenSubsonic server settings. The password is keyring-only.
    pub subsonic: SubsonicConfig,
    /// Cloud home provider + per-provider settings. Flattened so the on-disk
    /// keys sit at the top level.
    #[serde(flatten)]
    pub cloud_home: CloudHomeConfig,
}

impl ConfigYaml {
    /// Convert to a runtime Config. The caller resolves device_id (auto-generating
    /// if missing from YAML) and provides the library_dir.
    fn into_config(self, device_id: String, library_path: PathBuf) -> Config {
        Config {
            inner: coven::Config {
                store_id: self.library_id,
                device_id,
                store_name: self.library_name,
                cloud_home: self.cloud_home,
            },
            library_path,
            discogs: self.discogs,
            replay_gain_mode: self.replay_gain_mode,
            save_presets: self.save_presets,
            default_track_save_preset: self.default_track_save_preset,
            default_release_save_preset: self.default_release_save_preset,
            pause_between_sides: self.pause_between_sides,
            max_concurrent_uploads: self.max_concurrent_uploads,
            max_concurrent_downloads: self.max_concurrent_downloads,
            show_remaining_time: self.show_remaining_time,
            library_full_width: self.library_full_width,
            verify_decode_on_import: self.verify_decode_on_import,
            cast_enabled: self.cast_enabled,
            mcp: self.mcp,
            subsonic: self.subsonic,
        }
    }
}

impl From<&Config> for ConfigYaml {
    fn from(config: &Config) -> Self {
        Self {
            library_id: config.store_id.clone(),
            library_name: config.store_name.clone(),
            device_id: Some(config.device_id.clone()),
            discogs: config.discogs,
            replay_gain_mode: config.replay_gain_mode,
            save_presets: config.save_presets.clone(),
            default_track_save_preset: config.default_track_save_preset.clone(),
            default_release_save_preset: config.default_release_save_preset.clone(),
            pause_between_sides: config.pause_between_sides,
            max_concurrent_uploads: config.max_concurrent_uploads,
            max_concurrent_downloads: config.max_concurrent_downloads,
            show_remaining_time: config.show_remaining_time,
            library_full_width: config.library_full_width,
            verify_decode_on_import: config.verify_decode_on_import,
            cast_enabled: config.cast_enabled,
            mcp: config.mcp,
            subsonic: config.subsonic.clone(),
            cloud_home: config.cloud_home.clone(),
        }
    }
}

/// Metadata about a discovered library (for the library switcher UI)
/// A library found under `~/.bae/libraries/`, whether or not it can be opened.
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
/// Holds coven's sync/cloud config (`inner`) plus bae's own fields.
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
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some` doubles as the hint that a key is in the keyring, so
    /// settings render without a keyring read.
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
    /// Whether playback pauses between vinyl/cassette sides.
    pub pause_between_sides: bool,
    /// How many blob uploads coven's upload drain runs at once. Device-local: a
    /// concurrency limit reflects one machine's link and CPU, so unlike most
    /// preferences it does not follow the user across devices.
    pub max_concurrent_uploads: NonZeroU32,
    /// How many blob downloads a pin fetches at once. Device-local, like uploads.
    pub max_concurrent_downloads: NonZeroU32,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. Defaults to `false` (elapsed). A
    /// preference like any other, so it follows the user to every device rather
    /// than living in each platform's own store.
    pub show_remaining_time: bool,
    /// Whether the library page spans the window's full width instead of
    /// centering its content in a width-capped column. Defaults to `false`
    /// (capped). A synced preference, like `show_remaining_time`.
    pub library_full_width: bool,
    /// Whether import verifies each track by fully decoding it, failing the import
    /// for a broken (truncated/corrupt) track. Defaults to `true`.
    pub verify_decode_on_import: bool,
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

    pub fn load_registered_library(
        library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let bae_dir = bae_dir()?;
        Self::load_registered_library_from_bae_dir(&bae_dir, library_id, ids)
    }

    pub(crate) fn load_registered_library_from_bae_dir(
        bae_dir: &std::path::Path,
        library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let library_dir = registered_library_path(bae_dir, library_id);
        Self::load_from_registered_library_dir(library_dir, library_id, ids)
    }

    fn load_from_registered_library_dir(
        library_dir: PathBuf,
        expected_library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let config_path = library_dir.join("config.yaml");
        let yaml_config = load_registered_config_yaml(&library_dir, expected_library_id)?;
        Self::config_from_yaml(yaml_config, library_dir, &config_path, ids)
    }

    /// Read config.yaml into the fields this build uses.
    fn load_config_yaml(config_path: &std::path::Path) -> Result<ConfigYaml, ConfigError> {
        let content = std::fs::read_to_string(config_path)?;
        parse_config_yaml(&content)
    }

    fn config_from_yaml(
        mut yaml_config: ConfigYaml,
        library_dir: PathBuf,
        config_path: &std::path::Path,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let device_id = match yaml_config.device_id.clone() {
            Some(id) => id,
            None => {
                let id = ids.new_id();
                info!("No device_id in config.yaml, generated: {}", id);
                yaml_config.device_id = Some(id.clone());
                let serialized = serde_yaml::to_string(&yaml_config)
                    .map_err(|e| ConfigError::Serialization(e.to_string()))?;
                write_atomic(config_path, serialized.as_bytes()).map_err(WriteError::into_inner)?;
                id
            }
        };
        Ok(yaml_config.into_config(device_id, library_dir))
    }

    pub fn is_dev_mode() -> bool {
        dev_mode_enabled()
    }

    /// Save the active library UUID to the global pointer file (~/.bae/active-library).
    pub fn save_active_library(&self) -> Result<(), ConfigError> {
        let app_dir = bae_dir()?;
        std::fs::create_dir_all(&app_dir)?;
        let pointer_path = app_dir.join("active-library");
        write_atomic(&pointer_path, self.store_id.as_bytes()).map_err(WriteError::into_inner)?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        self.write_config_yaml().map_err(WriteError::into_inner)
    }

    fn write_config_yaml(&self) -> Result<(), WriteError<ConfigError>> {
        std::fs::create_dir_all(&self.library_path)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::from(e)))?;
        let yaml: ConfigYaml = self.into();
        let serialized = serde_yaml::to_string(&yaml)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::Serialization(e.to_string())))?;
        write_atomic(
            &self.library_path.join("config.yaml"),
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
            discogs: None,
            replay_gain_mode: ReplayGainMode::Off,
            save_presets: default_save_presets(),
            default_track_save_preset: "flac".to_string(),
            default_release_save_preset: "flac".to_string(),
            pause_between_sides: false,
            max_concurrent_uploads: NonZeroU32::new(DEFAULT_CONCURRENT_TRANSFERS)
                .expect("DEFAULT_CONCURRENT_TRANSFERS is non-zero"),
            max_concurrent_downloads: NonZeroU32::new(DEFAULT_CONCURRENT_TRANSFERS)
                .expect("DEFAULT_CONCURRENT_TRANSFERS is non-zero"),
            show_remaining_time: false,
            library_full_width: false,
            verify_decode_on_import: true,
            cast_enabled: false,
            mcp: McpConfig::disabled_default(),
            subsonic: SubsonicConfig::disabled_default(),
        }
    }

    /// Discover all libraries under ~/.bae/libraries/.
    pub fn discover_libraries() -> Result<Vec<LibraryInfo>, ConfigError> {
        let app_dir = bae_dir()?;
        discover_libraries_from_bae_dir(&app_dir)
    }

    pub fn active_library_id() -> Result<Option<String>, ConfigError> {
        let app_dir = bae_dir()?;
        read_active_library_id(&app_dir)
    }
}

fn discover_libraries_from_bae_dir(
    app_dir: &std::path::Path,
) -> Result<Vec<LibraryInfo>, ConfigError> {
    let active_id = read_active_library_id(app_dir)?;

    let mut libraries: Vec<LibraryInfo> = discover_all_library_paths(app_dir)
        .into_iter()
        .map(|(path, yaml)| match yaml {
            Ok(yaml) => LibraryInfo {
                is_active: active_id.as_deref() == Some(&yaml.library_id),
                id: yaml.library_id,
                name: yaml.library_name,
                path,
                cloud_provider: yaml.cloud_home.provider.clone(),
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

/// Rename a library by id without loading it into memory: locate its directory,
/// read its `config.yaml`, replace `library_name`, write back. Used by
/// `LibraryManager::rename_library` for libraries that aren't the active one —
/// the active one renames through [`ConfigHandle::rename_library`], so its
/// subscribers see the change.
pub fn rename_inactive_library(
    bae_dir: &std::path::Path,
    library_id: &str,
    new_name: &crate::library_name::LibraryName,
) -> Result<(), ConfigError> {
    let library_dir = find_library_by_id(bae_dir, library_id)
        .ok_or_else(|| ConfigError::Config(format!("library not found: {library_id}")))?;
    let config_path = library_dir.join("config.yaml");
    let content = std::fs::read_to_string(&config_path)?;
    let mut yaml: ConfigYaml =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    yaml.library_name = new_name.as_str().to_string();
    let serialized =
        serde_yaml::to_string(&yaml).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    write_atomic(&config_path, serialized.as_bytes()).map_err(WriteError::into_inner)?;
    Ok(())
}

pub(crate) fn registered_library_path(bae_dir: &std::path::Path, library_id: &str) -> PathBuf {
    bae_dir.join("libraries").join(library_id)
}

fn load_registered_config_yaml(
    library_dir: &std::path::Path,
    expected_library_id: &str,
) -> Result<ConfigYaml, ConfigError> {
    let yaml_config = Config::load_config_yaml(&library_dir.join("config.yaml"))?;
    if yaml_config.library_id != expected_library_id {
        return Err(ConfigError::Config(format!(
            "registered library directory {} contains library_id {}",
            library_dir.display(),
            yaml_config.library_id
        )));
    }
    Ok(yaml_config)
}

/// Read the active library UUID from `~/.bae/active-library`, if it exists.
fn read_active_library_id(bae_dir: &std::path::Path) -> Result<Option<String>, ConfigError> {
    let pointer_path = bae_dir.join("active-library");
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

/// Find a library's directory by its UUID, scanning `~/.bae/libraries/` subdirectories.
fn find_library_by_id(bae_dir: &std::path::Path, uuid: &str) -> Option<PathBuf> {
    for (path, yaml) in discover_all_library_paths(bae_dir) {
        // A library whose config will not parse cannot be addressed by id — its id
        // is precisely what we could not read.
        if yaml.is_ok_and(|yaml| yaml.library_id == uuid) {
            return Some(path);
        }
    }
    None
}

/// Collect every library directory under ~/.bae/libraries/ with the outcome of
/// reading its config — `Err` for one that cannot be read.
///
/// The failure is carried, not dropped, so an unreadable library remains visible
/// in the picker.
fn discover_all_library_paths(
    bae_dir: &std::path::Path,
) -> Vec<(PathBuf, Result<ConfigYaml, ConfigError>)> {
    let mut results = Vec::new();
    let libraries_dir = bae_dir.join("libraries");

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
            // A `.bae/libraries/` dir bae created is UTF-8 by construction
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
            match read_config_yaml(&path) {
                Ok(Some(yaml)) => results.push((path, Ok(yaml))),
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

/// Read and parse config.yaml from a library directory, if it exists.
///
/// Returns `Ok(None)` if the file doesn't exist, `Err` if it exists but can't be parsed.
fn read_config_yaml(path: &std::path::Path) -> Result<Option<ConfigYaml>, ConfigError> {
    let config_path = path.join("config.yaml");
    let Some(content) = read_optional_file(&config_path)? else {
        return Ok(None);
    };
    parse_config_yaml(&content).map(Some)
}

/// Parse config.yaml into the fields this build uses.
fn parse_config_yaml(content: &str) -> Result<ConfigYaml, ConfigError> {
    serde_yaml::from_str(content).map_err(|e| ConfigError::Serialization(e.to_string()))
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
