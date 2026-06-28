use crate::library_dir::LibraryDir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::watch;
use tracing::{debug, info, warn};

/// Initialize the keyring credential store.
///
/// On macOS, uses the protected data store with iCloud cloud-sync enabled,
/// so the encryption key is backed up via iCloud Keychain (if the user has it on).
///
/// Must be called once at startup before any keyring operations.
pub fn init_keyring() {
    // coven namespaces every key entry under the host app's identity, which the
    // host must set once before any keyring access — coven's getters panic
    // otherwise. "bae" keeps bae's coven key entries from colliding with any
    // other coven-based app on the same machine. Set-once, so it's safe to run
    // through every init path (bridge, windows-ffi, bae-core bootstrap, tests).
    coven::set_keyring_service("bae");

    #[cfg(target_os = "macos")]
    {
        use std::collections::HashMap;
        let config = HashMap::from([("cloud-sync", "true")]);
        match apple_native_keyring_store::protected::Store::new_with_configuration(&config) {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (protected store, iCloud sync enabled)");
            }
            Err(e) => {
                warn!("Failed to create protected keyring store: {e}, falling back to local");
                if let Ok(store) = apple_native_keyring_store::protected::Store::new() {
                    keyring_core::set_default_store(store);
                    info!("Keyring initialized (protected store, local only)");
                }
            }
        }
    }

    #[cfg(target_os = "android")]
    {
        if let Ok(store) = android_native_keyring_store::Store::new() {
            keyring_core::set_default_store(store);
            info!("Keyring initialized (Android keystore)");
        } else {
            warn!("Failed to create Android keyring store");
        }
    }

    #[cfg(target_os = "windows")]
    {
        match windows_native_keyring_store::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                info!("Keyring initialized (Windows Credential Manager)");
            }
            Err(e) => warn!("Failed to create Windows keyring store: {e}"),
        }
    }
}

/// Read a dev-mode `BAE_*` env var, distinguishing the three outcomes that
/// matter: absent (skip silently — the common case for an unset secret), present
/// but non-UTF-8 (a misconfigured `.env`, warned and skipped so it isn't
/// silently treated as absent), and present with a non-empty value (returned for
/// seeding). An empty value is treated as absent.
fn dev_env_secret(var: &str) -> Option<String> {
    match std::env::var(var) {
        Ok(value) if !value.is_empty() => Some(value),
        Ok(_) => None,
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(raw)) => {
            warn!("dev: ignoring non-UTF-8 {var}: {raw:?}");
            None
        }
    }
}

/// Bridge bae's dev-mode `BAE_*` env vars into coven's keyring for one library.
///
/// coven's `KeyService` reads secrets from the keyring. In dev mode bae's secrets
/// live in env vars (`.env` / `BAE_ENCRYPTION_KEY` / `BAE_CLOUD_HOME_CREDENTIALS`
/// / `BAE_DISCOGS_API_KEY`), so before coven reads them bae seeds each present
/// env value into the keyring account coven reads from — through coven's own
/// `KeyService` setters, not a hand-rolled keyring entry. Production is a no-op:
/// `is_dev_mode()` is false, so coven reads the OS keyring directly.
///
/// Call once after the keyring store is installed (`init_keyring`) and the
/// `library_id` is known, before constructing coven's `KeyService`.
pub fn seed_dev_keyring(library_id: &str) {
    if !Config::is_dev_mode() {
        return;
    }

    let keys = coven::KeyService::new(library_id.to_string());

    if let Some(key) = dev_env_secret("BAE_ENCRYPTION_KEY") {
        match keys.set_encryption_key(&key) {
            Ok(()) => info!("dev: seeded encryption key from env"),
            Err(e) => warn!("dev: failed to seed encryption key: {e}"),
        }
    }

    if let Some(creds_json) = dev_env_secret("BAE_CLOUD_HOME_CREDENTIALS") {
        match serde_json::from_str::<coven::CloudHomeCredentials>(&creds_json) {
            Ok(creds) => match keys.set_cloud_home_credentials(&creds) {
                Ok(()) => info!("dev: seeded cloud home credentials from env"),
                Err(e) => warn!("dev: failed to seed cloud home credentials: {e}"),
            },
            Err(e) => warn!("dev: ignoring malformed BAE_CLOUD_HOME_CREDENTIALS JSON: {e}"),
        }
    }

    // bae's own Discogs API key — a bae-domain credential with no coven setter,
    // written through bae's own keyring path (`BaeKeyServiceExt::set_discogs_key`).
    if let Some(discogs) = dev_env_secret("BAE_DISCOGS_API_KEY") {
        use crate::keys::BaeKeyServiceExt;
        match keys.set_discogs_key(&discogs) {
            Ok(()) => info!("dev: seeded discogs api key from env"),
            Err(e) => warn!("dev: failed to seed discogs api key: {e}"),
        }
    }
}

