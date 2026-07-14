use coven::StoreDir;
use serde::{Deserialize, Deserializer, Serialize};
use std::path::PathBuf;
use tokio::sync::watch;
use tracing::{debug, info, warn};

mod dev;
mod export;
mod keyring;

pub use dev::seed_dev_keyring;
pub use export::{
    ExportBitDepth, ExportLocation, ExportPregapPlacement, ExportPreset, ExportPresetCodec,
    ExportSelection,
};
pub use keyring::init_keyring;
#[cfg(any(test, feature = "test-utils"))]
pub use keyring::install_test_keyring;

use crate::util::atomic_write::{write_atomic, write_atomic_io, WriteError};
use dev::dev_mode_enabled;
use export::{default_export_filename_template, default_export_presets};

pub const MCP_DEFAULT_PORT: u16 = 47777;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub port: u16,
}

impl McpConfig {
    pub fn disabled_default() -> Self {
        Self {
            enabled: false,
            port: MCP_DEFAULT_PORT,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::Config(
                "MCP port must be between 1 and 65535".to_string(),
            ));
        }
        Ok(())
    }
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
    pub fn from_coven(c: coven::Config) -> Self {
        let mut cfg = Self::with_defaults(
            c.store_id.clone(),
            c.device_id.clone(),
            c.store_dir.clone(),
            c.store_name.clone(),
        );
        cfg.inner = c;
        cfg.mcp = McpConfig::disabled_default();
        cfg
    }
}

pub use coven::ConfigError;

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

/// The config schema this build writes. A file carrying an older version is
/// upgraded on read by `migrate_to_current`; a file carrying a newer one is
/// refused, because this build cannot know what its fields mean.
///
/// **Adding a field to `ConfigYaml`: add it to [`Config::with_defaults`] too, and
/// bump this.** `with_defaults` is the single place a field's default is stated —
/// the migration fills an older file's missing keys straight from it, so there is
/// no second table to keep in step.
pub const CONFIG_VERSION: u32 = 2;

/// A file written before versioning existed. Such a file has no `version` key,
/// and is missing every field added since it was written.
const UNVERSIONED: u32 = 0;

/// YAML config file structure for non-secret settings (per-library).
///
/// Every field except `device_id` is required, with no `serde` defaults:
/// serialization always emits every key, so at the *current* [`CONFIG_VERSION`] a
/// missing key means the file is corrupt or foreign, and it fails the load loudly
/// rather than silently taking an implicit value.
///
/// An *older* file legitimately lacks the fields added after it was written. That
/// is not corruption, and it is not this type's problem: `parse_config_yaml`
/// upgrades the file before it ever reaches this struct. So the strictness here
/// keeps its meaning — it now only fires on files that really are broken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigYaml {
    /// The schema this file was written with. See [`CONFIG_VERSION`].
    pub version: u32,
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
    /// Whether an encryption key is stored in the keyring (hint flag, avoids keyring read)
    pub encryption_key_stored: bool,
    /// SHA-256 fingerprint of the encryption key (first 8 bytes, hex).
    /// Used to detect wrong key without attempting decryption.
    #[serde(deserialize_with = "deserialize_some")]
    pub encryption_key_fingerprint: Option<String>,
    /// How loudness normalization is applied at playback.
    pub replay_gain_mode: ReplayGainMode,
    /// Where release exports write.
    pub export_location: ExportLocation,
    /// Template for the default filename a single-track export suggests.
    pub export_filename_template: String,
    /// Configured export presets offered by release and track export.
    pub export_presets: Vec<ExportPreset>,
    /// Default selected option in the track export picker.
    pub default_track_export_selection: ExportSelection,
    /// Default selected option in the release export picker.
    pub default_release_export_selection: ExportSelection,
    /// Whether playback pauses between vinyl/cassette sides.
    pub pause_between_sides: bool,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed.
    pub show_remaining_time: bool,
    /// Whether import fully decodes each track to verify it (fatal-error / frame
    /// shortfall), failing the import for a broken track rather than importing it
    /// and failing at play time. Rides the loudness decode, so it adds no work.
    pub verify_decode_on_import: bool,
    /// Local automation server configuration.
    pub mcp: McpConfig,
    /// Cloud home provider + per-provider settings. Flattened so the on-disk
    /// keys sit at the top level.
    #[serde(flatten)]
    pub cloud_home: CloudHomeConfig,
}

impl ConfigYaml {
    /// Convert to a runtime Config. The caller resolves device_id (auto-generating
    /// if missing from YAML) and provides the library_dir.
    fn into_config(self, device_id: String, library_dir: StoreDir) -> Config {
        // `version` describes the file, not the running config: it is re-stamped
        // from CONFIG_VERSION on every write, so it does not ride on `Config`.
        Config {
            inner: coven::Config {
                store_id: self.library_id,
                device_id,
                store_dir: library_dir,
                store_name: self.library_name,
                encryption_key_stored: self.encryption_key_stored,
                encryption_key_fingerprint: self.encryption_key_fingerprint,
                cloud_home: self.cloud_home,
            },
            discogs: self.discogs,
            replay_gain_mode: self.replay_gain_mode,
            export_location: self.export_location,
            export_filename_template: self.export_filename_template,
            export_presets: self.export_presets,
            default_track_export_selection: self.default_track_export_selection,
            default_release_export_selection: self.default_release_export_selection,
            pause_between_sides: self.pause_between_sides,
            show_remaining_time: self.show_remaining_time,
            verify_decode_on_import: self.verify_decode_on_import,
            mcp: self.mcp,
        }
    }
}

