use coven::LibraryDir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::watch;
use tracing::{debug, info, warn};

mod atomic_write;
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

use atomic_write::{write_atomic, write_atomic_io};
use dev::{dev_env_secret, dev_mode_enabled, overlay_cloud_home_s3_env};
use export::{
    default_export_filename_template, default_export_location, default_export_presets,
    default_export_selection,
};

pub const MCP_DEFAULT_PORT: u16 = 47777;

/// Cloud home provider selection. bae uses coven's enum directly — same
/// variants, same serialization, same `needs_oauth` — rather than maintaining a
/// duplicate it would have to keep mapping back and forth.
pub use coven::CloudProvider;

/// Cloud home settings (provider + per-provider fields). bae uses coven's type
/// directly — same fields, extracted from bae — instead of a parallel copy.
pub use coven::CloudHomeConfig;

/// How a cloud home stores its objects: opaque (encrypted, obfuscated blob
/// paths) or browsable (stored in the clear at readable paths). The host picks
/// this when creating a cloud home; it drives both encryption-at-rest and the
/// blob-path scheme. Not access control — the provider's own credentials gate
/// the bucket either way; this is only about whether what's stored is legible.
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

/// The serde default for `ConfigYaml.replay_gain_mode`. Enums carry no
/// `#[derive(Default)]` in this project, so the default is named explicitly.
fn default_replay_gain_mode() -> ReplayGainMode {
    ReplayGainMode::Off
}

/// The serde default for `ConfigYaml.verify_decode_on_import`: on. Import fully
/// decodes every track for loudness anyway, so the verify rides that pass for
/// free; defaulting on means a truncated/corrupt track is caught at import
/// instead of failing at play time.
fn default_verify_decode_on_import() -> bool {
    true
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
            c.library_id.clone(),
            c.device_id.clone(),
            c.library_dir.clone(),
            c.library_name.clone(),
        );
        cfg.inner = c;
        cfg.mcp = McpConfig::disabled_default();
        cfg
    }
}

/// Configuration errors. bae uses coven's enum directly — identical, and it was
/// extracted from bae.
pub use coven::ConfigError;

/// bae's application directory (`~/.bae`). The base coven's restore/join build
/// per-library dirs under (`<app_dir>/libraries/<id>`).
pub fn bae_dir() -> Result<std::path::PathBuf, ConfigError> {
    Ok(dirs::home_dir()
        .ok_or_else(|| ConfigError::Config("could not determine home directory".to_string()))?
        .join(".bae"))
}

#[derive(Debug)]
enum ConfigWriteError {
    BeforeCommit(ConfigError),
    AfterCommit(ConfigError),
}

impl ConfigWriteError {
    fn committed(&self) -> bool {
        matches!(self, Self::AfterCommit(_))
    }

    fn into_config_error(self) -> ConfigError {
        match self {
            Self::BeforeCommit(e) | Self::AfterCommit(e) => e,
        }
    }
}