/// Install an in-memory keyring store and set coven's keyring service for tests.
///
/// coven's `KeyService` reads and writes the keyring instead of the environment,
/// and its getters panic unless the service is set. Tests don't run
/// `init_keyring` (which would install the OS store and prompt), so this is the
/// startup every test needing the keyring calls: an in-memory store stands in
/// for the OS keyring, and the service is set to "bae" to match production.
/// Genuinely set-once: the mock store is installed on the first call and kept
/// for the rest of the process. Replacing it on a later call (as this used to)
/// wipes entries other parallel tests already wrote — the store is one
/// process-global namespace — which flaked any test reading a key a sibling had
/// just stored. Entries stay isolated by library id (see the test-manager
/// setup) instead of by a fresh store per test.
#[cfg(any(test, feature = "test-utils"))]
pub fn install_test_keyring() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        keyring_core::set_default_store(
            keyring_core::mock::Store::new().expect("create mock keyring store"),
        );
        coven::set_keyring_service("bae");
    });
}

/// Cloud home provider selection. bae uses coven's enum directly — same
/// variants, same serialization, same `needs_oauth` — rather than maintaining a
/// duplicate it would have to keep mapping back and forth.
pub use coven::CloudProvider;

/// Parse an RFC 3339 timestamp into Unix epoch milliseconds. coven and bae's
/// own queue both store sync/created times as RFC 3339 text, but the UI only
/// needs an instant, so this is the one place that maps the text to epoch
/// millis. The parse result is returned so each caller decides how to handle a
/// value that won't parse (log-and-drop, or surface as a conversion error).
pub fn rfc3339_to_epoch_millis(s: &str) -> Result<i64, chrono::ParseError> {
    chrono::DateTime::parse_from_rfc3339(s).map(|dt| dt.timestamp_millis())
}

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
    pub fn from_coven(c: coven::Config) -> Self {
        let mut cfg = Self::with_defaults(
            c.library_id.clone(),
            c.device_id.clone(),
            c.library_dir.clone(),
            c.library_name.clone(),
        );
        cfg.inner = c;
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
    /// Whether playback pauses between vinyl/cassette sides.
    #[serde(default)]
    pub pause_between_sides: bool,
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
            pause_between_sides: self.pause_between_sides,
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
            pause_between_sides: config.pause_between_sides,
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
    /// Whether playback pauses between vinyl/cassette sides.
    pub pause_between_sides: bool,
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
    pub fn load(ids: &dyn crate::id_provider::IdProvider) -> Self {
        let dev_mode = std::env::var("BAE_DEV_MODE").is_ok() || {
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            {
                dotenvy::dotenv().is_ok()
            }
            #[cfg(any(target_os = "ios", target_os = "android"))]
            {
                false
            }
        };
        if dev_mode {
            info!("Dev mode activated — loading config.yaml with .env overrides");
            Self::from_env(ids)
        } else {
            info!("Production mode - loading from config.yaml");
            Self::from_config_file(ids)
        }
    }

    fn from_env(ids: &dyn crate::id_provider::IdProvider) -> Self {
        // Use the same active-library pointer file as production mode
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        let mut config = Self::load_from_bae_dir(&bae_dir, ids);

        // Overlay dev-specific env vars on top of the config.yaml values
        if let Some(path) = std::env::var("BAE_LIBRARY_PATH")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.library_dir = LibraryDir::new(PathBuf::from(path));
        }

        let encryption_key_hex = std::env::var("BAE_ENCRYPTION_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        if let Some(ref key) = encryption_key_hex {
            // `from_env` is infallible — a malformed env var is a dev setup
            // bug, not a runtime condition. Silently writing
            // `encryption_key_stored = true` with no fingerprint would let any
            // key later unlock the library (the fingerprint guard short-circuits
            // on None), so surface the parse failure loudly.
            let fingerprint = crate::encryption::EncryptionService::new(key)
                .expect("BAE_ENCRYPTION_KEY is malformed")
                .fingerprint();
            config.encryption_key_stored = true;
            config.encryption_key_fingerprint = Some(fingerprint);
        }

        if std::env::var("BAE_DISCOGS_API_KEY")
            .ok()
            .filter(|k| !k.is_empty())
            .is_some()
        {
            // In dev mode, assume the env-provided key is valid
            config.discogs = Some(DiscogsValidation::Valid);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_BUCKET")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home.s3_bucket = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_REGION")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home.s3_region = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_ENDPOINT")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home.s3_endpoint = Some(v);
        }

        if let Some(v) = std::env::var("BAE_CLOUD_HOME_S3_KEY_PREFIX")
            .ok()
            .filter(|s| !s.is_empty())
        {
            config.cloud_home.s3_key_prefix = Some(v);
        }

        config
    }

    fn from_config_file(ids: &dyn crate::id_provider::IdProvider) -> Self {
        let home_dir = dirs::home_dir().expect("Failed to get home directory");
        let bae_dir = home_dir.join(".bae");
        Self::load_from_bae_dir(&bae_dir, ids)
    }

    fn load_from_bae_dir(
        bae_dir: &std::path::Path,
        ids: &dyn crate::id_provider::IdProvider,
    ) -> Self {
        // Read active library UUID from pointer file
        let pointer_file = bae_dir.join("active-library");
        let active_id = read_active_library_id(bae_dir);

        let library_id = match active_id {
            Some(id) => id,
            None => {
                // No pointer file — auto-select the first known library
                let libraries = discover_all_library_paths(bae_dir);
                match libraries.into_iter().next() {
                    Some((_path, yaml)) => yaml.library_id,
                    None => panic!(
                        "No active-library pointer at {} and no libraries found. \
                         Run bae to set up a library.",
                        pointer_file.display()
                    ),
                }
            }
        };

        let library_dir = find_library_by_id(bae_dir, &library_id).unwrap_or_else(|| {
            panic!(
                "Library '{}' not found. The library may have been removed or its drive unmounted.",
                library_id
            )
        });

        // Read library-specific config — must exist with library_id (first-run flow creates it)
        let config_path = library_dir.config_path();
        let yaml_config: ConfigYaml =
            serde_yaml::from_str(&std::fs::read_to_string(&config_path).unwrap_or_else(|e| {
                panic!(
                    "No config.yaml at {}. Library may be corrupted. ({})",
                    config_path.display(),
                    e
                )
            }))
            .unwrap_or_else(|e| panic!("Failed to parse {}: {}", config_path.display(), e));

        // Auto-generate device_id if missing (first startup after upgrade)
        let device_id = match yaml_config.device_id.clone() {
            Some(id) => id,
            None => {
                let id = ids.new_id();

                info!("No device_id in config.yaml, generated: {}", id);
                let mut yaml_to_save = yaml_config.clone();
                yaml_to_save.device_id = Some(id.clone());
                if let Err(e) =
                    std::fs::write(&config_path, serde_yaml::to_string(&yaml_to_save).unwrap())
                {
                    warn!("Failed to save device_id to config.yaml: {e}");
                }
                id
            }
        };

        yaml_config.into_config(device_id, library_dir)
    }

    pub fn is_dev_mode() -> bool {
        std::env::var("BAE_DEV_MODE").is_ok() || std::path::Path::new(".env").exists()
    }

    /// Save the active library UUID to the global pointer file (~/.bae/active-library).
    pub fn save_active_library(&self) -> Result<(), ConfigError> {
        let bae_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".bae");
        std::fs::create_dir_all(&bae_dir)?;
        std::fs::write(bae_dir.join("active-library"), &self.library_id)?;
        Ok(())
    }

    pub fn save_to_config_yaml(&self) -> Result<(), ConfigError> {
        std::fs::create_dir_all(&*self.library_dir)?;
        let yaml: ConfigYaml = self.into();
        std::fs::write(
            self.library_dir.config_path(),
            serde_yaml::to_string(&yaml).unwrap(),
        )?;
        Ok(())
    }

    /// Construct a Config with defaults for a new library.
    pub fn with_defaults(
        library_id: String,
        device_id: String,
        library_dir: LibraryDir,
        library_name: String,
    ) -> Self {
        Self {
            inner: coven::Config::with_defaults(
                library_id,
                device_id,
                library_dir,
                library_name,
            ),
            discogs: None,
            replay_gain_mode: default_replay_gain_mode(),
            pause_between_sides: false,
        }
    }

    /// Discover all libraries under ~/.bae/libraries/.
    pub fn discover_libraries() -> Vec<LibraryInfo> {
        let home_dir = match dirs::home_dir() {
            Some(d) => d,
            None => return vec![],
        };
        let bae_dir = home_dir.join(".bae");
        let active_id = read_active_library_id(&bae_dir);

        let mut libraries: Vec<LibraryInfo> = discover_all_library_paths(&bae_dir)
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

        libraries
    }
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
        let mut config = self.config().clone();
        edit(&mut config);
        config.save_to_config_yaml()?;
        self.state.send_replace(config);
        Ok(())
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
    std::fs::write(&config_path, serialized)?;
    Ok(())
}