impl From<&Config> for ConfigYaml {
    fn from(config: &Config) -> Self {
        Self {
            version: CONFIG_VERSION,
            library_id: config.store_id.clone(),
            library_name: config.store_name.clone(),
            device_id: Some(config.device_id.clone()),
            discogs: config.discogs,
            encryption_key_stored: config.encryption_key_stored,
            encryption_key_fingerprint: config.encryption_key_fingerprint.clone(),
            replay_gain_mode: config.replay_gain_mode,
            export_location: config.export_location.clone(),
            export_filename_template: config.export_filename_template.clone(),
            export_presets: config.export_presets.clone(),
            default_track_export_selection: config.default_track_export_selection.clone(),
            default_release_export_selection: config.default_release_export_selection.clone(),
            pause_between_sides: config.pause_between_sides,
            show_remaining_time: config.show_remaining_time,
            verify_decode_on_import: config.verify_decode_on_import,
            mcp: config.mcp,
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
    /// silently vanish from the picker, which is what used to happen the moment a
    /// new required field was added. Its `id` and `name` fall back to the
    /// directory name, because the name is exactly what could not be read.
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
    /// The stored Discogs key's validation state, or `None` when no key is
    /// configured. `Some` doubles as the hint that a key is in the keyring, so
    /// settings render without a keyring read.
    pub discogs: Option<DiscogsValidation>,
    /// How loudness normalization is applied at playback. Defaults to `Off`.
    pub replay_gain_mode: ReplayGainMode,
    /// Where release exports write. Defaults to prompting each time.
    pub export_location: ExportLocation,
    /// Template for the default filename a single-track export suggests.
    pub export_filename_template: String,
    /// Configured export presets offered by release and track export.
    pub export_presets: Vec<ExportPreset>,
    /// Default selected option in the track export picker.
    pub default_track_export_selection: ExportSelection,
    /// Default selected option in the release export picker.
    pub default_release_export_selection: ExportSelection,
    /// Whether playback pauses between vinyl/cassette sides.
    pub pause_between_sides: bool,
    /// Whether the seek bar's leading label counts down the time remaining
    /// instead of showing the time elapsed. Defaults to `false` (elapsed). A
    /// preference like any other, so it follows the user to every device rather
    /// than living in each platform's own store.
    pub show_remaining_time: bool,
    /// Whether import verifies each track by fully decoding it, failing the import
    /// for a broken (truncated/corrupt) track. Defaults to `true`.
    pub verify_decode_on_import: bool,
    /// Local automation server configuration. The bearer token is keyring-only.
    pub mcp: McpConfig,
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
    pub fn load_registered_library(
        library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let bae_dir = bae_dir()?;
        Self::load_registered_library_from_bae_dir(&bae_dir, library_id, ids)
    }

    pub fn load_from_library_path(
        library_path: PathBuf,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        Self::load_from_library_dir(StoreDir::new(library_path), ids)
    }

    pub(crate) fn load_registered_library_from_bae_dir(
        bae_dir: &std::path::Path,
        library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let library_dir = registered_library_dir(bae_dir, library_id);
        Self::load_from_registered_library_dir(library_dir, library_id, ids)
    }

    fn load_from_library_dir(
        library_dir: StoreDir,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let config_path = library_dir.config_path();
        let (yaml_config, migrated) = Self::load_config_yaml(&config_path)?;
        Self::config_from_yaml(yaml_config, migrated, library_dir, &config_path, ids)
    }

    fn load_from_registered_library_dir(
        library_dir: StoreDir,
        expected_library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let config_path = library_dir.config_path();
        let (yaml_config, migrated) =
            load_registered_config_yaml(&library_dir, expected_library_id)?;
        Self::config_from_yaml(yaml_config, migrated, library_dir, &config_path, ids)
    }

    /// Read config.yaml, upgrading an older schema in the process. The `bool` is
    /// whether it was migrated; the caller writes a migrated config back.
    fn load_config_yaml(config_path: &std::path::Path) -> Result<(ConfigYaml, bool), ConfigError> {
        let content = std::fs::read_to_string(config_path)?;
        parse_config_yaml(&content)
    }

    /// `migrated` is whether the file was upgraded from an older schema on the way
    /// in. Either that or a freshly generated `device_id` means what is on disk no
    /// longer matches what we hold, so it is written back — once, here, rather
    /// than on every read.
    fn config_from_yaml(
        mut yaml_config: ConfigYaml,
        migrated: bool,
        library_dir: StoreDir,
        config_path: &std::path::Path,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let mut dirty = migrated;
        let device_id = match yaml_config.device_id.clone() {
            Some(id) => id,
            None => {
                let id = ids.new_id();
                info!("No device_id in config.yaml, generated: {}", id);
                yaml_config.device_id = Some(id.clone());
                dirty = true;
                id
            }
        };
        if dirty {
            let serialized = serde_yaml::to_string(&yaml_config)
                .map_err(|e| ConfigError::Serialization(e.to_string()))?;
            write_atomic_io(config_path, serialized.as_bytes())?;
        }
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
        write_atomic_io(&pointer_path, self.store_id.as_bytes())?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        self.write_config_yaml().map_err(WriteError::into_inner)
    }

    fn write_config_yaml(&self) -> Result<(), WriteError<ConfigError>> {
        std::fs::create_dir_all(&*self.store_dir)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::from(e)))?;
        let yaml: ConfigYaml = self.into();
        let serialized = serde_yaml::to_string(&yaml)
            .map_err(|e| WriteError::BeforeCommit(ConfigError::Serialization(e.to_string())))?;
        write_atomic(&self.store_dir.config_path(), serialized.as_bytes())
            .map_err(|e| e.map(ConfigError::from))
    }