/// YAML config file structure for non-secret settings (per-library)
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
    #[serde(default)]
    pub discogs: Option<DiscogsValidation>,
    /// Whether an encryption key is stored in the keyring (hint flag, avoids keyring read)
    #[serde(default)]
    pub encryption_key_stored: bool,
    /// SHA-256 fingerprint of the encryption key (first 8 bytes, hex).
    /// Used to detect wrong key without attempting decryption.
    #[serde(default)]
    pub encryption_key_fingerprint: Option<String>,
    /// How loudness normalization is applied at playback. Defaults to `Off`.
    #[serde(default = "default_replay_gain_mode")]
    pub replay_gain_mode: ReplayGainMode,
    /// Where release exports write. Defaults to prompting each time.
    #[serde(default = "default_export_location")]
    pub export_location: ExportLocation,
    /// Template for the default filename a single-track export suggests.
    #[serde(default = "default_export_filename_template")]
    pub export_filename_template: String,
    /// Configured export presets offered by release and track export.
    #[serde(default = "default_export_presets")]
    pub export_presets: Vec<ExportPreset>,
    /// Default selected option in the track export picker.
    #[serde(default = "default_export_selection")]
    pub default_track_export_selection: ExportSelection,
    /// Default selected option in the release export picker.
    #[serde(default = "default_export_selection")]
    pub default_release_export_selection: ExportSelection,
    /// Whether playback pauses between vinyl/cassette sides.
    #[serde(default)]
    pub pause_between_sides: bool,
    /// Whether import fully decodes each track to verify it (fatal-error / frame
    /// shortfall), failing the import for a broken track rather than importing it
    /// and failing at play time. Rides the loudness decode, so it adds no work.
    #[serde(default = "default_verify_decode_on_import")]
    pub verify_decode_on_import: bool,
    /// Local automation server configuration. Required so missing/stale config
    /// files fail to load instead of silently taking an implicit value.
    pub mcp: McpConfig,
    /// Cloud home provider + per-provider settings. Flattened so the on-disk
    /// keys sit at the top level. bae uses coven's type — same fields, extracted
    /// from bae — instead of a parallel copy it would map back and forth.
    #[serde(default, flatten)]
    pub cloud_home: CloudHomeConfig,
}

impl ConfigYaml {
    /// Convert to a runtime Config. The caller resolves device_id (auto-generating
    /// if missing from YAML) and provides the library_dir.
    fn into_config(self, device_id: String, library_dir: LibraryDir) -> Config {
        Config {
            inner: coven::Config {
                library_id: self.library_id,
                device_id,
                library_dir,
                library_name: self.library_name,
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
            verify_decode_on_import: self.verify_decode_on_import,
            mcp: self.mcp,
        }
    }
}

impl From<&Config> for ConfigYaml {
    fn from(config: &Config) -> Self {
        Self {
            library_id: config.library_id.clone(),
            library_name: config.library_name.clone(),
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
            verify_decode_on_import: config.verify_decode_on_import,
            mcp: config.mcp,
            cloud_home: config.cloud_home.clone(),
        }
    }
}

/// Metadata about a discovered library (for the library switcher UI)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryInfo {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub is_active: bool,
    pub cloud_provider: Option<CloudProvider>,
}