/// Read the active library UUID from `~/.bae/active-library`, if it exists.
fn read_active_library_id(bae_dir: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(bae_dir.join("active-library"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

/// Absolute path to a library's data directory by its UUID, scanning
/// `~/.bae/libraries/`. `None` if no directory there carries that id. Used by
/// `forget_library` to locate the tree it removes.
pub fn library_data_dir(bae_dir: &std::path::Path, library_id: &str) -> Option<PathBuf> {
    discover_all_library_paths(bae_dir)
        .into_iter()
        .find(|(_, yaml)| yaml.library_id == library_id)
        .map(|(path, _)| path)
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
    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(ConfigError::Io(e)),
    };
    let yaml =
        serde_yaml::from_str(&content).map_err(|e| ConfigError::Serialization(e.to_string()))?;
    Ok(Some(yaml))
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn config_yaml_requires_library_id() {
        let yaml = "library_name: Test\n";
        let result: Result<ConfigYaml, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err(), "ConfigYaml should fail without library_id");
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

    /// `rfc3339_to_epoch_millis` is the single conversion the sync-time and
    /// outbox-row paths share. A fractional-second timestamp keeps its
    /// sub-second precision, a plain whole-second one converts cleanly, and an
    /// unparseable string surfaces the parse error rather than a wrong instant.
    #[test]
    fn rfc3339_to_epoch_millis_handles_fractional_and_invalid() {
        assert_eq!(
            rfc3339_to_epoch_millis("2024-01-02T03:04:05Z").unwrap(),
            1_704_164_645_000,
        );
        assert_eq!(
            rfc3339_to_epoch_millis("2024-01-02T03:04:05.250Z").unwrap(),
            1_704_164_645_250,
        );
        assert!(rfc3339_to_epoch_millis("not a timestamp").is_err());
    }

    #[test]
    fn config_yaml_parses_with_library_id() {
        let yaml = "library_id: abc-123\nlibrary_name: Test\n";
        let config: ConfigYaml = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.library_id, "abc-123");
        assert_eq!(config.library_name, "Test");
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

        let loaded = Config::load_from_bae_dir(
            bae_dir,
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
        assert_eq!(loaded.library_id, library_id);
        assert_eq!(&*loaded.library_dir, library_path.as_path());
    }

    #[test]
    fn load_from_bae_dir_auto_selects_first_library_without_pointer() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("auto-lib");

        // Create a library in libraries/ but no active-library pointer
        make_test_config("auto-lib", library_path.clone())
            .save_to_config_yaml()
            .unwrap();

        let loaded = Config::load_from_bae_dir(
            bae_dir,
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
        assert_eq!(loaded.library_id, "auto-lib");
    }

    #[test]
    #[should_panic(expected = "no libraries found")]
    fn load_from_bae_dir_panics_without_pointer_or_libraries() {
        let tmp = TempDir::new().unwrap();
        Config::load_from_bae_dir(
            tmp.path(),
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_when_library_id_not_found() {
        let tmp = TempDir::new().unwrap();
        // Pointer to a UUID that doesn't exist anywhere
        std::fs::write(tmp.path().join("active-library"), "nonexistent-uuid").unwrap();
        Config::load_from_bae_dir(
            tmp.path(),
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
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
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_when_dir_exists_but_no_config_yaml() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        // Dir exists but no config.yaml — library is invisible to find_library_by_id
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        Config::load_from_bae_dir(
            bae_dir,
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
    }

    #[test]
    #[should_panic(expected = "not found")]
    fn load_from_bae_dir_panics_on_unparseable_config() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_path = bae_dir.join("libraries").join("some-id");
        std::fs::create_dir_all(&library_path).unwrap();
        // config.yaml exists but missing library_id — invisible to find_library_by_id
        std::fs::write(library_path.join("config.yaml"), "library_name: Test\n").unwrap();
        std::fs::write(bae_dir.join("active-library"), "some-id").unwrap();

        Config::load_from_bae_dir(
            bae_dir,
            &crate::id_provider::SequentialIdProvider::new("device"),
        );
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
    fn create_library_preserves_library_id() {
        let tmp = TempDir::new().unwrap();
        let bae_dir = tmp.path();
        let library_id = "restored-lib-abc-123";

        let config = LibraryDir::create(
            bae_dir,
            library_id.to_string(),
            "Test Library".to_string(),
            &crate::id_provider::SequentialIdProvider::new("device"),
        )
        .unwrap();

        // The returned config must use the provided library_id, not a new UUID.
        assert_eq!(config.library_id, library_id);
        assert_eq!(config.library_name, "Test Library");

        // Device ID should be a new UUID (different from library_id).
        assert_ne!(config.device_id, library_id);
        assert!(!config.device_id.is_empty());

        // Directory should be created under bae_dir/libraries/<library_id>.
        let expected_dir = bae_dir.join("libraries").join(library_id);
        assert_eq!(&*config.library_dir, expected_dir.as_path());
        assert!(expected_dir.exists());

        // config.yaml should be persisted with the correct library_id.
        let yaml: ConfigYaml = serde_yaml::from_str(
            &std::fs::read_to_string(expected_dir.join("config.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(yaml.library_id, library_id);
    }
}