    /// Construct a Config with defaults for a new library.
    pub fn with_defaults(
        library_id: String,
        device_id: String,
        library_dir: StoreDir,
        library_name: String,
    ) -> Self {
        Self {
            inner: coven::Config::with_defaults(library_id, device_id, library_dir, library_name),
            discogs: None,
            replay_gain_mode: ReplayGainMode::Off,
            export_location: ExportLocation::AskEachTime,
            export_filename_template: default_export_filename_template(),
            export_presets: default_export_presets(),
            default_track_export_selection: ExportSelection::Original,
            default_release_export_selection: ExportSelection::Original,
            pause_between_sides: false,
            show_remaining_time: false,
            verify_decode_on_import: true,
            mcp: McpConfig::disabled_default(),
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

/// Reactive config state for the running app — the single source of truth.
///
/// Holds the live `Config` in a `watch` channel: readers borrow the current
/// value via `config()`, and subscribers receive the whole latest `Config` on
/// every change via `subscribe()`. Every mutation goes through `update`, which
/// edits the value, persists it to disk, and publishes it — so the UI reacts
/// without polling, re-reading, or a restart.
pub struct ConfigHandle {
    state: watch::Sender<Config>,
}

impl ConfigHandle {
    pub fn new(config: Config) -> Self {
        let (state, _) = watch::channel(config);
        Self { state }
    }

    /// Borrow the current config.
    pub fn config(&self) -> watch::Ref<'_, Config> {
        self.state.borrow()
    }

    /// Subscribe to the config-state stream. Each change yields the whole latest
    /// `Config`; the channel coalesces to the most recent value.
    pub fn subscribe(&self) -> watch::Receiver<Config> {
        self.state.subscribe()
    }

    /// Edit the config, persist it to disk, and publish the new state to
    /// subscribers. The single write path for every config change.
    pub fn update(&self, edit: impl FnOnce(&mut Config)) -> Result<(), ConfigError> {
        let mut save_err = None;
        self.state.send_if_modified(|config| {
            let mut edited = config.clone();
            edit(&mut edited);
            match edited.write_config_yaml() {
                Ok(()) => {
                    *config = edited;
                    true
                }
                Err(e) => {
                    let committed = e.committed();
                    save_err = Some(e.into_inner());
                    if committed {
                        *config = edited;
                    }
                    committed
                }
            }
        });
        match save_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub fn has_discogs_key(&self) -> bool {
        self.config().discogs.is_some()
    }

    /// Record that an encryption key now exists, with its fingerprint. Used
    /// after creating the key on first sync setup.
    pub fn record_encryption_key_fingerprint(
        &self,
        fingerprint: String,
    ) -> Result<(), ConfigError> {
        self.update(|c| {
            c.encryption_key_stored = true;
            c.encryption_key_fingerprint = Some(fingerprint);
        })
    }

    /// Rename the library. The name is already validated non-blank by its type.
    pub fn rename_library(
        &self,
        name: &crate::library_name::LibraryName,
    ) -> Result<(), ConfigError> {
        self.update(|c| c.store_name = name.as_str().to_string())
    }
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
    let config_path = library_dir.config_path();
    let content = std::fs::read_to_string(&config_path)?;
    let mut yaml: ConfigYaml =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    yaml.library_name = new_name.as_str().to_string();
    let serialized =
        serde_yaml::to_string(&yaml).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    write_atomic_io(&config_path, serialized.as_bytes())?;
    Ok(())
}

pub(crate) fn registered_library_path(bae_dir: &std::path::Path, library_id: &str) -> PathBuf {
    bae_dir.join("libraries").join(library_id)
}

fn registered_library_dir(bae_dir: &std::path::Path, library_id: &str) -> StoreDir {
    StoreDir::new(registered_library_path(bae_dir, library_id))
}

fn load_registered_config_yaml(
    library_dir: &StoreDir,
    expected_library_id: &str,
) -> Result<(ConfigYaml, bool), ConfigError> {
    let (yaml_config, migrated) = Config::load_config_yaml(&library_dir.config_path())?;
    if yaml_config.library_id != expected_library_id {
        return Err(ConfigError::Config(format!(
            "registered library directory {} contains library_id {}",
            library_dir.display(),
            yaml_config.library_id
        )));
    }
    Ok((yaml_config, migrated))
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
fn find_library_by_id(bae_dir: &std::path::Path, uuid: &str) -> Option<StoreDir> {
    for (path, yaml) in discover_all_library_paths(bae_dir) {
        // A library whose config will not parse cannot be addressed by id — its id
        // is precisely what we could not read.
        if yaml.is_ok_and(|yaml| yaml.library_id == uuid) {
            return Some(StoreDir::new(path));
        }
    }
    None
}

/// Collect every library directory under ~/.bae/libraries/ with the outcome of
/// reading its config — `Err` for one that cannot be read even after migration.
///
/// The failure is CARRIED, not dropped. Swallowing it here is what made a library
/// whose config predated a new field disappear from the picker entirely.
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
    // Discovery migrates in memory only: listing libraries must not write to disk.
    // The upgrade is persisted when the library is actually opened.
    let (yaml, _migrated) = parse_config_yaml(&content)?;
    Ok(Some(yaml))
}

/// The on-disk defaults an older config's missing keys are filled from — a
/// freshly-defaulted config, serialized.
///
/// This is why there is no per-field default table: [`Config::with_defaults`]
/// already states every default, so the migration reads them from there and
/// cannot fall out of step with it.
///
/// Identity is deliberately *not* defaultable: `library_id` and `library_name`
/// are stripped, so a file missing them still fails as corrupt instead of being
/// handed an invented identity. `device_id` is stripped too — it has its own
/// generate-and-write-back path in [`Config::config_from_yaml`].
fn config_yaml_defaults() -> serde_yaml::Mapping {
    let template = Config::with_defaults(
        String::new(),
        String::new(),
        StoreDir::new(PathBuf::new()),
        String::new(),
    );
    let value = serde_yaml::to_value(ConfigYaml::from(&template))
        .expect("a default ConfigYaml always serializes");
    let mut map = match value {
        serde_yaml::Value::Mapping(map) => map,
        other => panic!("ConfigYaml serializes to a mapping, got {other:?}"),
    };
    for identity in ["library_id", "library_name", "device_id"] {
        map.remove(serde_yaml::Value::String(identity.to_string()));
    }
    map
}

/// Fill every key `map` lacks from the defaults, and stamp it at the current
/// version. Returns the keys that were added, for the log line.
fn migrate_to_current(map: &mut serde_yaml::Mapping) -> Vec<String> {
    let mut added = Vec::new();
    for (key, default) in config_yaml_defaults() {
        if !map.contains_key(&key) {
            if let serde_yaml::Value::String(name) = &key {
                added.push(name.clone());
            }
            map.insert(key, default);
        }
    }
    map.insert(
        serde_yaml::Value::String("version".to_string()),
        serde_yaml::Value::Number(CONFIG_VERSION.into()),
    );
    added
}

/// Parse a config.yaml, upgrading it from an older schema if that is what it is.
///
/// Returns the config and whether it was migrated — the caller persists a
/// migrated config, so the upgrade happens once rather than on every read.
///
/// The three cases are kept apart on purpose:
/// - **older** (`version` below current, or absent entirely): the fields added
///   since are filled from [`config_yaml_defaults`] and the file is re-stamped.
/// - **current**: parsed strictly. A key missing *here* is corruption, not age,
///   and still fails loudly.
/// - **newer**: refused. This build cannot know what a future field means, and
///   guessing would silently drop whatever the newer bae stored.
fn parse_config_yaml(content: &str) -> Result<(ConfigYaml, bool), ConfigError> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    let mut map = match value {
        serde_yaml::Value::Mapping(map) => map,
        _ => {
            return Err(ConfigError::Serialization(
                "config.yaml is not a mapping".to_string(),
            ))
        }
    };

    let file_version = match map.get(serde_yaml::Value::String("version".to_string())) {
        Some(serde_yaml::Value::Number(n)) => n
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| {
                ConfigError::Serialization(format!("config.yaml has a nonsensical version: {n}"))
            })?,
        Some(other) => {
            return Err(ConfigError::Serialization(format!(
                "config.yaml `version` must be a number, found {other:?}"
            )))
        }
        None => UNVERSIONED,
    };

    if file_version > CONFIG_VERSION {
        return Err(ConfigError::Config(format!(
            "config.yaml was written by a newer version of bae (config schema v{file_version}; \
             this build reads v{CONFIG_VERSION}). Upgrade bae to open this library."
        )));
    }

    let migrated = file_version < CONFIG_VERSION;
    if migrated {
        let added = migrate_to_current(&mut map);
        info!(
            "upgrading config.yaml from schema v{file_version} to v{CONFIG_VERSION}: \
             filled {} field(s) with defaults [{}]",
            added.len(),
            added.join(", "),
        );
    }

    let yaml = serde_yaml::from_value(serde_yaml::Value::Mapping(map))
        .map_err(|e| ConfigError::Serialization(e.to_string()))?;
    Ok((yaml, migrated))
}

fn read_optional_file(path: &std::path::Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::{Arc, Barrier};
    use std::time::Duration;
    use tempfile::TempDir;

    fn make_test_config(library_id: &str, library_path: PathBuf) -> Config {
        Config::with_defaults(
            library_id.to_string(),
            "test-device-id".to_string(),
            StoreDir::new(library_path),
            "Test Library".to_string(),
        )
    }

    /// A full `ConfigYaml` serialized to a `serde_yaml::Value` mapping, for
    /// tests that assert a single missing key fails the load.
    fn full_config_yaml_value() -> serde_yaml::Value {
        let config = make_test_config("abc-123", PathBuf::from("unused"));
        serde_yaml::to_value(ConfigYaml::from(&config)).unwrap()
    }

    /// Parse a full config with one top-level key removed.
    fn parse_yaml_without(key: &str) -> Result<ConfigYaml, serde_yaml::Error> {
        let mut value = full_config_yaml_value();
        let map = value.as_mapping_mut().unwrap();
        map.remove(serde_yaml::Value::String(key.to_string()))
            .unwrap_or_else(|| panic!("{key} not in serialized config"));
        serde_yaml::from_value(value)
    }

    #[test]
    fn export_location_defaults_to_ask_each_time() {
        let tmp = TempDir::new().unwrap();
        let config = make_test_config("lib", tmp.path().to_path_buf());
        assert_eq!(config.export_location, ExportLocation::AskEachTime);
    }

    #[test]
    fn export_location_roundtrips_both_variants() {
        for location in [
            ExportLocation::AskEachTime,
            ExportLocation::Fixed(PathBuf::from("/exports/music")),
        ] {
            let tmp = TempDir::new().unwrap();
            let mut config = make_test_config("lib", tmp.path().to_path_buf());
            config.export_location = location.clone();
            config.save_to_config_yaml().unwrap();

            let yaml: ConfigYaml = serde_yaml::from_str(
                &std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap(),
            )
            .unwrap();
            assert_eq!(yaml.export_location, location);
        }
    }

    #[test]
    fn export_settings_survive_yaml_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut config = make_test_config("lib", tmp.path().to_path_buf());
        config.export_filename_template = "{artist} - {title}".to_string();
        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml =
            serde_yaml::from_str(&std::fs::read_to_string(tmp.path().join("config.yaml")).unwrap())
                .unwrap();
        assert_eq!(yaml.export_filename_template, "{artist} - {title}");
        assert_eq!(yaml.export_presets, config.export_presets);
        assert_eq!(
            yaml.default_track_export_selection,
            config.default_track_export_selection
        );
        assert_eq!(
            yaml.default_release_export_selection,
            config.default_release_export_selection
        );
    }