/// Application configuration.
///
/// Holds coven's sync/cloud config (`inner`) plus bae's own fields (Discogs).
/// `Deref`/`DerefMut` expose the coven fields/methods directly, so
/// `config.library_id`, `config.cloud_home.provider = …`,
/// `config.sync_enabled(…)` all work unchanged.
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
    pub fn load(ids: &dyn coven::IdProvider) -> Result<Self, ConfigError> {
        if dev_mode_enabled() {
            info!("Dev mode activated — loading config.yaml with .env overrides");
            Self::from_env(ids)
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file(ids)
        }
    }

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
        Self::load_from_library_dir(LibraryDir::new(library_path), ids)
    }

    fn from_env(ids: &dyn coven::IdProvider) -> Result<Self, ConfigError> {
        // Use the same active-library pointer file as production mode
        let app_dir = bae_dir()?;
        let mut config = Self::load_from_bae_dir(&app_dir, ids)?;

        // Overlay dev-specific env vars on top of the config.yaml values
        if let Some(path) = dev_env_secret("BAE_LIBRARY_PATH") {
            config.library_dir = LibraryDir::new(PathBuf::from(path));
        }

        let encryption_key_hex = dev_env_secret("BAE_ENCRYPTION_KEY");
        if let Some(ref key) = encryption_key_hex {
            // `from_env` is infallible — a malformed env var is a dev setup
            // bug, not a runtime condition. Silently writing
            // `encryption_key_stored = true` with no fingerprint would let any
            // key later unlock the library (the fingerprint guard short-circuits
            // on None), so surface the parse failure loudly.
            let fingerprint = coven::EncryptionService::new(key)
                .expect("BAE_ENCRYPTION_KEY is malformed")
                .fingerprint();
            config.encryption_key_stored = true;
            config.encryption_key_fingerprint = Some(fingerprint);
        }

        if dev_env_secret("BAE_DISCOGS_API_KEY").is_some() {
            // In dev mode, assume the env-provided key is valid
            config.discogs = Some(DiscogsValidation::Valid);
        }

        overlay_cloud_home_s3_env(&mut config.cloud_home);

        Ok(config)
    }

    fn from_config_file(ids: &dyn coven::IdProvider) -> Result<Self, ConfigError> {
        let app_dir = bae_dir()?;
        Self::load_from_bae_dir(&app_dir, ids)
    }

    fn load_from_bae_dir(
        bae_dir: &std::path::Path,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        // Read active library UUID from pointer file
        let pointer_file = bae_dir.join("active-library");
        let active_id = read_active_library_id(bae_dir).map_err(|e| {
            ConfigError::Config(format!(
                "failed to read active-library pointer at {}: {e}",
                pointer_file.display()
            ))
        })?;

        let library_id = match active_id {
            Some(id) => id,
            None => {
                // No pointer file — auto-select the first known library
                let libraries = discover_all_library_paths(bae_dir);
                match libraries.into_iter().next() {
                    Some((_path, yaml)) => yaml.library_id,
                    None => {
                        return Err(ConfigError::Config(format!(
                            "no active-library pointer at {} and no libraries found",
                            pointer_file.display()
                        )));
                    }
                }
            }
        };

        Self::load_registered_library_from_bae_dir(bae_dir, &library_id, ids)
            .map_err(|e| ConfigError::Config(format!("failed to load library config: {e}")))
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
        library_dir: LibraryDir,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let config_path = library_dir.config_path();
        let yaml_config = Self::load_config_yaml(&config_path)?;
        Self::config_from_yaml(yaml_config, library_dir, &config_path, ids)
    }

    fn load_from_registered_library_dir(
        library_dir: LibraryDir,
        expected_library_id: &str,
        ids: &dyn coven::IdProvider,
    ) -> Result<Self, ConfigError> {
        let config_path = library_dir.config_path();
        let yaml_config = load_registered_config_yaml(&library_dir, expected_library_id)?;
        Self::config_from_yaml(yaml_config, library_dir, &config_path, ids)
    }

    fn load_config_yaml(config_path: &std::path::Path) -> Result<ConfigYaml, ConfigError> {
        let content = std::fs::read_to_string(config_path)?;
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))
    }

    fn config_from_yaml(
        mut yaml_config: ConfigYaml,
        library_dir: LibraryDir,
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
                write_atomic_io(config_path, serialized.as_bytes())?;
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
        write_atomic_io(&pointer_path, self.library_id.as_bytes())?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        self.write_config_yaml()
            .map_err(ConfigWriteError::into_config_error)
    }

    fn write_config_yaml(&self) -> Result<(), ConfigWriteError> {
        std::fs::create_dir_all(&*self.library_dir)
            .map_err(ConfigError::from)
            .map_err(ConfigWriteError::BeforeCommit)?;
        let yaml: ConfigYaml = self.into();
        let serialized = serde_yaml::to_string(&yaml)
            .map_err(|e| ConfigError::Serialization(e.to_string()))
            .map_err(ConfigWriteError::BeforeCommit)?;
        write_atomic(&self.library_dir.config_path(), serialized.as_bytes()).map_err(|e| {
            let committed = e.committed();
            let config_error = ConfigError::from(e.into_io());
            if committed {
                ConfigWriteError::AfterCommit(config_error)
            } else {
                ConfigWriteError::BeforeCommit(config_error)
            }
        })
    }

    /// Construct a Config with defaults for a new library.
    pub fn with_defaults(
        library_id: String,
        device_id: String,
        library_dir: LibraryDir,
        library_name: String,
    ) -> Self {
        Self {
            inner: coven::Config::with_defaults(library_id, device_id, library_dir, library_name),
            discogs: None,
            replay_gain_mode: default_replay_gain_mode(),
            export_location: default_export_location(),
            export_filename_template: default_export_filename_template(),
            export_presets: default_export_presets(),
            default_track_export_selection: default_export_selection(),
            default_release_export_selection: default_export_selection(),
            pause_between_sides: false,
            verify_decode_on_import: default_verify_decode_on_import(),
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
        .map(|(path, yaml)| {
            let is_active = active_id.as_deref() == Some(&yaml.library_id);
            LibraryInfo {
                id: yaml.library_id,
                name: yaml.library_name,
                path,
                is_active,
                cloud_provider: yaml.cloud_home.provider.clone(),
            }
        })
        .collect();

    // Sort: active first, then by name/id
    libraries.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
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
                    save_err = Some(e.into_config_error());
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

    /// Rename the library.
    pub fn rename_library(&self, name: &str) -> Result<(), ConfigError> {
        if name.is_empty() {
            return Err(ConfigError::Config(
                "Library name cannot be empty".to_string(),
            ));
        }
        self.update(|c| c.library_name = name.to_string())
    }
}

/// Rename a library by id without loading it into memory: locate its
/// directory, read its `config.yaml`, replace `library_name`, write
/// back. Used by `LibraryManager::rename_library` for libraries that
/// aren't the currently-active one (where the reactive `ConfigState`
/// handles the rename through its normal save path).
pub fn rename_inactive_library(
    bae_dir: &std::path::Path,
    library_id: &str,
    new_name: &str,
) -> Result<(), ConfigError> {
    let library_dir = find_library_by_id(bae_dir, library_id)
        .ok_or_else(|| ConfigError::Config(format!("library not found: {library_id}")))?;
    let config_path = library_dir.config_path();
    let content = std::fs::read_to_string(&config_path)?;
    let mut yaml: ConfigYaml =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    yaml.library_name = new_name.to_string();
    let serialized =
        serde_yaml::to_string(&yaml).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    write_atomic_io(&config_path, serialized.as_bytes())?;
    Ok(())
}

pub(crate) fn registered_library_path(bae_dir: &std::path::Path, library_id: &str) -> PathBuf {
    bae_dir.join("libraries").join(library_id)
}

fn registered_library_dir(bae_dir: &std::path::Path, library_id: &str) -> LibraryDir {
    LibraryDir::new(registered_library_path(bae_dir, library_id))
}

fn load_registered_config_yaml(
    library_dir: &LibraryDir,
    expected_library_id: &str,
) -> Result<ConfigYaml, ConfigError> {
    let yaml_config = Config::load_config_yaml(&library_dir.config_path())?;
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
fn find_library_by_id(bae_dir: &std::path::Path, uuid: &str) -> Option<LibraryDir> {
    for (path, yaml) in discover_all_library_paths(bae_dir) {
        if yaml.library_id == uuid {
            return Some(LibraryDir::new(path));
        }
    }
    None
}

/// Collect all (path, ConfigYaml) pairs from ~/.bae/libraries/.
fn discover_all_library_paths(bae_dir: &std::path::Path) -> Vec<(PathBuf, ConfigYaml)> {
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
                Ok(Some(yaml)) => results.push((path, yaml)),
                Ok(None) => {
                    debug!(
                        "skipping library dir with no config.yaml: {}",
                        path.display()
                    );
                }
                Err(e) => {
                    warn!("skipping library at {}: {e}", path.display());
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
    let yaml =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    Ok(Some(yaml))
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
            LibraryDir::new(library_path),
            "Test Library".to_string(),
        )
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
    fn export_settings_default_when_absent_from_yaml() {
        // A config.yaml with neither export field loads the named defaults
        // rather than failing — the greenfield defaults ride serde.
        let yaml = "library_id: abc-123\nlibrary_name: Test\nstorage: opaque\nmcp:\n  enabled: false\n  port: 47777\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.export_filename_template,
            default_export_filename_template()
        );
        assert_eq!(config.export_presets, default_export_presets());
        assert_eq!(
            config.default_track_export_selection,
            default_export_selection()
        );
        assert_eq!(
            config.default_release_export_selection,
            default_export_selection()
        );
    }

    #[test]
    fn config_yaml_requires_library_id() {
        let yaml = "library_name: Test\nmcp:\n  enabled: false\n  port: 47777\n";
        let result: Result<ConfigYaml, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "ConfigYaml should fail without library_id");
    }

    #[test]
    fn config_yaml_requires_mcp() {
        let yaml = "library_id: abc-123\nlibrary_name: Test\n";
        let result: Result<ConfigYaml, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "ConfigYaml should fail without mcp");
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
    fn config_yaml_parses_with_library_id() {
        let yaml = "library_id: abc-123\nlibrary_name: Test\nstorage: opaque\nmcp:\n  enabled: false\n  port: 47777\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.library_id, "abc-123");
        assert_eq!(config.library_name, "Test");
        assert_eq!(config.mcp, McpConfig::disabled_default());
    }

    #[test]
    fn config_yaml_requires_storage() {
        // `storage` rides the flattened coven CloudHomeConfig and carries no
        // serde default: a config file without it fails to load rather than
        // silently assuming a cipher/path scheme.
        let yaml =
            "library_id: abc-123\nlibrary_name: Test\nmcp:\n  enabled: false\n  port: 47777\n";
        let result: Result<ConfigYaml, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "ConfigYaml should fail without storage");
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

        assert_eq!(loaded.library_id, "external-lib-id");
        assert_eq!(&*loaded.library_dir, library_path.as_path());
    }

    #[test]
    fn load_from_registered_library_dir_rejects_mismatched_config_id() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().join("libraries").join("expected-lib-id");
        make_test_config("wrong-lib-id", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let result = Config::load_from_registered_library_dir(
            LibraryDir::new(library_path),
            "expected-lib-id",
            &coven::SequentialIdProvider::new("device"),
        );

        assert!(matches!(result, Err(ConfigError::Config(_))));
    }

    #[test]
    fn load_from_bae_dir_reads_pointer_and_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_id = "test-lib-id";
        let library_path = bae_dir.join("libraries").join(library_id);

        // Set up active-library pointer (UUID) + config.yaml
        let config = make_test_config(library_id, library_path.clone());
        config.save_to_config_yaml().unwrap();
        std::fs::write(bae_dir.join("active-library"), library_id).unwrap();

        let loaded =
            Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
                .unwrap();
        assert_eq!(loaded.library_id, library_id);
        assert_eq!(&*loaded.library_dir, library_path.as_path());
    }

    #[test]
    fn load_from_bae_dir_auto_selects_first_library_without_pointer() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = registered_library_path(bae_dir, "auto-lib");

        // Create a library in libraries/ but no active-library pointer
        make_test_config("auto-lib", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let loaded =
            Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
                .unwrap();
        assert_eq!(loaded.library_id, "auto-lib");
    }

    #[test]
    fn load_from_bae_dir_returns_active_pointer_read_error() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = registered_library_path(bae_dir, "auto-lib");

        make_test_config("auto-lib", library_path)
            .save_to_config_yaml()
            .unwrap();
        std::fs::create_dir(bae_dir.join("active-library")).unwrap();

        let err = Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
            .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("active-library pointer"));
    }

    #[test]
    fn load_from_bae_dir_returns_error_when_active_pointer_is_empty() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = registered_library_path(bae_dir, "auto-lib");

        make_test_config("auto-lib", library_path)
            .save_to_config_yaml()
            .unwrap();
        std::fs::write(bae_dir.join("active-library"), " \n").unwrap();

        let err = Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
            .unwrap_err();

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

    #[test]
    fn load_from_bae_dir_errors_for_misplaced_library_id() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let misplaced_path = registered_library_path(bae_dir, "other-dir");

        make_test_config("target-lib", misplaced_path)
            .save_to_config_yaml()
            .unwrap();
        std::fs::write(bae_dir.join("active-library"), "target-lib").unwrap();

        let err = Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
            .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("failed to load library config"));
    }

    #[test]
    fn load_from_bae_dir_errors_without_pointer_or_libraries() {
        let tmp = TempDir::new().unwrap();
        let err =
            Config::load_from_bae_dir(tmp.path(), &coven::SequentialIdProvider::new("device"))
                .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("no libraries found"));
    }

    #[test]
    fn load_from_bae_dir_errors_when_library_id_not_found() {
        let tmp = TempDir::new().unwrap();
        // Pointer to a UUID that doesn't exist anywhere
        std::fs::write(tmp.path().join("active-library"), "nonexistent-uuid").unwrap();
        let err =
            Config::load_from_bae_dir(tmp.path(), &coven::SequentialIdProvider::new("device"))
                .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("failed to load library config"));
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
        assert_eq!(discovered[0].1.library_id, "valid-lib");
    }

    #[test]
    fn load_from_bae_dir_errors_when_dir_exists_but_no_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        let err = Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
            .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("failed to load library config"));
    }

    #[test]
    fn load_from_bae_dir_errors_on_unparseable_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        std::fs::write(library_path.join("config.yaml"), "library_name: Test\n").unwrap();
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        let err = Config::load_from_bae_dir(bae_dir, &coven::SequentialIdProvider::new("device"))
            .unwrap_err();

        assert!(matches!(err, ConfigError::Config(_)));
        assert!(err.to_string().contains("failed to load library config"));
    }

    #[test]
    fn library_name_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let library_path = tmp.path().to_path_buf();
        let mut config = make_test_config("lib-1", library_path.clone());
        config.library_name = "My Music".to_string();
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
        lib2.library_name = "Second Library".to_string();
        lib2.encryption_key_fingerprint = Some("fingerprint-2".to_string());
        lib2.save_to_config_yaml().unwrap();

        // Create an invalid dir (no config.yaml)
        std::fs::create_dir_all(libraries_dir.join("invalid")).unwrap();

        let discovered = discover_all_library_paths(bae_dir);
        assert_eq!(discovered.len(), 2);

        let ids: Vec<&str> = discovered
            .iter()
            .map(|(_, y)| y.library_id.as_str())
            .collect();
        assert!(ids.contains(&"lib-1"));
        assert!(ids.contains(&"lib-2"));

        let lib2_entry = discovered
            .iter()
            .find(|(_, y)| y.library_id == "lib-2")
            .unwrap();
        assert_eq!(lib2_entry.1.library_name, "Second Library");
        assert_eq!(
            lib2_entry.1.encryption_key_fingerprint.as_deref(),
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

        handle.rename_library("New Name").unwrap();
        assert_eq!(handle.config().library_name, "New Name");
        assert!(handle.rename_library("").is_err());

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
            config.library_name = "Renamed Library".to_string();
        });
        let playback = spawn_update(Arc::clone(&handle), Arc::clone(&start), |config| {
            config.pause_between_sides = true;
        });

        start.wait();
        rename.join().unwrap();
        playback.join().unwrap();

        let final_config = handle.config().clone();
        assert_eq!(final_config.library_name, "Renamed Library");
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
            LibraryDir::new(library_path.clone()),
            "Test Library".to_string(),
        );
        coven_config.cloud_home.provider = Some(CloudProvider::CloudKit);
        coven_config.cloud_home.cloudkit_share_url = Some("https://icloud.com/share".to_string());
        coven_config.cloud_home.cloudkit_owner_name = Some("_owner".to_string());
        coven_config.cloud_home.cloudkit_zone_name = Some("bae-library".to_string());
        let config = Config::from_coven(coven_config);

        assert_eq!(config.library_id, library_id);
        assert_eq!(config.library_name, "Test Library");
        assert_eq!(config.mcp, McpConfig::disabled_default());
        assert_eq!(
            config.cloud_home.cloudkit_share_url.as_deref(),
            Some("https://icloud.com/share")
        );
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
            yaml.cloud_home.cloudkit_share_url.as_deref(),
            Some("https://icloud.com/share")
        );
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