    #[test]
    fn config_yaml_requires_library_id() {
        assert!(
            parse_yaml_without("library_id").is_err(),
            "ConfigYaml should fail without library_id"
        );
    }

    #[test]
    fn config_yaml_requires_mcp() {
        assert!(
            parse_yaml_without("mcp").is_err(),
            "ConfigYaml should fail without mcp"
        );
    }

    /// Every bae-local field except `device_id` is serialized unconditionally, so
    /// at the current `CONFIG_VERSION` a missing key means a corrupt or foreign
    /// file — the struct still refuses it rather than taking an implicit default.
    /// An *older* file is a different thing entirely and is upgraded before it
    /// reaches here (see the migration tests below).
    #[test]
    fn config_yaml_requires_every_bae_field() {
        for key in [
            "discogs",
            "encryption_key_stored",
            "encryption_key_fingerprint",
            "replay_gain_mode",
            "export_location",
            "export_filename_template",
            "export_presets",
            "default_track_export_selection",
            "default_release_export_selection",
            "pause_between_sides",
            "show_remaining_time",
            "verify_decode_on_import",
        ] {
            assert!(
                parse_yaml_without(key).is_err(),
                "ConfigYaml should fail without {key}"
            );
        }
    }

    // ── Config schema migration ──────────────────────────────────────────────
    //
    // The bug these pin: a config.yaml written before a field existed used to
    // fail the load, and `discover_all_library_paths` then swallowed the error —
    // so the library silently vanished from the picker.

    /// A config.yaml as an older bae wrote it: no `version` key, and missing a
    /// field a newer build requires. `set` overrides a value the user had chosen,
    /// so we can prove the migration preserves it.
    fn unversioned_yaml_without(missing: &str, set: &[(&str, serde_yaml::Value)]) -> String {
        let mut value = full_config_yaml_value();
        let map = value.as_mapping_mut().unwrap();
        map.remove(serde_yaml::Value::String("version".to_string()));
        map.remove(serde_yaml::Value::String(missing.to_string()))
            .unwrap_or_else(|| panic!("{missing} not in serialized config"));
        for (key, v) in set {
            map.insert(serde_yaml::Value::String(key.to_string()), v.clone());
        }
        serde_yaml::to_string(&value).unwrap()
    }

    /// The regression: an old config loads instead of failing, gains the default
    /// for the field it never had, and keeps every value the user actually set.
    #[test]
    fn an_older_config_migrates_instead_of_failing() {
        let yaml = unversioned_yaml_without(
            "verify_decode_on_import",
            &[
                ("pause_between_sides", serde_yaml::Value::Bool(true)),
                (
                    "library_name",
                    serde_yaml::Value::String("My Music".to_string()),
                ),
            ],
        );

        let (config, migrated) = parse_config_yaml(&yaml).expect("an older config still loads");

        assert!(migrated, "it should be reported as migrated");
        assert_eq!(config.version, CONFIG_VERSION);
        // The field the old file never had takes the documented default.
        assert!(config.verify_decode_on_import);
        // Everything the user had set survives.
        assert!(config.pause_between_sides);
        assert_eq!(config.library_name, "My Music");
        assert_eq!(config.library_id, "abc-123");
    }

    /// `Config::with_defaults` is the single source of a field's default, so the
    /// migration must cover *every* field from it — including the next one added.
    #[test]
    fn migration_fills_any_field_an_older_config_lacks() {
        for key in [
            "discogs",
            "encryption_key_stored",
            "encryption_key_fingerprint",
            "replay_gain_mode",
            "export_location",
            "export_filename_template",
            "export_presets",
            "default_track_export_selection",
            "default_release_export_selection",
            "pause_between_sides",
            "show_remaining_time",
            "verify_decode_on_import",
            "mcp",
        ] {
            let yaml = unversioned_yaml_without(key, &[]);
            let (config, migrated) = parse_config_yaml(&yaml)
                .unwrap_or_else(|e| panic!("an older config missing `{key}` should migrate: {e}"));
            assert!(migrated, "missing `{key}` should migrate");
            assert_eq!(config.version, CONFIG_VERSION);
        }
    }

    /// Identity is never invented: a config with no `library_id` is corrupt, not
    /// old, and must not be handed a default one.
    #[test]
    fn migration_never_defaults_the_library_identity() {
        let yaml = unversioned_yaml_without("library_id", &[]);
        assert!(
            parse_config_yaml(&yaml).is_err(),
            "a config without library_id must fail, not be given an invented one"
        );
    }

    /// Strictness is retained where it means something: at the current version a
    /// missing key is corruption, and is not quietly filled in.
    #[test]
    fn a_current_version_config_missing_a_field_still_fails() {
        let mut value = full_config_yaml_value();
        let map = value.as_mapping_mut().unwrap();
        // Keep `version` — this file claims to be current.
        map.remove(serde_yaml::Value::String(
            "verify_decode_on_import".to_string(),
        ))
        .unwrap();
        let yaml = serde_yaml::to_string(&value).unwrap();

        assert!(
            parse_config_yaml(&yaml).is_err(),
            "a corrupt current-version config must still fail loudly"
        );
    }

    /// A config from a future bae is refused rather than guessed at.
    #[test]
    fn a_newer_config_is_refused() {
        let mut value = full_config_yaml_value();
        value.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::String("version".to_string()),
            serde_yaml::Value::Number((CONFIG_VERSION + 1).into()),
        );
        let yaml = serde_yaml::to_string(&value).unwrap();

        let err = parse_config_yaml(&yaml).expect_err("a newer config must be refused");
        assert!(
            err.to_string().contains("newer version of bae"),
            "the error should say why: {err}"
        );
    }

    /// A config the current build already understands is left alone.
    #[test]
    fn a_current_config_is_not_migrated() {
        let yaml = serde_yaml::to_string(&full_config_yaml_value()).unwrap();
        let (_, migrated) = parse_config_yaml(&yaml).expect("a current config loads");
        assert!(!migrated, "a current config needs no rewrite");
    }

    /// THE regression. A library whose config.yaml predates a required field used
    /// to vanish from the picker: the load failed (correctly) and discovery then
    /// swallowed the error and dropped the row. It must stay listed, and be
    /// openable.
    #[test]
    #[serial]
    fn an_older_library_stays_in_the_picker() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_dir = bae_dir.join("libraries").join("lib-old");
        std::fs::create_dir_all(&library_dir).unwrap();

        // Write a config as an older bae would have: no `version`, and without a
        // field this build requires. The user had set a name and a preference.
        let yaml = unversioned_yaml_without(
            "verify_decode_on_import",
            &[
                (
                    "library_id",
                    serde_yaml::Value::String("lib-old".to_string()),
                ),
                (
                    "library_name",
                    serde_yaml::Value::String("Old Library".to_string()),
                ),
                ("pause_between_sides", serde_yaml::Value::Bool(true)),
            ],
        );
        std::fs::write(library_dir.join("config.yaml"), &yaml).unwrap();

        let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

        assert_eq!(libraries.len(), 1, "the old library must still be listed");
        assert_eq!(libraries[0].id, "lib-old");
        assert_eq!(libraries[0].name, "Old Library");
        assert_eq!(
            libraries[0].error, None,
            "an old config is not a broken one"
        );

        // And it actually opens, keeping what the user had set and taking the
        // default for the field it never had.
        let config = Config::load_from_library_path(library_dir.clone(), &coven::UuidProvider)
            .expect("the old library opens");
        assert_eq!(config.store_name, "Old Library");
        assert!(config.pause_between_sides);
        assert!(config.verify_decode_on_import);

        // The upgrade is persisted, so it happens once rather than on every read.
        let on_disk = std::fs::read_to_string(library_dir.join("config.yaml")).unwrap();
        let (reparsed, migrated_again) = parse_config_yaml(&on_disk).unwrap();
        assert!(!migrated_again, "the upgrade should have been written back");
        assert_eq!(reparsed.version, CONFIG_VERSION);
    }

    /// A config that is genuinely unreadable is SHOWN as broken, not skipped. The
    /// user must be able to see that the library is there and in trouble.
    #[test]
    #[serial]
    fn a_broken_library_is_listed_as_broken() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_dir = bae_dir.join("libraries").join("lib-broken");
        std::fs::create_dir_all(&library_dir).unwrap();
        std::fs::write(library_dir.join("config.yaml"), "{ this is not: [valid").unwrap();

        let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

        assert_eq!(libraries.len(), 1, "a broken library must not disappear");
        let broken = &libraries[0];
        assert!(broken.error.is_some(), "it must be marked broken");
        // Its name is unreadable — that is the failure — so the directory stands in.
        assert_eq!(broken.id, "lib-broken");
        assert_eq!(broken.name, "lib-broken");
    }

    /// A working library and a broken one coexist: the broken one sorts last but
    /// is still there.
    #[test]
    #[serial]
    fn a_broken_library_does_not_hide_a_working_one() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        let good_dir = libraries_dir.join("lib-good");
        std::fs::create_dir_all(&good_dir).unwrap();
        let mut good = make_test_config("lib-good", good_dir.clone());
        good.store_name = "Good".to_string();
        good.save_to_config_yaml().unwrap();

        let broken_dir = libraries_dir.join("lib-broken");
        std::fs::create_dir_all(&broken_dir).unwrap();
        std::fs::write(broken_dir.join("config.yaml"), "{ nope: [").unwrap();

        let libraries = discover_libraries_from_bae_dir(bae_dir).unwrap();

        assert_eq!(libraries.len(), 2);
        assert_eq!(libraries[0].name, "Good");
        assert!(libraries[0].error.is_none());
        assert_eq!(libraries[1].id, "lib-broken");
        assert!(libraries[1].error.is_some());
    }

    /// `device_id` is the one designed absence: missing on a fresh library, and
    /// auto-generated (and written back) on first load rather than failing.
    #[test]
    fn config_yaml_allows_missing_device_id() {
        let config = parse_yaml_without("device_id").unwrap();
        assert_eq!(config.device_id, None);
    }

    /// `is_usable` is the single source of truth for whether Discogs can be a
    /// metadata source: a stored key is usable optimistically unless rejected.
    #[test]
    fn discogs_token_status_usability() {
        assert!(DiscogsTokenStatus::Valid.is_usable());
        assert!(DiscogsTokenStatus::Unvalidated.is_usable());
        assert!(!DiscogsTokenStatus::Rejected.is_usable());
        assert!(!DiscogsTokenStatus::NotConfigured.is_usable());
    }

    /// `discogs_token_status` derives `NotConfigured` from `None` and maps the
    /// inner validation otherwise — no key means not configured, with no
    /// sentinel validation standing in.
    #[test]
    fn discogs_token_status_derives_from_option() {
        let tmp = TempDir::new().unwrap();
        let mut config = make_test_config("lib-discogs", tmp.path().to_path_buf());

        assert!(config.discogs.is_none());
        assert!(matches!(
            config.discogs_token_status(),
            DiscogsTokenStatus::NotConfigured
        ));

        config.discogs = Some(DiscogsValidation::Unvalidated);
        assert!(matches!(
            config.discogs_token_status(),
            DiscogsTokenStatus::Unvalidated
        ));

        config.discogs = Some(DiscogsValidation::Rejected);
        assert!(matches!(
            config.discogs_token_status(),
            DiscogsTokenStatus::Rejected
        ));
    }

    #[test]
    #[serial]
    fn dev_mode_uses_dotenv_search_for_parent_env_file() {
        let original_cwd = std::env::current_dir().unwrap();
        let original_dev_mode = std::env::var_os("BAE_DEV_MODE");
        let original_parent_dotenv = std::env::var_os("BAE_TEST_PARENT_DOTENV");

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".env"), "BAE_TEST_PARENT_DOTENV=1\n").unwrap();
        let child = tmp.path().join("child");
        std::fs::create_dir(&child).unwrap();

        std::env::remove_var("BAE_DEV_MODE");
        std::env::remove_var("BAE_TEST_PARENT_DOTENV");
        std::env::set_current_dir(&child).unwrap();
        let is_dev_mode = Config::is_dev_mode();

        std::env::set_current_dir(original_cwd).unwrap();
        match original_dev_mode {
            Some(value) => std::env::set_var("BAE_DEV_MODE", value),
            None => std::env::remove_var("BAE_DEV_MODE"),
        }
        match original_parent_dotenv {
            Some(value) => std::env::set_var("BAE_TEST_PARENT_DOTENV", value),
            None => std::env::remove_var("BAE_TEST_PARENT_DOTENV"),
        }

        assert!(is_dev_mode);
    }

    #[test]
    fn config_yaml_requires_storage() {
        // `storage` rides the flattened coven CloudHomeConfig and carries no
        // serde default: a config file without it fails to load rather than
        // silently assuming a cipher/path scheme.
        assert!(
            parse_yaml_without("storage").is_err(),
            "ConfigYaml should fail without storage"
        );
    }

    #[test]
    fn save_and_load_config_yaml_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let config = make_test_config("my-library-id", library_path.clone());

        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_id, "my-library-id");
        assert_eq!(yaml.mcp, McpConfig::disabled_default());
    }

    #[test]
    fn load_from_library_path_reads_exact_directory() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("external-library");
        make_test_config("external-lib-id", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let loaded = Config::load_from_library_path(
            library_path.clone(),
            &coven::SequentialIdProvider::new("device"),
        )
        .unwrap();

        assert_eq!(loaded.store_id, "external-lib-id");
        assert_eq!(&*loaded.store_dir, library_path.as_path());
    }

    #[test]
    fn load_from_registered_library_dir_rejects_mismatched_config_id() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("libraries").join("expected-lib-id");
        make_test_config("wrong-lib-id", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let result = Config::load_from_registered_library_dir(
            StoreDir::new(library_path),
            "expected-lib-id",
            &coven::SequentialIdProvider::new("device"),
        );

        assert!(matches!(result, Err(ConfigError::Config(_))));
    }

    #[test]
    fn read_active_library_id_errors_when_pointer_is_empty() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("active-library"), " \n").unwrap();

        let err = read_active_library_id(tmp.path()).unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("active-library pointer"));
    }

    #[test]
    fn discover_libraries_from_bae_dir_returns_active_pointer_read_error() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = registered_library_path(bae_dir, "auto-lib");

        make_test_config("auto-lib", library_path)
            .save_to_config_yaml()
            .unwrap();
        std::fs::create_dir(bae_dir.join("active-library")).unwrap();

        assert!(discover_libraries_from_bae_dir(bae_dir).is_err());
    }

    /// A library dir whose name isn't valid UTF-8 can't round-trip through the
    /// `String` paths the app addresses files by, so discovery skips it (rather
    /// than panicking or lossily mangling the path) and still finds the valid
    /// siblings.
    ///
    /// Unix-only, and even there only on a filesystem that accepts non-UTF-8
    /// names: APFS/HFS+ reject the raw byte at the syscall (EILSEQ), so the
    /// directory can't exist and the skip branch is unreachable — in that case
    /// the test has nothing to exercise and returns after confirming the
    /// filesystem refused the name.
    #[cfg(unix)]
    #[test]
    fn discovery_skips_non_utf8_library_dir() {
        use std::os::unix::ffi::OsStrExt;

        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");
        std::fs::create_dir_all(&libraries_dir).unwrap();

        // A valid library: UTF-8 dir name + config.yaml.
        let library_path = libraries_dir.join("valid-lib");
        make_test_config("valid-lib", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        // A sibling dir whose name is not valid UTF-8 (a lone 0xFF byte). On a
        // filesystem that rejects such names there's nothing to skip — the
        // discovery is then trivially correct and the rest of the test moot.
        let bad_name = std::ffi::OsStr::from_bytes(b"bad-\xff-name");
        if std::fs::create_dir(libraries_dir.join(bad_name)).is_err() {
            return;
        }

        let discovered = discover_all_library_paths(bae_dir);
        assert_eq!(discovered.len(), 1, "non-UTF-8 dir should be skipped");
        assert_eq!(discovered[0].1.as_ref().unwrap().library_id, "valid-lib");
    }

    #[test]
    fn library_name_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let mut config = make_test_config("lib-1", library_path.clone());
        config.store_name = "My Music".to_string();
        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_name, "My Music");
    }

    #[test]
    fn discover_libraries_finds_dirs_with_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        // Create two libraries
        let lib1_path = libraries_dir.join("lib-1");
        make_test_config("lib-1", lib1_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let lib2_path = libraries_dir.join("lib-2");
        let mut lib2 = make_test_config("lib-2", lib2_path.clone());
        lib2.store_name = "Second Library".to_string();
        lib2.encryption_key_fingerprint = Some("fingerprint-2".to_string());
        lib2.save_to_config_yaml().unwrap();

        // Create an invalid dir (no config.yaml)
        std::fs::create_dir_all(libraries_dir.join("invalid")).unwrap();

        let discovered = discover_all_library_paths(bae_dir);
        assert_eq!(discovered.len(), 2);

        let ids: Vec<&str> = discovered
            .iter()
            .map(|(_, y)| y.as_ref().unwrap().library_id.as_str())
            .collect();
        assert!(ids.contains(&"lib-1"));
        assert!(ids.contains(&"lib-2"));

        let lib2_entry = discovered
            .iter()
            .find(|(_, y)| y.as_ref().unwrap().library_id == "lib-2")
            .unwrap();
        let lib2_yaml = lib2_entry.1.as_ref().unwrap();
        assert_eq!(lib2_yaml.library_name, "Second Library");
        assert_eq!(
            lib2_yaml.encryption_key_fingerprint.as_deref(),
            Some("fingerprint-2")
        );
    }

    #[test]
    fn find_library_by_id_scans_libraries_dir() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let libraries_dir = bae_dir.join("libraries");

        let lib1_path = libraries_dir.join("lib-1");
        make_test_config("lib-1", lib1_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let lib2_path = libraries_dir.join("lib-2");
        make_test_config("lib-2", lib2_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let found = find_library_by_id(bae_dir, "lib-1");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), lib1_path.as_path());

        let found = find_library_by_id(bae_dir, "lib-2");
        assert!(found.is_some());
        assert_eq!(&*found.unwrap(), lib2_path.as_path());

        assert!(find_library_by_id(bae_dir, "nonexistent").is_none());
    }

    #[test]
    fn rename_library_updates_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let config = make_test_config("lib-1", library_path.clone());
        config.save_to_config_yaml().unwrap();
        let handle = ConfigHandle::new(config);

        handle
            .rename_library(&crate::library_name::LibraryName::parse("New Name").unwrap())
            .unwrap();
        assert_eq!(handle.config().store_name, "New Name");

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_name, "New Name");
        assert_eq!(yaml.library_id, "lib-1"); // unchanged
    }

    /// An `update` is reflected by the `Config` that `config()` returns — the
    /// same `Config` the bridge reads to build the UI's Discogs token status. If
    /// a write only reached an on-disk copy or a side cache, the bridge would
    /// keep reporting "not configured" until the next load.
    #[test]
    fn update_is_reflected_by_config() {
        let tmp = TempDir::new().unwrap();
        let config = make_test_config("lib-update", tmp.path().to_path_buf());
        config.save_to_config_yaml().unwrap();
        let handle = ConfigHandle::new(config);

        assert!(handle.config().discogs.is_none());
        handle
            .update(|c| c.discogs = Some(DiscogsValidation::Valid))
            .unwrap();
        assert_eq!(handle.config().discogs, Some(DiscogsValidation::Valid));
    }

    #[test]
    fn update_serializes_concurrent_edits() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let config = make_test_config("lib-update-race", library_path.clone());
        config.save_to_config_yaml().unwrap();
        let handle = Arc::new(ConfigHandle::new(config));
        let start = Arc::new(Barrier::new(3));

        fn spawn_update(
            handle: Arc<ConfigHandle>,
            start: Arc<Barrier>,
            edit: impl FnOnce(&mut Config) + Send + 'static,
        ) -> std::thread::JoinHandle<()> {
            std::thread::spawn(move || {
                start.wait();
                handle
                    .update(|config| {
                        std::thread::sleep(Duration::from_millis(100));
                        edit(config);
                    })
                    .unwrap();
            })
        }

        let rename = spawn_update(Arc::clone(&handle), Arc::clone(&start), |config| {
            config.store_name = "Renamed Library".to_string();
        });
        let playback = spawn_update(Arc::clone(&handle), Arc::clone(&start), |config| {
            config.pause_between_sides = true;
        });

        start.wait();
        rename.join().unwrap();
        playback.join().unwrap();

        let final_config = handle.config().clone();
        assert_eq!(final_config.store_name, "Renamed Library");
        assert!(final_config.pause_between_sides);

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_name, "Renamed Library");
        assert!(yaml.pause_between_sides);
    }

    #[test]
    fn from_coven_preserves_library_id_and_persists_bae_yaml() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("libraries").join("restored-lib-abc-123");
        let library_id = "restored-lib-abc-123";

        let mut coven_config = coven::Config::with_defaults(
            library_id.to_string(),
            "restored-device".to_string(),
            StoreDir::new(library_path.clone()),
            "Test Library".to_string(),
        );
        coven_config.cloud_home.provider = Some(CloudProvider::CloudKit);
        coven_config.cloud_home.cloudkit_owner_name = Some("_owner".to_string());
        coven_config.cloud_home.cloudkit_zone_name = Some("bae-library".to_string());
        let config = Config::from_coven(coven_config);

        assert_eq!(config.store_id, library_id);
        assert_eq!(config.store_name, "Test Library");
        assert_eq!(config.mcp, McpConfig::disabled_default());
        assert_eq!(
            config.cloud_home.cloudkit_owner_name.as_deref(),
            Some("_owner")
        );
        assert_eq!(
            config.cloud_home.cloudkit_zone_name.as_deref(),
            Some("bae-library")
        );

        config.save_to_config_yaml().unwrap();

        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(library_path.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_id, library_id);
        assert_eq!(yaml.mcp, McpConfig::disabled_default());
        assert_eq!(
            yaml.cloud_home.cloudkit_owner_name.as_deref(),
            Some("_owner")
        );
        assert_eq!(
            yaml.cloud_home.cloudkit_zone_name.as_deref(),
            Some("bae-library")
        );
    }
}
